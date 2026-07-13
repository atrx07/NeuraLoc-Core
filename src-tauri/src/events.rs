use chrono::{DateTime, Utc};
use std::collections::HashMap;

use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime};

use crate::errors::{AppError, AppResult};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventEnvelope<T: Serialize> {
    pub event_version: u16,
    pub sequence: u64,
    pub emitted_at: DateTime<Utc>,
    pub payload: T,
}

impl<T: Serialize> EventEnvelope<T> {
    pub fn new(sequence: u64, payload: T) -> Self {
        Self {
            event_version: 1,
            sequence,
            emitted_at: Utc::now(),
            payload,
        }
    }
}

#[derive(Default)]
pub struct EventEmitter {
    sequences: Mutex<HashMap<String, u64>>,
}

impl EventEmitter {
    pub fn emit<R, T>(
        &self,
        app: &AppHandle<R>,
        event_name: &str,
        stream_id: &str,
        payload: T,
    ) -> AppResult<()>
    where
        R: Runtime,
        T: Clone + Serialize,
    {
        let sequence = {
            let mut sequences = self.sequences.lock();
            let key = format!("{event_name}:{stream_id}");
            let next = sequences.entry(key).or_insert(0);
            *next = next.saturating_add(1);
            *next
        };
        app.emit(event_name, EventEnvelope::new(sequence, payload))
            .map_err(|error| AppError::Operation(format!("event emission failed: {error}")))
    }

    pub fn clear_stream(&self, event_name: &str, stream_id: &str) {
        self.sequences
            .lock()
            .remove(&format!("{event_name}:{stream_id}"));
    }
}
