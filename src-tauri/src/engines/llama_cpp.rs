use std::{
    collections::{BTreeMap, HashMap},
    net::{Ipv4Addr, TcpListener},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use reqwest::{header::CONTENT_TYPE, redirect, Client, Response, StatusCode};
use serde_json::Value;
use tokio::{
    sync::{watch, Mutex},
    time::sleep,
};
use uuid::Uuid;

use crate::{
    errors::{AppError, AppResult},
    processes::{EngineLifecycle, ProcessManager, ProcessSummary, SpawnOptions},
};

use super::traits::{
    ChatEngine, ChatRequest, EngineCapabilities, EngineConfig, EngineHandle, EngineHealth,
    EngineStartRequest, InferenceEngine, TokenSink, Usage,
};

const STARTUP_TIMEOUT: Duration = Duration::from_secs(120);
const HTTP_TIMEOUT: Duration = Duration::from_secs(2);
const GENERATION_TIMEOUT: Duration = Duration::from_secs(300);
const MAX_HTTP_BODY_BYTES: usize = 1024 * 1024;
const MAX_CHAT_REQUEST_BYTES: usize = 1024 * 1024;
const MAX_STREAM_LINE_BYTES: usize = 1024 * 1024;
const MAX_GENERATED_TEXT_BYTES: usize = 4 * 1024 * 1024;
const MAX_PORT_ATTEMPTS: usize = 3;

#[derive(Clone)]
struct PreparedConfig {
    executable: PathBuf,
    expected_version: String,
    environment: BTreeMap<String, String>,
}

#[derive(Clone)]
struct ActiveSession {
    session_id: String,
    process_id: String,
    endpoint: String,
    api_key: String,
    model_path: PathBuf,
    backend_version: String,
}

#[derive(Default)]
struct AdapterState {
    prepared: Option<PreparedConfig>,
    session: Option<ActiveSession>,
}

#[derive(Debug, Clone)]
pub(crate) struct LlamaCppSnapshot {
    pub session_id: String,
    pub process_id: String,
    pub model_path: PathBuf,
    pub backend_version: String,
    pub process: ProcessSummary,
}

pub struct LlamaCppEngine {
    processes: Arc<ProcessManager>,
    client: Client,
    generation_client: Client,
    state: Mutex<AdapterState>,
    active_jobs: Mutex<HashMap<String, watch::Sender<bool>>>,
}

impl LlamaCppEngine {
    pub fn new(processes: Arc<ProcessManager>) -> AppResult<Self> {
        let client = Client::builder()
            .no_proxy()
            .connect_timeout(HTTP_TIMEOUT)
            .timeout(HTTP_TIMEOUT)
            .redirect(redirect::Policy::none())
            .build()
            .map_err(|error| {
                AppError::Engine(format!(
                    "the llama.cpp health client could not start: {error}"
                ))
            })?;
        let generation_client = Client::builder()
            .no_proxy()
            .connect_timeout(HTTP_TIMEOUT)
            .timeout(GENERATION_TIMEOUT)
            .redirect(redirect::Policy::none())
            .build()
            .map_err(|error| {
                AppError::Engine(format!(
                    "the llama.cpp generation client could not start: {error}"
                ))
            })?;
        Ok(Self {
            processes,
            client,
            generation_client,
            state: Mutex::new(AdapterState::default()),
            active_jobs: Mutex::new(HashMap::new()),
        })
    }

    pub(crate) async fn snapshot(&self) -> Option<LlamaCppSnapshot> {
        let session = self.state.lock().await.session.clone()?;
        let process = self.processes.summary(&session.process_id).await?;
        Some(LlamaCppSnapshot {
            session_id: session.session_id,
            process_id: session.process_id,
            model_path: session.model_path,
            backend_version: session.backend_version,
            process,
        })
    }

    pub(crate) async fn logs(&self, session_id: &str) -> AppResult<(String, Vec<String>)> {
        let session = self
            .state
            .lock()
            .await
            .session
            .clone()
            .ok_or_else(|| AppError::Engine("there is no retained llama.cpp session".into()))?;
        if session.session_id != session_id {
            return Err(AppError::Engine(format!(
                "llama.cpp session {session_id} is not retained"
            )));
        }
        Ok((
            session.process_id.clone(),
            self.processes.logs(&session.process_id).await,
        ))
    }

    pub(crate) async fn active_job_count(&self) -> usize {
        self.active_jobs.lock().await.len()
    }

    async fn wait_until_ready(&self, session: &ActiveSession) -> AppResult<()> {
        let deadline = tokio::time::Instant::now() + STARTUP_TIMEOUT;
        loop {
            let summary = self
                .processes
                .summary(&session.process_id)
                .await
                .ok_or_else(|| AppError::Engine("the llama.cpp process disappeared".into()))?;
            if summary.state.is_terminal() {
                return Err(self.process_exit_error(session, &summary).await);
            }

            match self
                .client
                .get(format!("{}/health", session.endpoint))
                .send()
                .await
            {
                Ok(response) if response.status().is_success() => {
                    self.verify_identity(session, true).await?;
                    return Ok(());
                }
                Ok(response) if response.status() == StatusCode::SERVICE_UNAVAILABLE => {}
                Ok(response) => {
                    return Err(AppError::Engine(format!(
                        "llama.cpp health returned HTTP {}",
                        response.status()
                    )));
                }
                Err(_) => {}
            }

            if tokio::time::Instant::now() >= deadline {
                return Err(AppError::Engine(
                    "llama.cpp did not become ready within 120 seconds".into(),
                ));
            }
            sleep(Duration::from_millis(200)).await;
        }
    }

    async fn verify_identity(
        &self,
        session: &ActiveSession,
        require_auth_challenge: bool,
    ) -> AppResult<()> {
        let props_url = format!("{}/props", session.endpoint);
        if require_auth_challenge {
            let challenge = self
                .client
                .get(&props_url)
                .bearer_auth("neuraloc-invalid-ownership-token")
                .send()
                .await
                .map_err(|error| {
                    AppError::Engine(format!("llama.cpp ownership challenge failed: {error}"))
                })?;
            if !matches!(
                challenge.status(),
                StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
            ) {
                return Err(AppError::Engine(
                    "the loopback endpoint did not enforce the session ownership token".into(),
                ));
            }
        }

        let response = self
            .client
            .get(props_url)
            .bearer_auth(&session.api_key)
            .send()
            .await
            .map_err(|error| {
                AppError::Engine(format!("llama.cpp identity probe failed: {error}"))
            })?;
        if !response.status().is_success() {
            return Err(AppError::Engine(format!(
                "llama.cpp identity probe returned HTTP {}",
                response.status()
            )));
        }
        let body = bounded_body(response).await?;
        let props: Value = serde_json::from_slice(&body).map_err(|error| {
            AppError::Engine(format!("llama.cpp returned invalid identity JSON: {error}"))
        })?;
        validate_props_identity(&props, &session.model_path, &session.backend_version)
    }

    async fn process_exit_error(
        &self,
        session: &ActiveSession,
        summary: &ProcessSummary,
    ) -> AppError {
        let logs = self.processes.logs(&session.process_id).await;
        let detail = logs
            .last()
            .cloned()
            .unwrap_or_else(|| "no diagnostic output was captured".into());
        AppError::Engine(format!(
            "llama.cpp exited during startup with {:?}: {detail}",
            summary.exit_code
        ))
    }

    async fn run_generation(
        &self,
        session: &ActiveSession,
        request: &ChatRequest,
        sink: TokenSink,
        mut cancelled: watch::Receiver<bool>,
    ) -> AppResult<Usage> {
        let payload = build_chat_payload(request)?;

        let response = tokio::select! {
            _ = cancelled.changed() => return Err(AppError::Cancelled("chat generation stopped".into())),
            result = self.generation_client
                .post(format!("{}/v1/chat/completions", session.endpoint))
                .bearer_auth(&session.api_key)
                .header(CONTENT_TYPE, "application/json")
                .body(payload)
                .send() => result.map_err(|error| AppError::Engine(format!("llama.cpp chat request failed: {error}")))?,
        };
        if !response.status().is_success() {
            let status = response.status();
            let body = bounded_body(response).await?;
            let detail = serde_json::from_slice::<Value>(&body)
                .ok()
                .and_then(|value| {
                    value
                        .pointer("/error/message")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| String::from_utf8_lossy(&body).trim().to_owned());
            return Err(AppError::Engine(if detail.is_empty() {
                format!("llama.cpp chat returned HTTP {status}")
            } else {
                format!("llama.cpp chat returned HTTP {status}: {detail}")
            }));
        }

        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        if !content_type
            .to_ascii_lowercase()
            .starts_with("text/event-stream")
        {
            return Err(AppError::Engine(
                "llama.cpp chat did not return an event stream".into(),
            ));
        }

        let mut response = response;
        let mut decoder = SseLineDecoder::default();
        let mut sequence = 0_u64;
        let mut generated_bytes = 0_usize;
        let mut usage = Usage {
            prompt_tokens: 0,
            output_tokens: 0,
            tokens_per_second: 0.0,
        };
        let mut done = false;
        while !done {
            let chunk = tokio::select! {
                _ = cancelled.changed() => return Err(AppError::Cancelled("chat generation stopped".into())),
                result = response.chunk() => result.map_err(|error| AppError::Engine(format!("llama.cpp chat stream failed: {error}")))?,
            };
            let Some(chunk) = chunk else { break };
            for data in decoder.push(&chunk)? {
                if data == "[DONE]" {
                    done = true;
                    break;
                }
                let event: Value = serde_json::from_str(&data).map_err(|error| {
                    AppError::Engine(format!("llama.cpp returned invalid stream JSON: {error}"))
                })?;
                update_usage(&event, &mut usage);
                if let Some(text) = stream_text(&event) {
                    generated_bytes = generated_bytes.saturating_add(text.len());
                    if generated_bytes > MAX_GENERATED_TEXT_BYTES {
                        return Err(AppError::Engine(
                            "llama.cpp generated more than the 4 MiB response limit".into(),
                        ));
                    }
                    sequence = sequence.saturating_add(1);
                    sink.send(super::traits::TokenChunk {
                        job_id: request.job_id.clone(),
                        sequence,
                        text: text.to_owned(),
                    })
                    .await
                    .map_err(|_| AppError::Cancelled("the chat receiver closed".into()))?;
                }
            }
        }
        for data in decoder.finish()? {
            if data == "[DONE]" {
                done = true;
            } else {
                let event: Value = serde_json::from_str(&data).map_err(|error| {
                    AppError::Engine(format!("llama.cpp returned invalid stream JSON: {error}"))
                })?;
                update_usage(&event, &mut usage);
            }
        }
        if !done {
            return Err(AppError::Engine(
                "llama.cpp ended the chat stream before its terminal event".into(),
            ));
        }
        if generated_bytes == 0 {
            return Err(AppError::Engine(
                "the model completed without visible response text".into(),
            ));
        }
        Ok(usage)
    }
}

#[async_trait]
impl InferenceEngine for LlamaCppEngine {
    fn engine_id(&self) -> &'static str {
        "llama.cpp"
    }

    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities {
            workloads: vec!["chat".into()],
            devices: vec!["cpu".into()],
            formats: vec!["gguf".into()],
        }
    }

    async fn prepare(&self, config: EngineConfig) -> AppResult<()> {
        let executable = std::fs::canonicalize(&config.executable).map_err(|error| {
            AppError::Engine(format!("the llama.cpp executable is unavailable: {error}"))
        })?;
        let filename = executable
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if !filename.eq_ignore_ascii_case("llama-server.exe") {
            return Err(AppError::Engine(
                "the verified package does not contain llama-server.exe at its expected path"
                    .into(),
            ));
        }
        let logs = self
            .processes
            .run_owned_probe(
                "llama.cpp version probe",
                &executable,
                &["--version".into(), "--help".into()],
                SpawnOptions {
                    environment: config.environment.clone(),
                },
                Duration::from_secs(15),
            )
            .await
            .map_err(|error| AppError::Engine(error.to_string()))?;
        let version_output = logs.join("\n");
        if !version_matches(&version_output, &config.expected_version) {
            return Err(AppError::Engine(format!(
                "llama.cpp reported an unexpected version; expected {}",
                config.expected_version
            )));
        }
        self.state.lock().await.prepared = Some(PreparedConfig {
            executable,
            expected_version: config.expected_version,
            environment: config.environment,
        });
        Ok(())
    }

    async fn start(&self, request: EngineStartRequest) -> AppResult<EngineHandle> {
        if request.device_id != "cpu" {
            return Err(AppError::Engine(
                "the installed llama.cpp package supports only the CPU route".into(),
            ));
        }
        let model_path = validate_model_path(&request.model_path)?;
        let prepared = {
            let state = self.state.lock().await;
            state.prepared.clone().ok_or_else(|| {
                AppError::Engine("llama.cpp must be prepared before it can start".into())
            })?
        };
        if let Some(snapshot) = self.snapshot().await {
            if !snapshot.process.state.is_terminal() {
                return Err(AppError::Engine(
                    "a llama.cpp model session is already active".into(),
                ));
            }
        }

        let mut last_bind_error = None;
        for _ in 0..MAX_PORT_ATTEMPTS {
            let reservation = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(|error| {
                AppError::Engine(format!("a loopback port could not be reserved: {error}"))
            })?;
            let port = reservation.local_addr()?.port();
            let api_key = Uuid::new_v4().simple().to_string();
            let args = build_server_args(&model_path, port, &request.options)?;
            let mut environment = prepared.environment.clone();
            environment.insert("LLAMA_API_KEY".into(), api_key.clone());
            drop(reservation);

            let process_id = self
                .processes
                .spawn_owned(
                    "llama.cpp CPU server",
                    &prepared.executable,
                    &args,
                    SpawnOptions { environment },
                )
                .await?;
            self.processes
                .set_state(&process_id, EngineLifecycle::LoadingModel)
                .await?;
            let session = ActiveSession {
                session_id: Uuid::new_v4().to_string(),
                process_id: process_id.clone(),
                endpoint: format!("http://127.0.0.1:{port}"),
                api_key,
                model_path: model_path.clone(),
                backend_version: prepared.expected_version.clone(),
            };
            self.state.lock().await.session = Some(session.clone());

            match self.wait_until_ready(&session).await {
                Ok(()) => {
                    self.processes
                        .set_state(&process_id, EngineLifecycle::Ready)
                        .await?;
                    return Ok(EngineHandle {
                        session_id: session.session_id,
                        process_id,
                        endpoint: None,
                        backend_version: session.backend_version,
                    });
                }
                Err(error) => {
                    let logs = self.processes.logs(&process_id).await;
                    let bind_failed = logs.iter().any(|line| {
                        let line = line.to_ascii_lowercase();
                        line.contains("address already in use") || line.contains("bind failed")
                    });
                    let _ = self
                        .processes
                        .stop_with_grace(&process_id, Duration::from_millis(250))
                        .await;
                    if bind_failed {
                        last_bind_error = Some(error);
                        continue;
                    }
                    return Err(error);
                }
            }
        }
        Err(last_bind_error.unwrap_or_else(|| {
            AppError::Engine("llama.cpp could not claim a reserved loopback port".into())
        }))
    }

    async fn stop(&self) -> AppResult<()> {
        for cancellation in self.active_jobs.lock().await.values() {
            let _ = cancellation.send(true);
        }
        let session = self
            .state
            .lock()
            .await
            .session
            .clone()
            .ok_or_else(|| AppError::Engine("there is no llama.cpp session to stop".into()))?;
        let Some(summary) = self.processes.summary(&session.process_id).await else {
            return Ok(());
        };
        if summary.state.is_terminal() {
            return Ok(());
        }
        self.processes
            .set_state(&session.process_id, EngineLifecycle::Stopping)
            .await?;
        self.processes
            .stop_with_grace(&session.process_id, Duration::from_millis(250))
            .await
    }

    async fn health(&self) -> AppResult<EngineHealth> {
        let session = self
            .state
            .lock()
            .await
            .session
            .clone()
            .ok_or_else(|| AppError::Engine("there is no llama.cpp session".into()))?;
        let summary = self
            .processes
            .summary(&session.process_id)
            .await
            .ok_or_else(|| AppError::Engine("the llama.cpp process is unavailable".into()))?;
        if summary.state.is_terminal() {
            return Ok(EngineHealth {
                ready: false,
                detail: format!("llama.cpp is {:?}", summary.state),
            });
        }
        let response = self
            .client
            .get(format!("{}/health", session.endpoint))
            .send()
            .await
            .map_err(|error| AppError::Engine(format!("llama.cpp health failed: {error}")))?;
        if !response.status().is_success() {
            return Ok(EngineHealth {
                ready: false,
                detail: format!("llama.cpp health returned HTTP {}", response.status()),
            });
        }
        self.verify_identity(&session, false).await?;
        Ok(EngineHealth {
            ready: true,
            detail: "llama.cpp is ready and its loopback identity matches".into(),
        })
    }
}

