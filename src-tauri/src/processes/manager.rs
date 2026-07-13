use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    path::Path,
    process::{ExitStatus, Stdio},
    sync::Arc,
    time::Duration,
};

use chrono::Utc;
use parking_lot::Mutex as ParkingMutex;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
    sync::{mpsc, oneshot, Mutex},
    time::{sleep, timeout, Instant},
};
use uuid::Uuid;

use crate::errors::{AppError, AppResult};

use super::{EngineLifecycle, ProcessSummary};

const MAX_ARGUMENTS: usize = 256;
const MAX_ARGUMENT_BYTES: usize = 64 * 1024;
const MAX_LOG_LINES: usize = 2_000;
const MAX_LOG_LINE_BYTES: usize = 16 * 1024;
const DEFAULT_STOP_GRACE: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Default)]
pub struct SpawnOptions {
    pub environment: BTreeMap<String, String>,
}

struct ProcessRecord {
    summary: ParkingMutex<ProcessSummary>,
    logs: Arc<ParkingMutex<VecDeque<String>>>,
    control: mpsc::Sender<ProcessControl>,
}

enum ProcessControl {
    Stop {
        grace: Duration,
        reply: oneshot::Sender<AppResult<()>>,
    },
}

#[derive(Clone, Default)]
pub struct ProcessManager {
    processes: Arc<Mutex<HashMap<String, Arc<ProcessRecord>>>>,
}

impl ProcessManager {
    pub async fn spawn_owned(
        &self,
        label: &str,
        executable: &Path,
        args: &[String],
        options: SpawnOptions,
    ) -> AppResult<String> {
        validate_spawn(label, args, &options.environment)?;
        let executable = canonical_executable(executable)?;
        let mut command = Command::new(&executable);
        command
            .args(args)
            .env_clear()
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        apply_baseline_environment(&mut command);
        command.envs(options.environment);

        let mut child = command
            .spawn()
            .map_err(|error| AppError::Process(format!("could not start {label}: {error}")))?;
        let id = Uuid::new_v4().to_string();
        let logs = Arc::new(ParkingMutex::new(VecDeque::with_capacity(MAX_LOG_LINES)));
        if let Some(stdout) = child.stdout.take() {
            Self::capture(stdout, Arc::clone(&logs), "stdout");
        }
        if let Some(stderr) = child.stderr.take() {
            Self::capture(stderr, Arc::clone(&logs), "stderr");
        }
        let (control, receiver) = mpsc::channel(1);
        let record = Arc::new(ProcessRecord {
            summary: ParkingMutex::new(ProcessSummary {
                id: id.clone(),
                label: label.trim().into(),
                pid: child.id(),
                state: EngineLifecycle::Starting,
                started_at: Utc::now().to_rfc3339(),
                ended_at: None,
                exit_code: None,
            }),
            logs,
            control,
        });
        self.processes
            .lock()
            .await
            .insert(id.clone(), Arc::clone(&record));
        tauri::async_runtime::spawn(Self::supervise(child, record, receiver));
        Ok(id)
    }

    async fn supervise(
        mut child: Child,
        record: Arc<ProcessRecord>,
        mut receiver: mpsc::Receiver<ProcessControl>,
    ) {
        tokio::select! {
            result = child.wait() => {
                finish_process(&record, result, false);
            }
            control = receiver.recv() => {
                if let Some(ProcessControl::Stop { grace, reply }) = control {
                    record.summary.lock().state = EngineLifecycle::Stopping;
                    let result = match timeout(grace, child.wait()).await {
                        Ok(wait_result) => wait_result
                            .map(|status| {
                                finish_process(&record, Ok(status), true);
                            })
                            .map_err(|error| AppError::Process(format!("could not wait for the owned process: {error}"))),
                        Err(_) => child.kill().await
                            .map(|_| {
                                finish_stopped_process(&record, None);
                            })
                            .map_err(|error| AppError::Process(format!("could not force-stop the owned process: {error}"))),
                    };
                    if result.is_err() {
                        finish_error_process(&record);
                    }
                    let _ = reply.send(result);
                }
            }
        }
    }

