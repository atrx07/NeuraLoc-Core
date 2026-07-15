use std::sync::Arc;

use rusqlite::{params, OptionalExtension, Row, Transaction};

use crate::{
    errors::{AppError, AppResult},
    storage::Database,
};

use super::types::{ParsedPrompt, PromptMetadata, PromptSummary, PromptVersionRecord};

const VERSION_COLUMNS: &str = "
    id, profile_id, version, source_path, source_hash, front_matter_json,
    content, raw_document, source_profile_id, source_version_id, created_at
";

pub(crate) struct PromptRepository {
    database: Arc<Database>,
}

pub(crate) struct AppendVersionOutcome {
    pub version: PromptVersionRecord,
    pub already_exists: bool,
}

pub(crate) struct CreateProfileInput<'a> {
    pub profile_id: &'a str,
    pub version_id: &'a str,
    pub stable_name: &'a str,
    pub parsed: &'a ParsedPrompt,
    pub source_path: Option<&'a str>,
    pub source_profile_id: Option<&'a str>,
    pub source_version_id: Option<&'a str>,
    pub now: &'a str,
}

struct InsertVersionInput<'a> {
    version_id: &'a str,
    profile_id: &'a str,
    version: u32,
    source_path: Option<&'a str>,
    parsed: &'a ParsedPrompt,
    metadata_json: &'a str,
    source_profile_id: Option<&'a str>,
    source_version_id: Option<&'a str>,
    now: &'a str,
}

impl PromptRepository {
    pub fn new(database: Arc<Database>) -> Self {
        Self { database }
    }

    pub fn list(&self, query: &str) -> AppResult<Vec<PromptSummary>> {
        let query = query.trim();
        if query.chars().count() > 200 {
            return Err(AppError::InvalidPrompt(
                "prompt search is limited to 200 characters".into(),
            ));
        }
        let pattern = format!("%{query}%");
        let connection = self.database.connection();
        let mut statement = connection.prepare(
            "SELECT
               p.id, p.stable_name, p.collection, p.pinned, p.created_at, p.updated_at,
               v.id, v.version, v.source_path, v.front_matter_json, v.created_at
             FROM prompt_profiles p
             JOIN prompt_versions v ON v.profile_id = p.id
             WHERE p.deleted_at IS NULL
               AND v.version = (
                 SELECT MAX(latest.version) FROM prompt_versions latest WHERE latest.profile_id = p.id
               )
               AND (?1 = '' OR p.stable_name LIKE ?2 COLLATE NOCASE
                    OR COALESCE(p.collection, '') LIKE ?2 COLLATE NOCASE
                    OR v.front_matter_json LIKE ?2 COLLATE NOCASE)
             ORDER BY p.pinned DESC, p.updated_at DESC, p.stable_name COLLATE NOCASE
             LIMIT 200",
        )?;
        let stored = statement
            .query_map(params![query, pattern], summary_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        stored.into_iter().map(StoredSummary::try_into).collect()
    }

    pub fn summary(&self, profile_id: &str) -> AppResult<Option<PromptSummary>> {
        let connection = self.database.connection();
        let stored = connection
            .query_row(
                "SELECT
                   p.id, p.stable_name, p.collection, p.pinned, p.created_at, p.updated_at,
                   v.id, v.version, v.source_path, v.front_matter_json, v.created_at
                 FROM prompt_profiles p
                 JOIN prompt_versions v ON v.profile_id = p.id
                 WHERE p.id = ?1 AND p.deleted_at IS NULL
                 ORDER BY v.version DESC LIMIT 1",
                [profile_id],
                summary_from_row,
            )
            .optional()?;
        stored.map(TryInto::try_into).transpose()
    }

    pub fn get_version(&self, version_id: &str) -> AppResult<Option<PromptVersionRecord>> {
        let connection = self.database.connection();
        let stored = connection
            .query_row(
                &format!("SELECT {VERSION_COLUMNS} FROM prompt_versions WHERE id = ?1"),
                [version_id],
                StoredVersion::from_row,
            )
            .optional()?;
        stored.map(TryInto::try_into).transpose()
    }

    pub fn latest_by_source_path(
        &self,
        source_path: &str,
    ) -> AppResult<Option<PromptVersionRecord>> {
        let connection = self.database.connection();
        let stored = connection
            .query_row(
                &format!(
                    "SELECT {VERSION_COLUMNS} FROM prompt_versions
                     WHERE source_path = ?1 AND profile_id IN (
                       SELECT id FROM prompt_profiles WHERE deleted_at IS NULL
                     )
                     ORDER BY version DESC LIMIT 1"
                ),
                [source_path],
                StoredVersion::from_row,
            )
            .optional()?;
        stored.map(TryInto::try_into).transpose()
    }

    pub fn create_profile(&self, input: CreateProfileInput<'_>) -> AppResult<PromptVersionRecord> {
        let metadata_json = serialize_metadata(&input.parsed.metadata)?;
        let mut connection = self.database.connection();
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO prompt_profiles(
               id, stable_name, collection, pinned, deleted_at, created_at, updated_at
             ) VALUES (?1, ?2, ?3, 0, NULL, ?4, ?4)",
            params![
                input.profile_id,
                input.stable_name,
                input.parsed.metadata.collection,
                input.now
            ],
        )?;
        insert_version(
            &transaction,
            InsertVersionInput {
                version_id: input.version_id,
                profile_id: input.profile_id,
                version: 1,
                source_path: input.source_path,
                parsed: input.parsed,
                metadata_json: &metadata_json,
                source_profile_id: input.source_profile_id,
                source_version_id: input.source_version_id,
                now: input.now,
            },
        )?;
        transaction.commit()?;
        drop(connection);
        self.get_version(input.version_id)?
            .ok_or_else(|| AppError::Operation("the created prompt version disappeared".into()))
    }