#[async_trait]
impl ChatEngine for LlamaCppEngine {
    async fn generate(&self, request: ChatRequest, sink: TokenSink) -> AppResult<Usage> {
        if request.job_id.trim().is_empty() || request.job_id.len() > 128 {
            return Err(AppError::Engine("the chat job ID is invalid".into()));
        }
        if !(1..=4_096).contains(&request.max_output_tokens) {
            return Err(AppError::Engine(
                "maximum output tokens must be between 1 and 4096".into(),
            ));
        }
        let session = self
            .state
            .lock()
            .await
            .session
            .clone()
            .ok_or_else(|| AppError::Engine("there is no loaded llama.cpp model".into()))?;
        let summary = self
            .processes
            .summary(&session.process_id)
            .await
            .ok_or_else(|| AppError::Engine("the llama.cpp process is unavailable".into()))?;
        if summary.state != EngineLifecycle::Ready {
            return Err(AppError::Engine(
                "the llama.cpp model session is not ready".into(),
            ));
        }

        let (cancellation, receiver) = watch::channel(false);
        {
            let mut jobs = self.active_jobs.lock().await;
            if !jobs.is_empty() {
                return Err(AppError::Engine(
                    "another chat generation is already active".into(),
                ));
            }
            jobs.insert(request.job_id.clone(), cancellation);
        }
        if let Err(error) = self
            .processes
            .set_state(&session.process_id, EngineLifecycle::Busy)
            .await
        {
            self.active_jobs.lock().await.remove(&request.job_id);
            return Err(error);
        }
        let result = self
            .run_generation(&session, &request, sink, receiver)
            .await;
        self.active_jobs.lock().await.remove(&request.job_id);
        if self
            .processes
            .summary(&session.process_id)
            .await
            .is_some_and(|summary| !summary.state.is_terminal())
        {
            self.processes
                .set_state(&session.process_id, EngineLifecycle::Ready)
                .await?;
        }
        result
    }

