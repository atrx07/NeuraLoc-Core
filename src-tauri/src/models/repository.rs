use std::{path::Path, sync::Arc};

use rusqlite::{params, OptionalExtension, Row};

use crate::{
    errors::{AppError, AppResult},
    storage::Database,
};

use super::types::{GgufMetadata, ModelRecord, VerificationState};

const MODEL_COLUMNS: &str = "
    id, kind, display_name, family, format, path, size_bytes, sha256,
    verification_state, verification_error, gguf_metadata_json,
    modified_at_unix_ms, imported_at, last_verified_at, file_identity
";

pub struct ModelRepository {
    database: Arc<Database>,
}

impl ModelRepository {
    pub fn new(database: Arc<Database>) -> Self {
        Self { database }
    }

    pub fn list(&self) -> AppResult<Vec<ModelRecord>> {
        let connection = self.database.connection();
        let mut statement = connection.prepare(&format!(
            "SELECT {MODEL_COLUMNS} FROM models ORDER BY display_name COLLATE NOCASE, imported_at"
        ))?;
        let stored = statement
            .query_map([], StoredModel::from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        stored.into_iter().map(ModelRecord::try_from).collect()
    }

    pub fn get(&self, model_id: &str) -> AppResult<Option<ModelRecord>> {
        let connection = self.database.connection();
        let stored = connection
            .query_row(
                &format!("SELECT {MODEL_COLUMNS} FROM models WHERE id = ?1"),
                [model_id],
                StoredModel::from_row,
            )
            .optional()?;
        stored.map(ModelRecord::try_from).transpose()
    }

    pub fn find_existing(
        &self,
        path: &Path,
        file_identity: Option<&str>,
    ) -> AppResult<Option<ModelRecord>> {
        let path = path.to_string_lossy();
        let connection = self.database.connection();
        let stored = connection
            .query_row(
                &format!(
                    "SELECT {MODEL_COLUMNS} FROM models
                     WHERE path = ?1 OR (?2 IS NOT NULL AND file_identity = ?2)
                     LIMIT 1"
                ),
                params![path.as_ref(), file_identity],
                StoredModel::from_row,
            )
            .optional()?;
        stored.map(ModelRecord::try_from).transpose()
    }

    pub fn insert(&self, model: &ModelRecord) -> AppResult<()> {
        let metadata_json = serde_json::to_string(&model.gguf_metadata).map_err(|error| {
            AppError::Operation(format!("GGUF metadata could not be serialized: {error}"))
        })?;
        let size_bytes = i64::try_from(model.size_bytes)
            .map_err(|_| AppError::InvalidModel("the model size cannot be stored".into()))?;
        let connection = self.database.connection();
        connection.execute(
            "INSERT INTO models(
               id, kind, display_name, family, format, path, size_bytes, sha256,
               compatibility_json, imported_at, last_verified_at, verification_state,
               verification_error, gguf_metadata_json, modified_at_unix_ms, file_identity
             ) VALUES (
               ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, '{}', ?9, ?10, ?11, ?12, ?13, ?14, ?15
             )",
            params![
                model.id,
                model.kind,
                model.display_name,
                model.family,
                model.format,
                model.path,
                size_bytes,
                model.sha256,
                model.imported_at,
                model.last_verified_at,
                model.verification_state.as_str(),
                model.verification_error,
                metadata_json,
                model.modified_at_unix_ms,
                model.file_identity,
            ],
        )?;
        Ok(())
    }

    pub fn update_verification(&self, model: &ModelRecord) -> AppResult<()> {
        let metadata_json = serde_json::to_string(&model.gguf_metadata).map_err(|error| {
            AppError::Operation(format!("GGUF metadata could not be serialized: {error}"))
        })?;
        let size_bytes = i64::try_from(model.size_bytes)
            .map_err(|_| AppError::InvalidModel("the model size cannot be stored".into()))?;
        let connection = self.database.connection();
        let changed = connection.execute(
            "UPDATE models SET
               path = ?2,
               display_name = ?3,
               family = ?4,
               size_bytes = ?5,
               verification_state = ?6,
               verification_error = ?7,
               gguf_metadata_json = ?8,
               modified_at_unix_ms = ?9,
               last_verified_at = ?10,
               file_identity = ?11
             WHERE id = ?1",
            params![
                model.id,
                model.path,
                model.display_name,
                model.family,
                size_bytes,
                model.verification_state.as_str(),
                model.verification_error,
                metadata_json,
                model.modified_at_unix_ms,
                model.last_verified_at,
                model.file_identity,
            ],
        )?;
        if changed == 0 {
            return Err(AppError::ModelNotFound(model.id.clone()));
        }
        Ok(())
    }

    pub fn remove_record(&self, model_id: &str) -> AppResult<bool> {
        let connection = self.database.connection();
        Ok(connection.execute("DELETE FROM models WHERE id = ?1", [model_id])? > 0)
    }
}

struct StoredModel {
    id: String,
    kind: String,
    display_name: String,
    family: Option<String>,
    format: String,
    path: String,
    size_bytes: i64,
    sha256: Option<String>,
    verification_state: String,
    verification_error: Option<String>,
    gguf_metadata_json: String,
    modified_at_unix_ms: i64,
    imported_at: String,
    last_verified_at: Option<String>,
    file_identity: Option<String>,
}

impl StoredModel {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            kind: row.get(1)?,
            display_name: row.get(2)?,
            family: row.get(3)?,
            format: row.get(4)?,
            path: row.get(5)?,
            size_bytes: row.get(6)?,
            sha256: row.get(7)?,
            verification_state: row.get(8)?,
            verification_error: row.get(9)?,
            gguf_metadata_json: row.get(10)?,
            modified_at_unix_ms: row.get(11)?,
            imported_at: row.get(12)?,
            last_verified_at: row.get(13)?,
            file_identity: row.get(14)?,
        })
    }
}

impl TryFrom<StoredModel> for ModelRecord {
    type Error = AppError;

    fn try_from(value: StoredModel) -> Result<Self, Self::Error> {
        let verification_state =
            VerificationState::parse(&value.verification_state).ok_or_else(|| {
                AppError::Operation(format!(
                    "model {} has unknown verification state {}",
                    value.id, value.verification_state
                ))
            })?;
        let gguf_metadata: Option<GgufMetadata> = serde_json::from_str(&value.gguf_metadata_json)
            .map_err(|error| {
            AppError::Operation(format!(
                "model {} has corrupt GGUF metadata: {error}",
                value.id
            ))
        })?;
        let size_bytes = u64::try_from(value.size_bytes)
            .map_err(|_| AppError::Operation(format!("model {} has an invalid size", value.id)))?;
        Ok(ModelRecord {
            id: value.id,
            kind: value.kind,
            display_name: value.display_name,
            family: value.family,
            format: value.format,
            path: value.path,
            size_bytes,
            sha256: value.sha256,
            verification_state,
            verification_error: value.verification_error,
            gguf_metadata,
            modified_at_unix_ms: value.modified_at_unix_ms,
            imported_at: value.imported_at,
            last_verified_at: value.last_verified_at,
            file_identity: value.file_identity,
        })
    }
}