    pub fn append_version(
        &self,
        profile_id: &str,
        expected_base_version_id: &str,
        version_id: &str,
        parsed: &ParsedPrompt,
        source_path: Option<&str>,
        now: &str,
    ) -> AppResult<AppendVersionOutcome> {
        let metadata_json = serialize_metadata(&parsed.metadata)?;
        let mut connection = self.database.connection();
        let transaction = connection.transaction()?;
        let profile_exists = transaction
            .query_row(
                "SELECT 1 FROM prompt_profiles WHERE id = ?1 AND deleted_at IS NULL",
                [profile_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !profile_exists {
            return Err(AppError::PromptNotFound(profile_id.into()));
        }

        let (latest_id, latest_version): (String, i64) = transaction.query_row(
            "SELECT id, version FROM prompt_versions
             WHERE profile_id = ?1 ORDER BY version DESC LIMIT 1",
            [profile_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if latest_id != expected_base_version_id {
            return Err(AppError::Conflict(format!(
                "prompt {profile_id} now has a newer version"
            )));
        }
        if let Some(stored) = transaction
            .query_row(
                &format!(
                    "SELECT {VERSION_COLUMNS} FROM prompt_versions
                     WHERE profile_id = ?1 AND source_hash = ?2"
                ),
                params![profile_id, parsed.source_hash],
                StoredVersion::from_row,
            )
            .optional()?
        {
            transaction.commit()?;
            return Ok(AppendVersionOutcome {
                version: stored.try_into()?,
                already_exists: true,
            });
        }
        let next_version = latest_version
            .checked_add(1)
            .and_then(|value| u32::try_from(value).ok())
            .ok_or_else(|| AppError::Operation("prompt version number overflowed".into()))?;
        insert_version(
            &transaction,
            InsertVersionInput {
                version_id,
                profile_id,
                version: next_version,
                source_path,
                parsed,
                metadata_json: &metadata_json,
                source_profile_id: None,
                source_version_id: None,
                now,
            },
        )?;
        transaction.execute(
            "UPDATE prompt_profiles SET stable_name = ?2, collection = ?3, updated_at = ?4
             WHERE id = ?1",
            params![
                profile_id,
                parsed.metadata.name.as_deref().unwrap_or("Untitled Prompt"),
                parsed.metadata.collection,
                now
            ],
        )?;
        transaction.commit()?;
        drop(connection);
        Ok(AppendVersionOutcome {
            version: self.get_version(version_id)?.ok_or_else(|| {
                AppError::Operation("the appended prompt version disappeared".into())
            })?,
            already_exists: false,
        })
    }

    pub fn set_pinned(&self, profile_id: &str, pinned: bool, now: &str) -> AppResult<()> {
        let connection = self.database.connection();
        let changed = connection.execute(
            "UPDATE prompt_profiles SET pinned = ?2, updated_at = ?3
             WHERE id = ?1 AND deleted_at IS NULL",
            params![profile_id, i64::from(pinned), now],
        )?;
        if changed == 0 {
            return Err(AppError::PromptNotFound(profile_id.into()));
        }
        Ok(())
    }

    pub fn soft_delete(&self, profile_id: &str, now: &str) -> AppResult<()> {
        let connection = self.database.connection();
        let changed = connection.execute(
            "UPDATE prompt_profiles SET deleted_at = ?2, updated_at = ?2
             WHERE id = ?1 AND deleted_at IS NULL",
            params![profile_id, now],
        )?;
        if changed == 0 {
            return Err(AppError::PromptNotFound(profile_id.into()));
        }
        Ok(())
    }
}

fn insert_version(transaction: &Transaction<'_>, input: InsertVersionInput<'_>) -> AppResult<()> {
    transaction.execute(
        "INSERT INTO prompt_versions(
           id, profile_id, version, source_path, source_hash, front_matter_json,
           content, created_at, raw_document, source_profile_id, source_version_id
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            input.version_id,
            input.profile_id,
            input.version,
            input.source_path,
            input.parsed.source_hash,
            input.metadata_json,
            input.parsed.content,
            input.now,
            input.parsed.raw_document,
            input.source_profile_id,
            input.source_version_id,
        ],
    )?;
    Ok(())
}

fn serialize_metadata(metadata: &PromptMetadata) -> AppResult<String> {
    serde_json::to_string(metadata).map_err(|error| {
        AppError::Operation(format!("prompt metadata could not be serialized: {error}"))
    })
}

struct StoredVersion {
    id: String,
    profile_id: String,
    version: i64,
    source_path: Option<String>,
    source_hash: String,
    front_matter_json: String,
    content: String,
    raw_document: String,
    source_profile_id: Option<String>,
    source_version_id: Option<String>,
    created_at: String,
}

impl StoredVersion {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            profile_id: row.get(1)?,
            version: row.get(2)?,
            source_path: row.get(3)?,
            source_hash: row.get(4)?,
            front_matter_json: row.get(5)?,
            content: row.get(6)?,
            raw_document: row.get(7)?,
            source_profile_id: row.get(8)?,
            source_version_id: row.get(9)?,
            created_at: row.get(10)?,
        })
    }
}