    async fn cancel(&self, job_id: &str) -> AppResult<()> {
        let cancellation = self
            .active_jobs
            .lock()
            .await
            .get(job_id)
            .cloned()
            .ok_or_else(|| AppError::Engine(format!("chat job {job_id} is not active")))?;
        cancellation
            .send(true)
            .map_err(|_| AppError::Engine(format!("chat job {job_id} already finished")))
    }
}

#[derive(Default)]
struct SseLineDecoder {
    buffer: Vec<u8>,
}

impl SseLineDecoder {
    fn push(&mut self, bytes: &[u8]) -> AppResult<Vec<String>> {
        if self.buffer.len().saturating_add(bytes.len()) > MAX_STREAM_LINE_BYTES {
            return Err(AppError::Engine(
                "llama.cpp returned an oversized stream event".into(),
            ));
        }
        self.buffer.extend_from_slice(bytes);
        let mut data = Vec::new();
        while let Some(position) = self.buffer.iter().position(|byte| *byte == b'\n') {
            let mut line: Vec<u8> = self.buffer.drain(..=position).collect();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            if let Some(value) = parse_sse_data_line(&line)? {
                data.push(value);
            }
        }
        Ok(data)
    }

    fn finish(&mut self) -> AppResult<Vec<String>> {
        if self.buffer.is_empty() {
            return Ok(Vec::new());
        }
        let line = std::mem::take(&mut self.buffer);
        Ok(parse_sse_data_line(&line)?.into_iter().collect())
    }
}

