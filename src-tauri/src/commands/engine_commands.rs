use std::{sync::Arc, time::Duration};

use tauri::{AppHandle, State};

use crate::{
    app_state::AppState,
    engines::{
        EngineLogBatch, EngineLogSnapshot, EngineRuntimeService, EngineRuntimeStatus,
        EngineSessionRequest, StartEngineRequest,
    },
    errors::IpcError,
    events::EventEmitter,
    processes::EngineLifecycle,
};

#[tauri::command]
pub async fn get_engine_status(
    state: State<'_, AppState>,
) -> Result<EngineRuntimeStatus, IpcError> {
    state.engines.status().await.map_err(Into::into)
}

#[tauri::command]
pub async fn get_engine_health(
    state: State<'_, AppState>,
) -> Result<crate::engines::traits::EngineHealth, IpcError> {
    state.engines.health().await.map_err(Into::into)
}

#[tauri::command]
pub async fn start_engine(
    app: AppHandle,
    state: State<'_, AppState>,
    request: StartEngineRequest,
) -> Result<EngineRuntimeStatus, IpcError> {
    let status = state.engines.start(request).await?;
    let session_id = status
        .session_id
        .clone()
        .expect("a started engine status must include a session ID");
    state
        .events
        .emit(&app, "engine://state-changed", &session_id, status.clone())?;
    spawn_monitor(
        app,
        Arc::clone(&state.events),
        Arc::clone(&state.engines),
        session_id,
        status.clone(),
    );
    Ok(status)
}

#[tauri::command]
pub async fn stop_engine(
    app: AppHandle,
    state: State<'_, AppState>,
    request: EngineSessionRequest,
) -> Result<EngineRuntimeStatus, IpcError> {
    let status = state.engines.stop(&request.session_id).await?;
    state.events.emit(
        &app,
        "engine://state-changed",
        &request.session_id,
        status.clone(),
    )?;
    Ok(status)
}

#[tauri::command]
pub async fn get_engine_logs(
    state: State<'_, AppState>,
    request: EngineSessionRequest,
) -> Result<EngineLogSnapshot, IpcError> {
    state
        .engines
        .logs(&request.session_id)
        .await
        .map_err(Into::into)
}

fn spawn_monitor(
    app: AppHandle,
    events: Arc<EventEmitter>,
    engines: Arc<EngineRuntimeService>,
    session_id: String,
    initial_status: EngineRuntimeStatus,
) {
    tauri::async_runtime::spawn(async move {
        let mut previous_status = initial_status;
        let mut emitted_logs = 0_usize;
        loop {
            let status = match engines.status().await {
                Ok(status) => status,
                Err(error) => {
                    tracing::warn!(%error, "engine lifecycle monitor could not read status");
                    break;
                }
            };
            if status.session_id.as_deref() != Some(&session_id) {
                break;
            }
            if status != previous_status {
                if let Err(error) =
                    events.emit(&app, "engine://state-changed", &session_id, status.clone())
                {
                    tracing::warn!(%error, "engine lifecycle event failed");
                }
                previous_status = status.clone();
            }

            match engines.logs(&session_id).await {
                Ok(snapshot) => {
                    if snapshot.lines.len() < emitted_logs {
                        emitted_logs = 0;
                    }
                    if snapshot.lines.len() > emitted_logs {
                        let end = (emitted_logs + 100).min(snapshot.lines.len());
                        let batch = EngineLogBatch {
                            session_id: session_id.clone(),
                            process_id: snapshot.process_id,
                            lines: snapshot.lines[emitted_logs..end].to_vec(),
                        };
                        emitted_logs = end;
                        if let Err(error) =
                            events.emit(&app, "engine://log-line", &session_id, batch)
                        {
                            tracing::warn!(%error, "engine log event failed");
                        }
                    }
                }
                Err(error) => tracing::warn!(%error, "engine log monitor could not read logs"),
            }

            if matches!(
                status.lifecycle,
                EngineLifecycle::Stopped | EngineLifecycle::Crashed | EngineLifecycle::Error
            ) {
                if let Ok(snapshot) = engines.logs(&session_id).await {
                    if emitted_logs >= snapshot.lines.len() {
                        break;
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        events.clear_stream("engine://state-changed", &session_id);
        events.clear_stream("engine://log-line", &session_id);
    });
}