    fn capture<R>(reader: R, buffer: Arc<ParkingMutex<VecDeque<String>>>, stream: &'static str)
    where
        R: tokio::io::AsyncRead + Unpin + Send + 'static,
    {
        tauri::async_runtime::spawn(async move {
            let mut reader = BufReader::new(reader);
            let mut pending = Vec::with_capacity(1_024);
            let mut truncated = false;
            while let Ok(available) = reader.fill_buf().await {
                if available.is_empty() {
                    if !pending.is_empty() || truncated {
                        push_log_line(&buffer, stream, &pending, truncated);
                    }
                    break;
                }
                let consumed = available
                    .iter()
                    .position(|byte| *byte == b'\n')
                    .map(|index| index + 1)
                    .unwrap_or(available.len());
                let remaining = MAX_LOG_LINE_BYTES.saturating_sub(pending.len());
                let copied = consumed.min(remaining);
                pending.extend_from_slice(&available[..copied]);
                truncated |= copied < consumed;
                let ended_line = available[..consumed].ends_with(b"\n");
                reader.consume(consumed);
                if ended_line {
                    push_log_line(&buffer, stream, &pending, truncated);
                    pending.clear();
                    truncated = false;
                }
            }
        });
    }

    pub async fn stop(&self, id: &str) -> AppResult<()> {
        self.stop_with_grace(id, DEFAULT_STOP_GRACE).await
    }

    pub async fn stop_with_grace(&self, id: &str, grace: Duration) -> AppResult<()> {
        let record = self
            .processes
            .lock()
            .await
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::Process(format!("owned process {id} was not found")))?;
        if record.summary.lock().state.is_terminal() {
            return Ok(());
        }
        let (reply, response) = oneshot::channel();
        if record
            .control
            .send(ProcessControl::Stop { grace, reply })
            .await
            .is_err()
        {
            return Ok(());
        }
        response.await.unwrap_or(Ok(()))
    }

    pub async fn stop_all(&self) {
        let ids: Vec<String> = self
            .processes
            .lock()
            .await
            .iter()
            .filter(|(_, process)| !process.summary.lock().state.is_terminal())
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            let _ = self.stop(&id).await;
        }
    }

    pub async fn set_state(&self, id: &str, state: EngineLifecycle) -> AppResult<()> {
        let record = self
            .processes
            .lock()
            .await
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::Process(format!("owned process {id} was not found")))?;
        let mut summary = record.summary.lock();
        if summary.state.is_terminal() {
            return Err(AppError::Process(format!(
                "owned process {id} has already exited"
            )));
        }
        summary.state = state;
        Ok(())
    }

    pub async fn active_count(&self) -> usize {
        self.processes
            .lock()
            .await
            .values()
            .filter(|process| !process.summary.lock().state.is_terminal())
            .count()
    }

    pub async fn summary(&self, id: &str) -> Option<ProcessSummary> {
        self.processes
            .lock()
            .await
            .get(id)
            .map(|process| process.summary.lock().clone())
    }

    pub async fn summaries(&self) -> Vec<ProcessSummary> {
        let mut summaries: Vec<_> = self
            .processes
            .lock()
            .await
            .values()
            .map(|process| process.summary.lock().clone())
            .collect();
        summaries.sort_by(|left, right| right.started_at.cmp(&left.started_at));
        summaries
    }

    pub async fn logs(&self, id: &str) -> Vec<String> {
        self.processes
            .lock()
            .await
            .get(id)
            .map(|process| process.logs.lock().iter().cloned().collect())
            .unwrap_or_default()
    }

    pub async fn run_owned_probe(
        &self,
        label: &str,
        executable: &Path,
        args: &[String],
        options: SpawnOptions,
        probe_timeout: Duration,
    ) -> AppResult<Vec<String>> {
        let id = self.spawn_owned(label, executable, args, options).await?;
        let deadline = Instant::now() + probe_timeout;
        loop {
            let summary = self
                .summary(&id)
                .await
                .ok_or_else(|| AppError::Process(format!("owned probe {id} disappeared")))?;
            if summary.state.is_terminal() {
                // Log readers finish independently of the child waiter.
                sleep(Duration::from_millis(20)).await;
                let logs = self.logs(&id).await;
                if summary.state == EngineLifecycle::Stopped && summary.exit_code == Some(0) {
                    return Ok(logs);
                }
                let detail = logs
                    .last()
                    .cloned()
                    .unwrap_or_else(|| "the probe produced no diagnostic output".into());
                return Err(AppError::Process(format!(
                    "{label} failed with {:?}: {detail}",
                    summary.exit_code
                )));
            }
            if Instant::now() >= deadline {
                let _ = self.stop_with_grace(&id, Duration::from_millis(100)).await;
                return Err(AppError::Process(format!("{label} timed out")));
            }
            sleep(Duration::from_millis(20)).await;
        }
    }

    pub async fn run_probe(&self, executable: &str, args: &[&str]) -> AppResult<String> {
        let output = timeout(
            Duration::from_secs(4),
            Command::new(executable).args(args).output(),
        )
        .await
        .map_err(|_| AppError::Probe(format!("{executable} timed out")))?
        .map_err(|error| AppError::Probe(format!("{executable} could not start: {error}")))?;
        if !output.status.success() {
            return Err(AppError::Probe(format!(
                "{executable} exited with {}",
                output.status
            )));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }
}