fn parse_sse_data_line(line: &[u8]) -> AppResult<Option<String>> {
    let line = std::str::from_utf8(line)
        .map_err(|_| AppError::Engine("llama.cpp returned a non-UTF-8 stream event".into()))?;
    let Some(data) = line.strip_prefix("data:") else {
        return Ok(None);
    };
    Ok(Some(data.strip_prefix(' ').unwrap_or(data).to_owned()))
}

fn stream_text(event: &Value) -> Option<&str> {
    event
        .pointer("/choices/0/delta/content")
        .and_then(Value::as_str)
        .or_else(|| event.pointer("/choices/0/text").and_then(Value::as_str))
        .filter(|text| !text.is_empty())
}

fn build_chat_payload(request: &ChatRequest) -> AppResult<Vec<u8>> {
    let messages: Value = serde_json::from_str(&request.messages_json).map_err(|error| {
        AppError::Engine(format!("the chat message payload is invalid: {error}"))
    })?;
    if !messages.is_array() {
        return Err(AppError::Engine(
            "the chat message payload must be an array".into(),
        ));
    }
    let payload = serde_json::to_vec(&serde_json::json!({
        "model": "local-model",
        "messages": messages,
        "max_tokens": request.max_output_tokens,
        "stream": true,
        "stream_options": { "include_usage": true },
        "chat_template_kwargs": { "enable_thinking": false }
    }))
    .map_err(|error| AppError::Engine(format!("the chat request could not be encoded: {error}")))?;
    if payload.len() > MAX_CHAT_REQUEST_BYTES {
        return Err(AppError::Engine(
            "the chat request exceeds the 1 MiB transport limit".into(),
        ));
    }
    Ok(payload)
}

