use std::{
    fs::File,
    io::{Read, Take},
    sync::Arc,
};

use chrono::Utc;
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::{
    errors::{AppError, AppResult},
    storage::Database,
};

use super::{
    parser::{parse_prompt_document, MAX_PROMPT_BYTES},
    path_grants::prompt_file,
    repository::{CreateProfileInput, PromptRepository},
    types::{
        CompiledPrompt, CreatePromptRequest, DuplicatePromptRequest, ExportPromptRequest,
        ImportPromptRequest, PromptExport, PromptExportMode, PromptMetadata, PromptMutationOutcome,
        PromptSummary, PromptVersionRecord, SavePromptVersionRequest,
    },
};

pub struct PromptService {
    repository: PromptRepository,
}

impl PromptService {
    pub fn new(database: Arc<Database>) -> Self {
        Self {
            repository: PromptRepository::new(database),
        }
    }

    pub fn list(&self, query: Option<&str>) -> AppResult<Vec<PromptSummary>> {
        self.repository.list(query.unwrap_or_default())
    }

    pub fn import(&self, request: ImportPromptRequest) -> AppResult<PromptMutationOutcome> {
        let granted = prompt_file(&request.path)?;
        let bytes = read_bounded(File::open(&granted.canonical_path)?)?;
        let document = String::from_utf8(bytes).map_err(|error| {
            AppError::InvalidPrompt(format!(
                "the selected file is not valid UTF-8 near byte {}",
                error.utf8_error().valid_up_to()
            ))
        })?;
        let parsed = parse_prompt_document(&document, &granted.fallback_name)?;
        let source_path = granted.canonical_path.to_string_lossy().into_owned();
        if let Some(latest) = self.repository.latest_by_source_path(&source_path)? {
            let outcome = self.repository.append_version(
                &latest.profile_id,
                &latest.id,
                &Uuid::new_v4().to_string(),
                &parsed,
                Some(&source_path),
                &Utc::now().to_rfc3339(),
            )?;
            return self.outcome(latest.profile_id, outcome.version, outcome.already_exists);
        }
        self.create_from_parsed(&parsed, Some(&source_path), None, None, None)
    }

    pub fn create(&self, request: CreatePromptRequest) -> AppResult<PromptMutationOutcome> {
        let name = validate_profile_name(&request.name)?;
        let parsed = parse_prompt_document(&request.content, &name)?;
        self.create_from_parsed(&parsed, None, Some(name), None, None)
    }

    pub fn save_version(
        &self,
        request: SavePromptVersionRequest,
    ) -> AppResult<PromptMutationOutcome> {
        let summary = self
            .repository
            .summary(&request.profile_id)?
            .ok_or_else(|| AppError::PromptNotFound(request.profile_id.clone()))?;
        let parsed = parse_prompt_document(&request.document, &summary.stable_name)?;
        let outcome = self.repository.append_version(
            &request.profile_id,
            &request.base_version_id,
            &Uuid::new_v4().to_string(),
            &parsed,
            None,
            &Utc::now().to_rfc3339(),
        )?;
        self.outcome(request.profile_id, outcome.version, outcome.already_exists)
    }

    pub fn get_version(&self, version_id: &str) -> AppResult<PromptVersionRecord> {
        self.repository
            .get_version(version_id)?
            .ok_or_else(|| AppError::PromptNotFound(version_id.into()))
    }

    pub fn duplicate(&self, request: DuplicatePromptRequest) -> AppResult<PromptMutationOutcome> {
        let source = self.get_version(&request.version_id)?;
        let source_summary = self
            .repository
            .summary(&source.profile_id)?
            .ok_or_else(|| AppError::PromptNotFound(source.profile_id.clone()))?;
        let name = validate_profile_name(
            &request
                .name
                .unwrap_or_else(|| format!("{} Copy", source_summary.stable_name)),
        )?;
        let parsed = parse_prompt_document(&source.raw_document, &name)?;
        self.create_from_parsed(
            &parsed,
            None,
            Some(name),
            Some(&source.profile_id),
            Some(&source.id),
        )
    }

    pub fn set_pinned(&self, profile_id: &str, pinned: bool) -> AppResult<PromptSummary> {
        self.repository
            .set_pinned(profile_id, pinned, &Utc::now().to_rfc3339())?;
        self.repository
            .summary(profile_id)?
            .ok_or_else(|| AppError::PromptNotFound(profile_id.into()))
    }

    pub fn soft_delete(&self, profile_id: &str) -> AppResult<()> {
        self.repository
            .soft_delete(profile_id, &Utc::now().to_rfc3339())
    }