fn validate_spawn(
    label: &str,
    args: &[String],
    environment: &BTreeMap<String, String>,
) -> AppResult<()> {
    if label.trim().is_empty() || label.len() > 128 || label.contains(['\r', '\n', '\0']) {
        return Err(AppError::Process("the process label is invalid".into()));
    }
    if args.len() > MAX_ARGUMENTS
        || args.iter().map(String::len).sum::<usize>() > MAX_ARGUMENT_BYTES
        || args.iter().any(|argument| argument.contains('\0'))
    {
        return Err(AppError::Process(
            "the process argument list exceeds its safety limits".into(),
        ));
    }
    if environment
        .iter()
        .any(|(key, value)| key.is_empty() || key.contains(['=', '\0']) || value.contains('\0'))
    {
        return Err(AppError::Process(
            "the process environment contains an invalid entry".into(),
        ));
    }
    Ok(())
}

fn canonical_executable(executable: &Path) -> AppResult<std::path::PathBuf> {
    if !executable.is_absolute() {
        return Err(AppError::Process(
            "the executable path must be absolute".into(),
        ));
    }
    let canonical = std::fs::canonicalize(executable)
        .map_err(|error| AppError::Process(format!("the executable is unavailable: {error}")))?;
    if !std::fs::metadata(&canonical)?.is_file() {
        return Err(AppError::Process(
            "the executable path is not a regular file".into(),
        ));
    }
    Ok(canonical)
}