fn update_usage(event: &Value, usage: &mut Usage) {
    usage.prompt_tokens = event
        .pointer("/usage/prompt_tokens")
        .and_then(Value::as_u64)
        .or_else(|| event.pointer("/timings/prompt_n").and_then(Value::as_u64))
        .unwrap_or(usage.prompt_tokens);
    usage.output_tokens = event
        .pointer("/usage/completion_tokens")
        .and_then(Value::as_u64)
        .or_else(|| {
            event
                .pointer("/timings/predicted_n")
                .and_then(Value::as_u64)
        })
        .unwrap_or(usage.output_tokens);
    usage.tokens_per_second = event
        .pointer("/timings/predicted_per_second")
        .and_then(Value::as_f64)
        .map(|value| value as f32)
        .unwrap_or(usage.tokens_per_second);
}

fn build_server_args(
    model_path: &Path,
    port: u16,
    options: &BTreeMap<String, String>,
) -> AppResult<Vec<String>> {
    let mut args = vec![
        "--host".into(),
        "127.0.0.1".into(),
        "--port".into(),
        port.to_string(),
        "--model".into(),
        model_path.to_string_lossy().into_owned(),
        "--gpu-layers".into(),
        "0".into(),
        "--parallel".into(),
        "1".into(),
        "--no-webui".into(),
    ];
    for (name, value) in options {
        match name.as_str() {
            "context_size" => {
                let value = bounded_number(value, "context size", 256, 1_048_576)?;
                args.extend(["--ctx-size".into(), value.to_string()]);
            }
            "threads" => {
                let value = bounded_number(value, "thread count", 1, 512)?;
                args.extend(["--threads".into(), value.to_string()]);
            }
            _ => {
                return Err(AppError::Engine(format!(
                    "unsupported llama.cpp option: {name}"
                )));
            }
        }
    }
    Ok(args)
}