    pub fn export(&self, request: ExportPromptRequest) -> AppResult<PromptExport> {
        let version = self.get_version(&request.version_id)?;
        let summary = self
            .repository
            .summary(&version.profile_id)?
            .ok_or_else(|| AppError::PromptNotFound(version.profile_id.clone()))?;
        let content = match request.mode {
            PromptExportMode::Original => version.raw_document,
            PromptExportMode::Normalized => {
                normalized_document(&version.metadata, &version.content)?
            }
        };
        Ok(PromptExport {
            file_name: format!("{}.md", safe_file_stem(&summary.stable_name)),
            content,
        })
    }

    pub fn compile(&self, version_id: &str) -> AppResult<CompiledPrompt> {
        let version = self.get_version(version_id)?;
        let estimated_tokens = version
            .content
            .chars()
            .count()
            .div_ceil(4)
            .min(u32::MAX as usize) as u32;
        Ok(CompiledPrompt {
            version_id: version.id,
            content: version.content,
            estimated_tokens,
            approximate: true,
        })
    }

    fn create_from_parsed(
        &self,
        parsed: &super::types::ParsedPrompt,
        source_path: Option<&str>,
        stable_name: Option<String>,
        source_profile_id: Option<&str>,
        source_version_id: Option<&str>,
    ) -> AppResult<PromptMutationOutcome> {
        let profile_id = Uuid::new_v4().to_string();
        let version_id = Uuid::new_v4().to_string();
        let stable_name = stable_name
            .or_else(|| parsed.metadata.name.clone())
            .unwrap_or_else(|| "Untitled Prompt".into());
        let now = Utc::now().to_rfc3339();
        let version = self.repository.create_profile(CreateProfileInput {
            profile_id: &profile_id,
            version_id: &version_id,
            stable_name: &stable_name,
            parsed,
            source_path,
            source_profile_id,
            source_version_id,
            now: &now,
        })?;
        self.outcome(profile_id, version, false)
    }

    fn outcome(
        &self,
        profile_id: String,
        version: PromptVersionRecord,
        already_exists: bool,
    ) -> AppResult<PromptMutationOutcome> {
        let prompt = self
            .repository
            .summary(&profile_id)?
            .ok_or_else(|| AppError::PromptNotFound(profile_id))?;
        Ok(PromptMutationOutcome {
            prompt,
            version,
            already_exists,
        })
    }
}

fn read_bounded(file: File) -> AppResult<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut reader: Take<File> = file.take((MAX_PROMPT_BYTES + 1) as u64);
    reader.read_to_end(&mut bytes)?;
    if bytes.len() > MAX_PROMPT_BYTES {
        return Err(AppError::InvalidPrompt(format!(
            "the prompt file exceeds the {MAX_PROMPT_BYTES}-byte limit"
        )));
    }
    Ok(bytes)
}

fn normalized_document(metadata: &PromptMetadata, content: &str) -> AppResult<String> {
    let mut object = Map::new();
    insert_optional(
        &mut object,
        "name",
        metadata.name.clone().map(Value::String),
    );
    insert_optional(
        &mut object,
        "version",
        metadata.declared_version.clone().map(Value::String),
    );
    insert_optional(
        &mut object,
        "description",
        metadata.description.clone().map(Value::String),
    );
    if !metadata.tags.is_empty() {
        object.insert(
            "tags".into(),
            Value::Array(metadata.tags.iter().cloned().map(Value::String).collect()),
        );
    }
    if !metadata.recommended_models.is_empty() {
        object.insert(
            "recommended_models".into(),
            Value::Array(
                metadata
                    .recommended_models
                    .iter()
                    .cloned()
                    .map(Value::String)
                    .collect(),
            ),
        );
    }
    insert_optional(
        &mut object,
        "temperature",
        metadata
            .temperature
            .and_then(serde_json::Number::from_f64)
            .map(Value::Number),
    );
    insert_optional(
        &mut object,
        "top_p",
        metadata
            .top_p
            .and_then(serde_json::Number::from_f64)
            .map(Value::Number),
    );
    insert_optional(
        &mut object,
        "top_k",
        metadata.top_k.map(|value| Value::Number(value.into())),
    );
    insert_optional(
        &mut object,
        "context_reserve",
        metadata
            .context_reserve
            .map(|value| Value::Number(value.into())),
    );
    insert_optional(
        &mut object,
        "collection",
        metadata.collection.clone().map(Value::String),
    );
    for (key, value) in &metadata.extra {
        object.entry(key.clone()).or_insert_with(|| value.clone());
    }
    let front_matter = serde_json::to_string_pretty(&Value::Object(object)).map_err(|error| {
        AppError::Operation(format!(
            "normalized prompt metadata could not be exported: {error}"
        ))
    })?;
    Ok(format!("---\n{front_matter}\n---\n{content}"))
}

fn insert_optional(object: &mut Map<String, Value>, key: &str, value: Option<Value>) {
    if let Some(value) = value {
        object.insert(key.into(), value);
    }
}