fn apply_baseline_environment(command: &mut Command) {
    #[cfg(windows)]
    const KEYS: &[&str] = &["SystemRoot", "WINDIR", "TEMP", "TMP"];
    #[cfg(not(windows))]
    const KEYS: &[&str] = &["HOME", "LANG", "PATH", "TMPDIR"];
    for key in KEYS {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
}

fn finish_process(
    record: &ProcessRecord,
    result: std::io::Result<ExitStatus>,
    stop_requested: bool,
) {
    match result {
        Ok(status) => {
            let mut summary = record.summary.lock();
            summary.state = if stop_requested || status.success() {
                EngineLifecycle::Stopped
            } else {
                EngineLifecycle::Crashed
            };
            summary.exit_code = status.code();
            summary.ended_at = Some(Utc::now().to_rfc3339());
        }
        Err(_) => finish_error_process(record),
    }
}

fn finish_stopped_process(record: &ProcessRecord, exit_code: Option<i32>) {
    let mut summary = record.summary.lock();
    summary.state = EngineLifecycle::Stopped;
    summary.exit_code = exit_code;
    summary.ended_at = Some(Utc::now().to_rfc3339());
}

fn finish_error_process(record: &ProcessRecord) {
    let mut summary = record.summary.lock();
    summary.state = EngineLifecycle::Error;
    summary.ended_at = Some(Utc::now().to_rfc3339());
}

fn push_log_line(
    buffer: &ParkingMutex<VecDeque<String>>,
    stream: &str,
    bytes: &[u8],
    truncated: bool,
) {
    let text = String::from_utf8_lossy(bytes);
    let text = text.trim_end_matches(['\r', '\n']);
    let text = redact_log_line(text);
    let suffix = if truncated { " [truncated]" } else { "" };
    let mut target = buffer.lock();
    if target.len() == MAX_LOG_LINES {
        target.pop_front();
    }
    target.push_back(format!("[{stream}] {text}{suffix}"));
}

fn redact_log_line(line: &str) -> String {
    const MARKERS: &[&str] = &["api_key=", "apikey=", "token=", "authorization:"];
    let lower = line.to_ascii_lowercase();
    let mut first = None;
    for marker in MARKERS {
        if let Some(index) = lower.find(marker) {
            if first.map(|current| index < current).unwrap_or(true) {
                first = Some(index + marker.len());
            }
        }
    }
    match first {
        Some(value_start) => format!("{}[REDACTED]", &line[..value_start]),
        None => line.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Instant};

    struct TestExecutable(std::path::PathBuf);

    impl TestExecutable {
        fn copy_from_current() -> Self {
            let source = std::env::current_exe().unwrap();
            let extension = source
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or_default();
            let filename = if extension.is_empty() {
                format!("neuraloc-process-fixture-{}", Uuid::new_v4())
            } else {
                format!("neuraloc-process-fixture-{}.{}", Uuid::new_v4(), extension)
            };
            let destination = std::env::temp_dir().join(filename);
            std::fs::copy(source, &destination).unwrap();
            Self(destination)
        }
    }

    impl Drop for TestExecutable {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[test]
    fn process_fixture_entrypoint() {
        if std::env::var("NEURALOC_PROCESS_FIXTURE").as_deref() != Ok("1") {
            return;
        }
        println!("fixture ready");
        eprintln!("token=fixture-secret");
        match std::env::var("NEURALOC_PROCESS_FIXTURE_MODE").as_deref() {
            Ok("crash") => std::process::exit(17),
            Ok("wait") => std::thread::sleep(Duration::from_secs(30)),
            _ => {}
        }
    }

    fn fixture_options(mode: &str) -> SpawnOptions {
        SpawnOptions {
            environment: BTreeMap::from([
                ("NEURALOC_PROCESS_FIXTURE".into(), "1".into()),
                ("NEURALOC_PROCESS_FIXTURE_MODE".into(), mode.into()),
            ]),
        }
    }

    fn fixture_args() -> Vec<String> {
        vec![
            "--exact".into(),
            "processes::manager::tests::process_fixture_entrypoint".into(),
            "--nocapture".into(),
            "--test-threads=1".into(),
        ]
    }

    async fn wait_for_terminal(manager: &ProcessManager, id: &str) -> ProcessSummary {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let summary = manager.summary(id).await.unwrap();
            if summary.state.is_terminal() {
                return summary;
            }
            assert!(
                Instant::now() < deadline,
                "process did not reach a terminal state"
            );
            sleep(Duration::from_millis(20)).await;
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn tracks_natural_exit_and_redacts_logs() {
        let manager = ProcessManager::default();
        let executable = TestExecutable::copy_from_current();
        let id = manager
            .spawn_owned(
                "fixture",
                &executable.0,
                &fixture_args(),
                fixture_options("exit"),
            )
            .await
            .unwrap();
        let summary = wait_for_terminal(&manager, &id).await;
        assert_eq!(summary.state, EngineLifecycle::Stopped);
        assert_eq!(summary.exit_code, Some(0));
        assert!(summary.ended_at.is_some());

        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let logs = manager.logs(&id).await;
            if logs.iter().any(|line| line.contains("[REDACTED]")) {
                assert!(!logs.iter().any(|line| line.contains("fixture-secret")));
                break;
            }
            assert!(Instant::now() < deadline, "fixture logs were not captured");
            sleep(Duration::from_millis(20)).await;
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn marks_nonzero_natural_exit_as_crashed() {
        let manager = ProcessManager::default();
        let executable = TestExecutable::copy_from_current();
        let id = manager
            .spawn_owned(
                "fixture",
                &executable.0,
                &fixture_args(),
                fixture_options("crash"),
            )
            .await
            .unwrap();
        let summary = wait_for_terminal(&manager, &id).await;
        assert_eq!(summary.state, EngineLifecycle::Crashed);
        assert_eq!(summary.exit_code, Some(17));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn force_stops_an_owned_process_after_the_grace_period() {
        let manager = ProcessManager::default();
        let executable = TestExecutable::copy_from_current();
        let id = manager
            .spawn_owned(
                "fixture",
                &executable.0,
                &fixture_args(),
                fixture_options("wait"),
            )
            .await
            .unwrap();
        manager
            .stop_with_grace(&id, Duration::from_millis(25))
            .await
            .unwrap();
        let summary = manager.summary(&id).await.unwrap();
        assert_eq!(summary.state, EngineLifecycle::Stopped);
        assert!(summary.ended_at.is_some());
        assert_eq!(manager.active_count().await, 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn runs_a_bounded_owned_probe() {
        let manager = ProcessManager::default();
        let executable = TestExecutable::copy_from_current();
        let logs = manager
            .run_owned_probe(
                "fixture version probe",
                &executable.0,
                &fixture_args(),
                fixture_options("exit"),
                Duration::from_secs(2),
            )
            .await
            .unwrap();
        assert!(logs.iter().any(|line| line.contains("fixture ready")));
        assert!(logs.iter().any(|line| line.contains("[REDACTED]")));
    }

    #[test]
    fn bounds_and_redacts_captured_lines() {
        let buffer = ParkingMutex::new(VecDeque::new());
        let input = format!("api_key={}\n", "x".repeat(MAX_LOG_LINE_BYTES + 100));
        push_log_line(&buffer, "stderr", input.as_bytes(), true);
        let line = buffer.lock().front().unwrap().clone();
        assert!(line.contains("api_key=[REDACTED]"));
        assert!(!line.contains(&"x".repeat(32)));
        assert!(line.ends_with("[truncated]"));
    }
}