fn bounded_number(value: &str, label: &str, minimum: u32, maximum: u32) -> AppResult<u32> {
    let parsed = value
        .parse::<u32>()
        .map_err(|_| AppError::Engine(format!("the {label} is invalid")))?;
    if !(minimum..=maximum).contains(&parsed) {
        return Err(AppError::Engine(format!(
            "the {label} must be between {minimum} and {maximum}"
        )));
    }
    Ok(parsed)
}

fn validate_model_path(path: &Path) -> AppResult<PathBuf> {
    if !path.is_absolute()
        || !path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("gguf"))
    {
        return Err(AppError::Engine(
            "llama.cpp requires an absolute GGUF model path".into(),
        ));
    }
    let link_metadata = std::fs::symlink_metadata(path)?;
    if link_metadata.file_type().is_symlink() || is_reparse_point(&link_metadata) {
        return Err(AppError::Engine(
            "linked or reparse-point model files cannot be loaded".into(),
        ));
    }
    let canonical = std::fs::canonicalize(path)?;
    if !std::fs::metadata(&canonical)?.is_file() {
        return Err(AppError::Engine(
            "the selected GGUF model is not a regular file".into(),
        ));
    }
    Ok(canonical)
}

async fn bounded_body(mut response: Response) -> AppResult<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_HTTP_BODY_BYTES as u64)
    {
        return Err(AppError::Engine(
            "llama.cpp returned an oversized backend response".into(),
        ));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| AppError::Engine(format!("llama.cpp response failed: {error}")))?
    {
        if body.len().saturating_add(chunk.len()) > MAX_HTTP_BODY_BYTES {
            return Err(AppError::Engine(
                "llama.cpp returned an oversized backend response".into(),
            ));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn validate_props_identity(
    props: &Value,
    model_path: &Path,
    expected_version: &str,
) -> AppResult<()> {
    let reported_path = props
        .get("model_path")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::Engine("llama.cpp did not report its loaded model path".into()))?;
    if !paths_equal(Path::new(reported_path), model_path) {
        return Err(AppError::Engine(
            "the loopback endpoint reported a different loaded model".into(),
        ));
    }
    let build_info = props
        .get("build_info")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::Engine("llama.cpp did not report its build version".into()))?;
    if !version_matches(build_info, expected_version) {
        return Err(AppError::Engine(format!(
            "the loopback endpoint is not llama.cpp {expected_version}"
        )));
    }
    Ok(())
}

