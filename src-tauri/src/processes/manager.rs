use std::{
    collections::{HashMap, VecDeque},
    path::Path,
    process::Stdio,
    sync::Arc,
    time::Duration,
};

use chrono::Utc;
use parking_lot::Mutex as ParkingMutex;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
    sync::Mutex,
    time::timeout,
};
use uuid::Uuid;

use crate::errors::{AppError, AppResult};

use super::{EngineLifecycle, ProcessSummary};

const MAX_LOG_LINES: usize = 2_000;

struct OwnedProcess {
    summary: ProcessSummary,
    child: Child,
    logs: Arc<ParkingMutex<VecDeque<String>>>,
}

#[derive(Default)]
pub struct ProcessManager {
    processes: Mutex<HashMap<String, OwnedProcess>>,
}

impl ProcessManager {
    pub async fn spawn_owned(
        &self,
        label: &str,
        executable: &Path,
        args: &[String],
    ) -> AppResult<String> {
        let mut command = Command::new(executable);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
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
        let summary = ProcessSummary {
            id: id.clone(),
            label: label.into(),
            pid: child.id(),
            state: EngineLifecycle::Starting,
            started_at: Utc::now().to_rfc3339(),
        };
        self.processes.lock().await.insert(
            id.clone(),
            OwnedProcess {
                summary,
                child,
                logs,
            },
        );
        Ok(id)
    }

    fn capture<R>(reader: R, buffer: Arc<ParkingMutex<VecDeque<String>>>, stream: &'static str)
    where
        R: tokio::io::AsyncRead + Unpin + Send + 'static,
    {
        tauri::async_runtime::spawn(async move {
            let mut lines = BufReader::new(reader).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let mut target = buffer.lock();
                if target.len() == MAX_LOG_LINES {
                    target.pop_front();
                }
                target.push_back(format!("[{stream}] {line}"));
            }
        });
    }

    pub async fn stop(&self, id: &str) -> AppResult<()> {
        let mut process = self
            .processes
            .lock()
            .await
            .remove(id)
            .ok_or_else(|| AppError::Process(format!("owned process {id} was not found")))?;
        process.summary.state = EngineLifecycle::Stopping;
        if timeout(Duration::from_secs(3), process.child.wait())
            .await
            .is_err()
        {
            process.child.kill().await.map_err(|error| {
                AppError::Process(format!("could not stop {}: {error}", process.summary.label))
            })?;
        }
        Ok(())
    }

    pub async fn stop_all(&self) {
        let ids: Vec<String> = self.processes.lock().await.keys().cloned().collect();
        for id in ids {
            let _ = self.stop(&id).await;
        }
    }

    pub async fn summaries(&self) -> Vec<ProcessSummary> {
        self.processes
            .lock()
            .await
            .values()
            .map(|process| process.summary.clone())
            .collect()
    }

    pub async fn logs(&self, id: &str) -> Vec<String> {
        self.processes
            .lock()
            .await
            .get(id)
            .map(|process| process.logs.lock().iter().cloned().collect())
            .unwrap_or_default()
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