impl TryFrom<StoredVersion> for PromptVersionRecord {
    type Error = AppError;

    fn try_from(value: StoredVersion) -> Result<Self, Self::Error> {
        let metadata = serde_json::from_str(&value.front_matter_json).map_err(|error| {
            AppError::Operation(format!(
                "prompt version {} has corrupt metadata: {error}",
                value.id
            ))
        })?;
        let version = u32::try_from(value.version).map_err(|_| {
            AppError::Operation(format!("prompt version {} has an invalid number", value.id))
        })?;
        Ok(Self {
            id: value.id,
            profile_id: value.profile_id,
            version,
            source_path: value.source_path,
            source_hash: value.source_hash,
            metadata,
            content: value.content,
            raw_document: value.raw_document,
            source_profile_id: value.source_profile_id,
            source_version_id: value.source_version_id,
            created_at: value.created_at,
        })
    }
}

struct StoredSummary {
    profile_id: String,
    stable_name: String,
    collection: Option<String>,
    pinned: i64,
    profile_created_at: String,
    profile_updated_at: String,
    latest_version_id: String,
    latest_version: i64,
    source_path: Option<String>,
    front_matter_json: String,
    version_created_at: String,
}

fn summary_from_row(row: &Row<'_>) -> rusqlite::Result<StoredSummary> {
    Ok(StoredSummary {
        profile_id: row.get(0)?,
        stable_name: row.get(1)?,
        collection: row.get(2)?,
        pinned: row.get(3)?,
        profile_created_at: row.get(4)?,
        profile_updated_at: row.get(5)?,
        latest_version_id: row.get(6)?,
        latest_version: row.get(7)?,
        source_path: row.get(8)?,
        front_matter_json: row.get(9)?,
        version_created_at: row.get(10)?,
    })
}

impl TryFrom<StoredSummary> for PromptSummary {
    type Error = AppError;

    fn try_from(value: StoredSummary) -> Result<Self, Self::Error> {
        let metadata: PromptMetadata =
            serde_json::from_str(&value.front_matter_json).map_err(|error| {
                AppError::Operation(format!(
                    "prompt profile {} has corrupt metadata: {error}",
                    value.profile_id
                ))
            })?;
        let latest_version = u32::try_from(value.latest_version).map_err(|_| {
            AppError::Operation(format!(
                "prompt profile {} has an invalid version number",
                value.profile_id
            ))
        })?;
        Ok(Self {
            profile_id: value.profile_id,
            stable_name: value.stable_name,
            collection: value.collection,
            pinned: value.pinned != 0,
            latest_version_id: value.latest_version_id,
            latest_version,
            description: metadata.description,
            tags: metadata.tags,
            source_path: value.source_path,
            created_at: if value.profile_created_at.is_empty() {
                value.version_created_at.clone()
            } else {
                value.profile_created_at
            },
            updated_at: if value.profile_updated_at.is_empty() {
                value.version_created_at
            } else {
                value.profile_updated_at
            },
        })
    }
}