fn safe_file_stem(value: &str) -> String {
    let value: String = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect();
    let value = value.trim_matches('-');
    if value.is_empty() {
        "prompt".into()
    } else {
        value.chars().take(80).collect()
    }
}

fn validate_profile_name(value: &str) -> AppResult<String> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 120 || value.contains('\0') {
        return Err(AppError::InvalidPrompt(
            "the prompt name must contain 1 to 120 characters".into(),
        ));
    }
    Ok(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service() -> (PromptService, std::path::PathBuf) {
        let directory = std::env::temp_dir().join(format!("neuraloc-prompts-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let database = Arc::new(Database::open(&directory.join("test.db")).unwrap());
        (PromptService::new(database), directory)
    }

    #[test]
    fn imports_reimports_and_versions_a_prompt_file() {
        let (service, directory) = service();
        let path = directory.join("review.md");
        std::fs::write(
            &path,
            "---\r\nname: Reviewer\r\ntags: [code]\r\n---\r\nReview carefully.\r\n",
        )
        .unwrap();
        let first = service
            .import(ImportPromptRequest {
                path: path.to_string_lossy().into_owned(),
            })
            .unwrap();
        let duplicate = service
            .import(ImportPromptRequest {
                path: path.to_string_lossy().into_owned(),
            })
            .unwrap();
        assert_eq!(first.prompt.profile_id, duplicate.prompt.profile_id);
        assert!(duplicate.already_exists);
        assert_eq!(duplicate.version.version, 1);

        std::fs::write(&path, "---\nname: Reviewer\n---\nReview twice.\n").unwrap();
        let changed = service
            .import(ImportPromptRequest {
                path: path.to_string_lossy().into_owned(),
            })
            .unwrap();
        assert_eq!(changed.prompt.profile_id, first.prompt.profile_id);
        assert_eq!(changed.version.version, 2);
        assert_eq!(service.list(Some("reviewer")).unwrap().len(), 1);
        drop(service);
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn creates_immutable_versions_and_rejects_stale_edits() {
        let (service, directory) = service();
        let created = service
            .create(CreatePromptRequest {
                name: "Architect".into(),
                content: "Design carefully.".into(),
            })
            .unwrap();
        let saved = service
            .save_version(SavePromptVersionRequest {
                profile_id: created.prompt.profile_id.clone(),
                base_version_id: created.version.id.clone(),
                document: "Design carefully and verify.".into(),
            })
            .unwrap();
        assert_eq!(saved.version.version, 2);
        assert_eq!(
            service.get_version(&created.version.id).unwrap().content,
            "Design carefully."
        );
        assert!(service
            .save_version(SavePromptVersionRequest {
                profile_id: created.prompt.profile_id,
                base_version_id: created.version.id,
                document: "Stale edit".into(),
            })
            .is_err());
        drop(service);
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn duplicates_soft_deletes_and_keeps_historical_versions_readable() {
        let (service, directory) = service();
        let created = service
            .create(CreatePromptRequest {
                name: "Original".into(),
                content: "Original content".into(),
            })
            .unwrap();
        let duplicate = service
            .duplicate(DuplicatePromptRequest {
                version_id: created.version.id.clone(),
                name: None,
            })
            .unwrap();
        assert_eq!(
            duplicate.version.source_profile_id.as_deref(),
            Some(created.prompt.profile_id.as_str())
        );
        assert_eq!(
            duplicate.version.source_version_id.as_deref(),
            Some(created.version.id.as_str())
        );
        service.soft_delete(&created.prompt.profile_id).unwrap();
        assert_eq!(service.list(None).unwrap().len(), 1);
        assert_eq!(
            service.get_version(&created.version.id).unwrap().content,
            "Original content"
        );
        drop(service);
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn exports_original_and_normalized_documents_and_compiles_exact_content() {
        let (service, directory) = service();
        let created = service
            .create(CreatePromptRequest {
                name: "Exact".into(),
                content: "Keep\r\nline endings.\r\n".into(),
            })
            .unwrap();
        let original = service
            .export(ExportPromptRequest {
                version_id: created.version.id.clone(),
                mode: PromptExportMode::Original,
            })
            .unwrap();
        assert_eq!(original.content, "Keep\r\nline endings.\r\n");
        let normalized = service
            .export(ExportPromptRequest {
                version_id: created.version.id.clone(),
                mode: PromptExportMode::Normalized,
            })
            .unwrap();
        assert!(normalized.content.starts_with("---\n{"));
        let reparsed = parse_prompt_document(&normalized.content, "fallback").unwrap();
        assert_eq!(reparsed.metadata, created.version.metadata);
        assert_eq!(reparsed.content, created.version.content);
        let compiled = service.compile(&created.version.id).unwrap();
        assert_eq!(compiled.content, created.version.content);
        assert!(compiled.approximate);
        drop(service);
        let _ = std::fs::remove_dir_all(directory);
    }
}