fn version_matches(output: &str, expected_version: &str) -> bool {
    let expected = expected_version.trim().to_ascii_lowercase();
    let build_number = expected.strip_prefix('b').unwrap_or(&expected);
    output.to_ascii_lowercase().contains(&expected)
        || output
            .split(|character: char| !character.is_ascii_alphanumeric())
            .any(|part| part == build_number)
}

fn paths_equal(left: &Path, right: &Path) -> bool {
    let left = std::fs::canonicalize(left).unwrap_or_else(|_| left.to_path_buf());
    let right = std::fs::canonicalize(right).unwrap_or_else(|_| right.to_path_buf());
    #[cfg(windows)]
    {
        normalize_windows_path(&left).eq_ignore_ascii_case(&normalize_windows_path(&right))
    }
    #[cfg(not(windows))]
    {
        left == right
    }
}

#[cfg(windows)]
fn normalize_windows_path(path: &Path) -> String {
    let value = path.to_string_lossy();
    if let Some(stripped) = value.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{stripped}")
    } else if let Some(stripped) = value.strip_prefix(r"\\?\") {
        stripped.to_owned()
    } else {
        value.into_owned()
    }
}

fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        let _ = metadata;
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_pinned_version_output_and_rejects_other_builds() {
        assert!(version_matches("llama.cpp version: 9986 (abcdef)", "b9986"));
        assert!(version_matches("build_info=b9986-deadbeef", "b9986"));
        assert!(!version_matches("llama.cpp version: 9985", "b9986"));
    }

    #[test]
    fn server_arguments_are_fixed_and_options_are_bounded() {
        let path = Path::new(r"C:\models\tiny.gguf");
        let args = build_server_args(
            path,
            32123,
            &BTreeMap::from([
                ("context_size".into(), "4096".into()),
                ("threads".into(), "8".into()),
            ]),
        )
        .unwrap();
        assert!(args.windows(2).any(|pair| pair == ["--host", "127.0.0.1"]));
        assert!(args.windows(2).any(|pair| pair == ["--gpu-layers", "0"]));
        assert!(!args.iter().any(|value| value.contains("api")));
        assert!(build_server_args(
            path,
            32123,
            &BTreeMap::from([("context_size".into(), "2".into())]),
        )
        .is_err());
        assert!(build_server_args(
            path,
            32123,
            &BTreeMap::from([("arbitrary_flag".into(), "value".into())]),
        )
        .is_err());
    }

    #[test]
    fn validates_props_model_and_build_identity() {
        let model = std::env::temp_dir().join(format!("{}.gguf", Uuid::new_v4()));
        std::fs::write(&model, b"fixture").unwrap();
        let props = serde_json::json!({
            "model_path": model.to_string_lossy(),
            "build_info": "b9986-abcdef"
        });
        assert!(validate_props_identity(&props, &model, "b9986").is_ok());
        assert!(validate_props_identity(&props, &model, "b9985").is_err());
        let _ = std::fs::remove_file(model);
    }

    #[test]
    fn decodes_split_sse_lines_and_usage() {
        let mut decoder = SseLineDecoder::default();
        assert!(decoder
            .push(b"data: {\"choices\":[{\"del")
            .unwrap()
            .is_empty());
        let lines = decoder
            .push(b"ta\":{\"content\":\"Hello\"}}]}\r\ndata: [DONE]\n\n")
            .unwrap();
        assert_eq!(lines.len(), 2);
        let event: Value = serde_json::from_str(&lines[0]).unwrap();
        assert_eq!(stream_text(&event), Some("Hello"));
        assert_eq!(lines[1], "[DONE]");

        let mut usage = Usage {
            prompt_tokens: 0,
            output_tokens: 0,
            tokens_per_second: 0.0,
        };
        update_usage(
            &serde_json::json!({
                "usage": { "prompt_tokens": 12, "completion_tokens": 4 },
                "timings": { "predicted_per_second": 23.5 }
            }),
            &mut usage,
        );
        assert_eq!(usage.prompt_tokens, 12);
        assert_eq!(usage.output_tokens, 4);
        assert_eq!(usage.tokens_per_second, 23.5);
    }

    #[test]
    fn disables_thinking_in_chat_payloads() {
        let payload = build_chat_payload(&ChatRequest {
            job_id: "payload-test".into(),
            messages_json: serde_json::json!([{
                "role": "user",
                "content": "Hello"
            }])
            .to_string(),
            max_output_tokens: 512,
        })
        .unwrap();
        let payload: Value = serde_json::from_slice(&payload).unwrap();
        assert_eq!(
            payload.pointer("/chat_template_kwargs/enable_thinking"),
            Some(&Value::Bool(false))
        );
        assert_eq!(
            payload.pointer("/max_tokens").and_then(Value::as_u64),
            Some(512)
        );
    }

    #[tokio::test]
    #[ignore = "requires NEURALOC_TEST_LLAMA_SERVER and NEURALOC_TEST_GGUF"]
    async fn loads_streams_and_stops_a_real_local_model() {
        let executable = PathBuf::from(
            std::env::var("NEURALOC_TEST_LLAMA_SERVER")
                .expect("NEURALOC_TEST_LLAMA_SERVER must point to llama-server.exe"),
        );
        let model = PathBuf::from(
            std::env::var("NEURALOC_TEST_GGUF")
                .expect("NEURALOC_TEST_GGUF must point to a tensor-bearing GGUF"),
        );
        let processes = Arc::new(ProcessManager::default());
        let engine = Arc::new(LlamaCppEngine::new(Arc::clone(&processes)).unwrap());
        engine
            .prepare(EngineConfig {
                executable,
                expected_version: "b9986".into(),
                environment: BTreeMap::new(),
            })
            .await
            .unwrap();
        let handle = engine
            .start(EngineStartRequest {
                model_path: model,
                device_id: "cpu".into(),
                options: BTreeMap::from([("context_size".into(), "1024".into())]),
            })
            .await
            .unwrap();
        assert!(engine.health().await.unwrap().ready);

        let (sink, mut tokens) = tokio::sync::mpsc::channel(32);
        let generation_engine = Arc::clone(&engine);
        let generation = tokio::spawn(async move {
            generation_engine
                .generate(
                    ChatRequest {
                        job_id: "real-model-smoke".into(),
                        messages_json: serde_json::json!([{
                            "role": "user",
                            "content": "Reply with the exact text NEURALOC_OK and nothing else."
                        }])
                        .to_string(),
                        max_output_tokens: 256,
                    },
                    sink,
                )
                .await
        });
        let mut output = String::new();
        while let Some(chunk) = tokens.recv().await {
            output.push_str(&chunk.text);
        }
        let usage = generation.await.unwrap().unwrap();
        assert!(
            output.contains("NEURALOC_OK"),
            "unexpected output: {output}"
        );
        assert!(usage.output_tokens > 0);

        let (sink, _tokens) = tokio::sync::mpsc::channel(32);
        let generation_engine = Arc::clone(&engine);
        let cancellation = tokio::spawn(async move {
            generation_engine
                .generate(
                    ChatRequest {
                        job_id: "real-model-cancel".into(),
                        messages_json: serde_json::json!([{
                            "role": "user",
                            "content": "Write a long numbered list of one thousand items."
                        }])
                        .to_string(),
                        max_output_tokens: 4_096,
                    },
                    sink,
                )
                .await
        });
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            if processes
                .summary(&handle.process_id)
                .await
                .is_some_and(|summary| summary.state == EngineLifecycle::Busy)
            {
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "generation did not enter busy state"
            );
            sleep(Duration::from_millis(10)).await;
        }
        engine.cancel("real-model-cancel").await.unwrap();
        assert!(matches!(
            cancellation.await.unwrap(),
            Err(AppError::Cancelled(_))
        ));

        engine.stop().await.unwrap();
        assert_eq!(processes.active_count().await, 0);
    }
}
