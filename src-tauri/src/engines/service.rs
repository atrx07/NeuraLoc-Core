use std::{collections::BTreeMap, path::Path, sync::Arc};

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::{
    engine_packages::{EnginePackageService, EnginePackageState},
    errors::{AppError, AppResult},
    models::{ModelService, VerificationState},
    processes::{EngineLifecycle, ProcessManager},
};

use super::{
    llama_cpp::LlamaCppEngine,
    traits::{
        ChatEngine, ChatRequest, EngineConfig, EngineHealth, EngineStartRequest, InferenceEngine,
        TokenSink, Usage,
    },
};

pub const LLAMA_CPP_CPU_PACKAGE_ID: &str = "llama.cpp-b9986-windows-x86_64-cpu";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartEngineRequest {
    pub model_id: String,
    pub context_size: Option<u32>,
    pub threads: Option<u16>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EngineSessionRequest {
    pub session_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineRuntimeStatus {
    pub engine_id: String,
    pub package_id: String,
    pub lifecycle: EngineLifecycle,
    pub session_id: Option<String>,
    pub process_id: Option<String>,
    pub pid: Option<u32>,
    pub model_id: Option<String>,
    pub model_name: Option<String>,
    pub backend_version: Option<String>,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub exit_code: Option<i32>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineLogSnapshot {
    pub session_id: String,
    pub process_id: String,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineLogBatch {
    pub session_id: String,
    pub process_id: String,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone)]
struct CurrentModel {
    id: String,
    name: String,
    path: String,
}

pub struct EngineRuntimeService {
    packages: Arc<EnginePackageService>,
    models: Arc<ModelService>,
    adapter: Arc<LlamaCppEngine>,
    operation: Mutex<()>,
    current_model: Mutex<Option<CurrentModel>>,
}

impl EngineRuntimeService {
    pub fn new(
        packages: Arc<EnginePackageService>,
        models: Arc<ModelService>,
        processes: Arc<ProcessManager>,
    ) -> AppResult<Self> {
        Ok(Self {
            packages,
            models,
            adapter: Arc::new(LlamaCppEngine::new(processes)?),
            operation: Mutex::new(()),
            current_model: Mutex::new(None),
        })
    }

    pub async fn start(&self, request: StartEngineRequest) -> AppResult<EngineRuntimeStatus> {
        let _operation = self.operation.lock().await;
        if self.has_active_session().await {
            return Err(AppError::Engine(
                "stop the active llama.cpp session before loading another model".into(),
            ));
        }
        if request.model_id.trim().is_empty() || request.model_id.len() > 128 {
            return Err(AppError::Engine("the model ID is invalid".into()));
        }

        let models = Arc::clone(&self.models);
        let model_id = request.model_id.clone();
        let model = tokio::task::spawn_blocking(move || models.reverify(&model_id))
            .await
            .map_err(|error| AppError::Engine(format!("model verification stopped: {error}")))??;
        if model.verification_state != VerificationState::Ready {
            return Err(AppError::Engine(
                model
                    .verification_error
                    .clone()
                    .unwrap_or_else(|| "the selected GGUF model is not ready".into()),
            ));
        }

        let package = self.packages.verify(LLAMA_CPP_CPU_PACKAGE_ID).await?;
        if package.state != EnginePackageState::Ready
            || package.engine_id != "llama.cpp"
            || package.route != "cpu"
        {
            return Err(AppError::Engine(
                "the verified llama.cpp CPU package is not ready".into(),
            ));
        }
        let executable = Path::new(&package.install_path).join("llama-server.exe");
        self.adapter
            .prepare(EngineConfig {
                executable,
                expected_version: package.version.clone(),
                environment: BTreeMap::new(),
            })
            .await?;

        let context_size = request.context_size.unwrap_or_else(|| {
            model
                .gguf_metadata
                .as_ref()
                .and_then(|metadata| metadata.context_length)
                .unwrap_or(4_096)
                .clamp(256, 4_096) as u32
        });
        let mut options = BTreeMap::from([("context_size".into(), context_size.to_string())]);
        if let Some(threads) = request.threads {
            options.insert("threads".into(), threads.to_string());
        }
        *self.current_model.lock().await = Some(CurrentModel {
            id: model.id.clone(),
            name: model.display_name.clone(),
            path: model.path.clone(),
        });
        self.adapter
            .start(EngineStartRequest {
                model_path: model.path.into(),
                device_id: "cpu".into(),
                options,
            })
            .await?;
        self.status().await
    }

    pub async fn stop(&self, session_id: &str) -> AppResult<EngineRuntimeStatus> {
        let snapshot = self
            .adapter
            .snapshot()
            .await
            .ok_or_else(|| AppError::Engine("there is no llama.cpp session to stop".into()))?;
        if snapshot.session_id != session_id {
            return Err(AppError::Engine(format!(
                "llama.cpp session {session_id} is not active"
            )));
        }
        self.adapter.stop().await?;
        self.status().await
    }

    pub async fn health(&self) -> AppResult<EngineHealth> {
        self.adapter.health().await
    }

    pub async fn generate(&self, request: ChatRequest, sink: TokenSink) -> AppResult<Usage> {
        self.adapter.generate(request, sink).await
    }

    pub async fn cancel_generation(&self, job_id: &str) -> AppResult<()> {
        self.adapter.cancel(job_id).await
    }

    pub async fn active_generation_count(&self) -> usize {
        self.adapter.active_job_count().await
    }

    pub async fn status(&self) -> AppResult<EngineRuntimeStatus> {
        let current_model = self.current_model.lock().await.clone();
        if let Some(snapshot) = self.adapter.snapshot().await {
            let model =
                current_model.filter(|model| paths_equal(&model.path, &snapshot.model_path));
            return Ok(EngineRuntimeStatus {
                engine_id: "llama.cpp".into(),
                package_id: LLAMA_CPP_CPU_PACKAGE_ID.into(),
                lifecycle: snapshot.process.state,
                session_id: Some(snapshot.session_id),
                process_id: Some(snapshot.process_id),
                pid: snapshot.process.pid,
                model_id: model.as_ref().map(|value| value.id.clone()),
                model_name: model.as_ref().map(|value| value.name.clone()),
                backend_version: Some(snapshot.backend_version),
                started_at: Some(snapshot.process.started_at),
                ended_at: snapshot.process.ended_at,
                exit_code: snapshot.process.exit_code,
                detail: lifecycle_detail(snapshot.process.state),
            });
        }

        let package_ready = self
            .packages
            .statuses()?
            .into_iter()
            .find(|status| status.manifest.id == LLAMA_CPP_CPU_PACKAGE_ID)
            .and_then(|status| status.installation)
            .is_some_and(|record| record.state == EnginePackageState::Ready);
        Ok(EngineRuntimeStatus {
            engine_id: "llama.cpp".into(),
            package_id: LLAMA_CPP_CPU_PACKAGE_ID.into(),
            lifecycle: if package_ready {
                EngineLifecycle::Installed
            } else {
                EngineLifecycle::NotInstalled
            },
            session_id: None,
            process_id: None,
            pid: None,
            model_id: None,
            model_name: None,
            backend_version: None,
            started_at: None,
            ended_at: None,
            exit_code: None,
            detail: if package_ready {
                "The verified llama.cpp CPU runtime is available.".into()
            } else {
                "Install and verify the llama.cpp CPU runtime to load a model.".into()
            },
        })
    }

    pub async fn logs(&self, session_id: &str) -> AppResult<EngineLogSnapshot> {
        let (process_id, lines) = self.adapter.logs(session_id).await?;
        Ok(EngineLogSnapshot {
            session_id: session_id.into(),
            process_id,
            lines,
        })
    }

    pub async fn is_active(&self) -> bool {
        if self.operation.try_lock().is_err() {
            return true;
        }
        self.has_active_session().await
    }

    async fn has_active_session(&self) -> bool {
        self.adapter
            .snapshot()
            .await
            .is_some_and(|snapshot| !snapshot.process.state.is_terminal())
    }
}

fn lifecycle_detail(lifecycle: EngineLifecycle) -> String {
    match lifecycle {
        EngineLifecycle::Starting => "Starting the owned llama.cpp process.".into(),
        EngineLifecycle::LoadingModel => {
            "llama.cpp is loading and validating the GGUF model.".into()
        }
        EngineLifecycle::Ready => {
            "The GGUF model is loaded and the loopback identity is verified.".into()
        }
        EngineLifecycle::Busy => "llama.cpp is processing a request.".into(),
        EngineLifecycle::Stopping => "Stopping the owned llama.cpp process.".into(),
        EngineLifecycle::Stopped => {
            "The llama.cpp session is stopped; retained logs remain available.".into()
        }
        EngineLifecycle::Crashed => {
            "llama.cpp exited unexpectedly; inspect the retained logs.".into()
        }
        EngineLifecycle::Error => "The llama.cpp process entered an error state.".into(),
        EngineLifecycle::Recovering => "llama.cpp recovery is in progress.".into(),
        EngineLifecycle::Installed => "The verified llama.cpp runtime is available.".into(),
        EngineLifecycle::NotInstalled => "The llama.cpp runtime is not installed.".into(),
    }
}

fn paths_equal(left: &str, right: &Path) -> bool {
    let left = std::fs::canonicalize(left).unwrap_or_else(|_| left.into());
    let right = std::fs::canonicalize(right).unwrap_or_else(|_| right.to_path_buf());
    #[cfg(windows)]
    {
        left.to_string_lossy()
            .trim_start_matches(r"\\?\")
            .eq_ignore_ascii_case(right.to_string_lossy().trim_start_matches(r"\\?\"))
    }
    #[cfg(not(windows))]
    {
        left == right
    }
}
