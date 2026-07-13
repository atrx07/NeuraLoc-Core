use chrono::{DateTime, Utc};
use serde::Serialize;

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
