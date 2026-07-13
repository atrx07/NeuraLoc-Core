use std::sync::Arc;

use rusqlite::{params, OptionalExtension, Row};

use crate::{
    errors::{AppError, AppResult},
    storage::Database,
};

use super::{EnginePackageRecord, EnginePackageState, InstalledPackageFile};

pub struct EnginePackageRepository {
    database: Arc<Database>,
}

impl EnginePackageRepository {
    pub fn new(database: Arc<Database>) -> Self {
        Self { database }
    }

    pub fn list(&self) -> AppResult<Vec<EnginePackageRecord>> {
        let connection = self.database.connection();
        let mut statement = connection.prepare(
            "SELECT id, engine_id, version, platform, architecture, route, install_path,
                    archive_sha256, file_manifest_json, state, source_url, error_json,
                    installed_at, verified_at
             FROM engine_packages
             ORDER BY engine_id, route, version DESC",
        )?;
        let rows = statement.query_map([], decode_record)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn get(&self, package_id: &str) -> AppResult<Option<EnginePackageRecord>> {
        let connection = self.database.connection();
        Ok(connection
            .query_row(
                "SELECT id, engine_id, version, platform, architecture, route, install_path,
                        archive_sha256, file_manifest_json, state, source_url, error_json,
                        installed_at, verified_at
                 FROM engine_packages WHERE id = ?1",
                [package_id],
                decode_record,
            )
            .optional()?)
    }

    pub fn upsert(&self, record: &EnginePackageRecord) -> AppResult<()> {
        let files = serde_json::to_string(&record.files).map_err(|error| {
            AppError::EnginePackage(format!("file inventory serialization failed: {error}"))
        })?;
        let error_json = record
            .error
            .as_ref()
            .map(|message| serde_json::json!({ "message": message }).to_string());
        let connection = self.database.connection();
        connection.execute(
            "INSERT INTO engine_packages(
               id, engine_id, version, platform, architecture, route, install_path,
               archive_sha256, file_manifest_json, state, source_url, error_json,
               installed_at, verified_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
             ON CONFLICT(id) DO UPDATE SET
               engine_id = excluded.engine_id,
               version = excluded.version,
               platform = excluded.platform,
               architecture = excluded.architecture,
               route = excluded.route,
               install_path = excluded.install_path,
               archive_sha256 = excluded.archive_sha256,
               file_manifest_json = excluded.file_manifest_json,
               state = excluded.state,
               source_url = excluded.source_url,
               error_json = excluded.error_json,
               installed_at = excluded.installed_at,
               verified_at = excluded.verified_at",
            params![
                record.id,
                record.engine_id,
                record.version,
                record.platform,
                record.architecture,
                record.route,
                record.install_path,
                record.archive_sha256,
                files,
                record.state.as_str(),
                record.source_url,
                error_json,
                record.installed_at,
                record.verified_at,
            ],
        )?;
        Ok(())
    }

    pub fn remove(&self, package_id: &str) -> AppResult<bool> {
        let connection = self.database.connection();
        Ok(connection.execute("DELETE FROM engine_packages WHERE id = ?1", [package_id])? > 0)
    }
}

fn decode_record(row: &Row<'_>) -> rusqlite::Result<EnginePackageRecord> {
    let files_json: String = row.get(8)?;
    let state_text: String = row.get(9)?;
    let error_json: Option<String> = row.get(11)?;
    let files: Vec<InstalledPackageFile> = serde_json::from_str(&files_json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            files_json.len(),
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })?;
    let state = EnginePackageState::parse(&state_text).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            state_text.len(),
            rusqlite::types::Type::Text,
            format!("unknown engine package state: {state_text}").into(),
        )
    })?;
    let error = error_json.and_then(|value| {
        serde_json::from_str::<serde_json::Value>(&value)
            .ok()
            .and_then(|json| json.get("message")?.as_str().map(str::to_owned))
            .or(Some(value))
    });
    Ok(EnginePackageRecord {
        id: row.get(0)?,
        engine_id: row.get(1)?,
        version: row.get(2)?,
        platform: row.get(3)?,
        architecture: row.get(4)?,
        route: row.get(5)?,
        install_path: row.get(6)?,
        archive_sha256: row.get(7)?,
        files,
        state,
        source_url: row.get(10)?,
        error,
        installed_at: row.get(12)?,
        verified_at: row.get(13)?,
    })
}
