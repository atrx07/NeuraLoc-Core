use std::{
    collections::BTreeMap,
    net::{Ipv4Addr, TcpListener},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use reqwest::{redirect, Client, Response, StatusCode};
use serde_json::Value;
use tokio::{sync::Mutex, time::sleep};
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
const MAX_HTTP_BODY_BYTES: usize = 1024 * 1024;
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
    state: Mutex<AdapterState>,
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
        Ok(Self {
            processes,
            client,
            state: Mutex::new(AdapterState::default()),
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
    async fn generate(&self, _request: ChatRequest, _sink: TokenSink) -> AppResult<Usage> {
        Err(AppError::Engine(
            "streaming chat generation is not connected yet".into(),
        ))
    }

    async fn cancel(&self, _job_id: &str) -> AppResult<()> {
        Err(AppError::Engine(
            "chat cancellation is not connected yet".into(),
        ))
    }
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
            "llama.cpp returned an oversized identity response".into(),
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
                "llama.cpp returned an oversized identity response".into(),
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
}
