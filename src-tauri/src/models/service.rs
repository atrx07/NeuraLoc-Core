use std::{
    collections::{HashMap, VecDeque},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use chrono::Utc;
use parking_lot::Mutex;
use uuid::Uuid;

use crate::{
    errors::{AppError, AppResult},
    storage::Database,
};

use super::{
    gguf::inspect_gguf,
    path_grants::{GrantedModelFile, PathGrantService},
    repository::ModelRepository,
    types::{
        ImportModelOutcome, ModelRecord, ModelScanIssue, ModelScanProgress, ModelScanSummary,
        ScanModelFolderRequest, ScanPhase, VerificationState,
    },
};

const MAX_SCAN_DEPTH: usize = 32;
const MAX_SCAN_FILES: usize = 100_000;
const MAX_REPORTED_ISSUES: usize = 100;

pub struct ModelService {
    repository: ModelRepository,
    scans: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl ModelService {
    pub fn new(database: Arc<Database>) -> Self {
        Self {
            repository: ModelRepository::new(database),
            scans: Mutex::new(HashMap::new()),
        }
    }

    pub fn list(&self) -> AppResult<Vec<ModelRecord>> {
        self.repository.list()
    }

    pub fn import_model(&self, path: &str) -> AppResult<ImportModelOutcome> {
        let granted = PathGrantService::model_file(path)?;
        let existing = self
            .repository
            .find_existing(&granted.canonical_path, granted.file_identity.as_deref())?;
        let already_indexed = existing.is_some();
        let now = Utc::now().to_rfc3339();
        let mut model = existing.unwrap_or_else(|| ModelRecord {
            id: Uuid::new_v4().to_string(),
            kind: "llm".into(),
            display_name: granted.display_name.clone(),
            family: None,
            format: "gguf".into(),
            path: granted.canonical_path.to_string_lossy().into_owned(),
            size_bytes: granted.size_bytes,
            sha256: None,
            verification_state: VerificationState::MetadataPending,
            verification_error: None,
            gguf_metadata: None,
            modified_at_unix_ms: granted.modified_at_unix_ms,
            imported_at: now.clone(),
            last_verified_at: None,
            file_identity: granted.file_identity.clone(),
        });

        apply_inspection(&mut model, &granted, now);
        if already_indexed {
            self.repository.update_verification(&model)?;
        } else {
            self.repository.insert(&model)?;
        }
        Ok(ImportModelOutcome {
            model,
            already_indexed,
        })
    }

    pub fn reverify(&self, model_id: &str) -> AppResult<ModelRecord> {
        let mut model = self
            .repository
            .get(model_id)?
            .ok_or_else(|| AppError::ModelNotFound(model_id.into()))?;
        let now = Utc::now().to_rfc3339();
        if !Path::new(&model.path).exists() {
            model.verification_state = VerificationState::Missing;
            model.verification_error =
                Some("The indexed file no longer exists at this path.".into());
            model.last_verified_at = Some(now);
            self.repository.update_verification(&model)?;
            return Ok(model);
        }

        match PathGrantService::model_file(&model.path) {
            Ok(granted) => apply_inspection(&mut model, &granted, now),
            Err(error) => {
                model.verification_state = VerificationState::Invalid;
                model.verification_error = Some(error.to_string());
                model.gguf_metadata = None;
                model.last_verified_at = Some(now);
            }
        }
        self.repository.update_verification(&model)?;
        Ok(model)
    }

    pub fn remove_record(&self, model_id: &str) -> AppResult<()> {
        if self.repository.remove_record(model_id)? {
            Ok(())
        } else {
            Err(AppError::ModelNotFound(model_id.into()))
        }
    }

    pub fn cancel_scan(&self, scan_id: &str) -> bool {
        let scans = self.scans.lock();
        if let Some(cancelled) = scans.get(scan_id) {
            cancelled.store(true, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    pub fn scan_folder<F>(
        &self,
        request: ScanModelFolderRequest,
        mut on_progress: F,
    ) -> AppResult<ModelScanSummary>
    where
        F: FnMut(ModelScanProgress),
    {
        Uuid::parse_str(&request.scan_id)
            .map_err(|_| AppError::Operation("the scan ID is invalid".into()))?;
        let cancelled = Arc::new(AtomicBool::new(false));
        {
            let mut scans = self.scans.lock();
            if scans.contains_key(&request.scan_id) {
                return Err(AppError::Operation(
                    "a scan with this ID is already running".into(),
                ));
            }
            scans.insert(request.scan_id.clone(), Arc::clone(&cancelled));
        }

        let result = self.scan_folder_inner(&request, &cancelled, &mut on_progress);
        self.scans.lock().remove(&request.scan_id);
        result
    }

    fn scan_folder_inner<F>(
        &self,
        request: &ScanModelFolderRequest,
        cancelled: &AtomicBool,
        on_progress: &mut F,
    ) -> AppResult<ModelScanSummary>
    where
        F: FnMut(ModelScanProgress),
    {
        let root = PathGrantService::folder(&request.path)?;
        let mut files = Vec::new();
        let mut pending = VecDeque::from([(root, 0_usize)]);
        let mut summary = ModelScanSummary {
            scan_id: request.scan_id.clone(),
            discovered: 0,
            processed: 0,
            imported: 0,
            duplicates: 0,
            invalid: 0,
            cancelled: false,
            issues: Vec::new(),
        };

        while let Some((folder, depth)) = pending.pop_front() {
            if cancelled.load(Ordering::Relaxed) {
                summary.cancelled = true;
                break;
            }
            if depth > MAX_SCAN_DEPTH {
                return Err(AppError::InvalidPath(format!(
                    "folder nesting exceeds the {MAX_SCAN_DEPTH}-level scan limit"
                )));
            }
            let mut entries = std::fs::read_dir(&folder)?.collect::<Result<Vec<_>, _>>()?;
            entries.sort_by_key(|entry| entry.path());
            for entry in entries {
                if cancelled.load(Ordering::Relaxed) {
                    summary.cancelled = true;
                    break;
                }
                let file_type = entry.file_type()?;
                if file_type.is_symlink() {
                    continue;
                }
                let path = entry.path();
                if file_type.is_dir() {
                    pending.push_back((path, depth + 1));
                } else if file_type.is_file() && has_gguf_extension(&path) {
                    if files.len() >= MAX_SCAN_FILES {
                        return Err(AppError::Operation(format!(
                            "the scan exceeds the {MAX_SCAN_FILES}-file safety limit"
                        )));
                    }
                    files.push(path.clone());
                    summary.discovered = files.len();
                    on_progress(progress_for(
                        &summary,
                        ScanPhase::Discovering,
                        Some(path.to_string_lossy().into_owned()),
                    ));
                }
            }
        }

        if !summary.cancelled {
            for path in files {
                if cancelled.load(Ordering::Relaxed) {
                    summary.cancelled = true;
                    break;
                }
                let path_text = path.to_string_lossy().into_owned();
                match self.import_model(&path_text) {
                    Ok(outcome) => {
                        if outcome.already_indexed {
                            summary.duplicates += 1;
                        } else if outcome.model.verification_state == VerificationState::Ready {
                            summary.imported += 1;
                        }
                        if outcome.model.verification_state == VerificationState::Invalid {
                            summary.invalid += 1;
                            if summary.issues.len() < MAX_REPORTED_ISSUES {
                                summary.issues.push(ModelScanIssue {
                                    path: path_text.clone(),
                                    message: outcome
                                        .model
                                        .verification_error
                                        .unwrap_or_else(|| "GGUF verification failed".into()),
                                });
                            }
                        }
                    }
                    Err(error) => {
                        summary.invalid += 1;
                        if summary.issues.len() < MAX_REPORTED_ISSUES {
                            summary.issues.push(ModelScanIssue {
                                path: path_text.clone(),
                                message: error.to_string(),
                            });
                        }
                    }
                }
                summary.processed += 1;
                on_progress(progress_for(
                    &summary,
                    ScanPhase::Importing,
                    Some(path_text),
                ));
            }
        }

        on_progress(progress_for(&summary, ScanPhase::Complete, None));
        Ok(summary)
    }
}

fn apply_inspection(model: &mut ModelRecord, granted: &GrantedModelFile, verified_at: String) {
    model.path = granted.canonical_path.to_string_lossy().into_owned();
    model.size_bytes = granted.size_bytes;
    model.modified_at_unix_ms = granted.modified_at_unix_ms;
    model.file_identity = granted.file_identity.clone();
    model.last_verified_at = Some(verified_at);
    match inspect_gguf(&granted.canonical_path) {
        Ok(metadata) => {
            model.display_name = metadata
                .name
                .as_ref()
                .filter(|value| !value.trim().is_empty())
                .cloned()
                .unwrap_or_else(|| granted.display_name.clone());
            model.family = metadata.architecture.clone();
            model.gguf_metadata = Some(metadata);
            model.verification_state = VerificationState::Ready;
            model.verification_error = None;
        }
        Err(error) => {
            model.display_name = granted.display_name.clone();
            model.family = None;
            model.gguf_metadata = None;
            model.verification_state = VerificationState::Invalid;
            model.verification_error = Some(error.to_string());
        }
    }
}

fn has_gguf_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .map(|value| value.eq_ignore_ascii_case("gguf"))
        .unwrap_or(false)
}

fn progress_for(
    summary: &ModelScanSummary,
    phase: ScanPhase,
    current_path: Option<String>,
) -> ModelScanProgress {
    ModelScanProgress {
        scan_id: summary.scan_id.clone(),
        phase,
        current_path,
        discovered: summary.discovered,
        processed: summary.processed,
        imported: summary.imported,
        duplicates: summary.duplicates,
        invalid: summary.invalid,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::gguf::write_test_gguf;

    struct TestDirectory(std::path::PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("neuraloc-model-test-{}", Uuid::new_v4()));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn new_service(directory: &TestDirectory) -> ModelService {
        let database = Arc::new(Database::open(&directory.0.join("test.db")).unwrap());
        ModelService::new(database)
    }

    #[test]
    fn imports_persists_and_reverifies_models() {
        let directory = TestDirectory::new();
        let model_path = directory.0.join("model.gguf");
        write_test_gguf(&model_path);
        let service = new_service(&directory);

        let imported = service.import_model(model_path.to_str().unwrap()).unwrap();
        assert!(!imported.already_indexed);
        assert_eq!(imported.model.verification_state, VerificationState::Ready);
        assert_eq!(service.list().unwrap().len(), 1);

        drop(service);
        assert_eq!(new_service(&directory).list().unwrap().len(), 1);
    }

    #[test]
    fn deduplicates_hard_links_by_file_identity() {
        let directory = TestDirectory::new();
        let first = directory.0.join("first.gguf");
        let second = directory.0.join("second.gguf");
        write_test_gguf(&first);
        std::fs::hard_link(&first, &second).unwrap();
        let service = new_service(&directory);

        let first_outcome = service.import_model(first.to_str().unwrap()).unwrap();
        let second_outcome = service.import_model(second.to_str().unwrap()).unwrap();
        assert!(second_outcome.already_indexed);
        assert_eq!(first_outcome.model.id, second_outcome.model.id);
        assert!(second_outcome.model.path.ends_with("second.gguf"));
        assert_eq!(service.list().unwrap().len(), 1);
    }

    #[test]
    fn records_malformed_and_missing_models_without_deleting_metadata() {
        let directory = TestDirectory::new();
        let model_path = directory.0.join("broken.gguf");
        std::fs::write(&model_path, b"not a gguf").unwrap();
        let service = new_service(&directory);
        let imported = service.import_model(model_path.to_str().unwrap()).unwrap();
        assert_eq!(
            imported.model.verification_state,
            VerificationState::Invalid
        );

        write_test_gguf(&model_path);
        let ready = service.reverify(&imported.model.id).unwrap();
        assert_eq!(ready.verification_state, VerificationState::Ready);
        std::fs::remove_file(&model_path).unwrap();
        let missing = service.reverify(&imported.model.id).unwrap();
        assert_eq!(missing.verification_state, VerificationState::Missing);
        assert!(missing.gguf_metadata.is_some());
    }

    #[test]
    fn folder_scans_can_be_cancelled() {
        let directory = TestDirectory::new();
        write_test_gguf(&directory.0.join("one.gguf"));
        write_test_gguf(&directory.0.join("two.gguf"));
        let service = new_service(&directory);
        let scan_id = Uuid::new_v4().to_string();
        let request = ScanModelFolderRequest {
            scan_id: scan_id.clone(),
            path: directory.0.to_string_lossy().into_owned(),
        };
        let summary = service
            .scan_folder(request, |progress| {
                if progress.discovered == 1 {
                    service.cancel_scan(&scan_id);
                }
            })
            .unwrap();
        assert!(summary.cancelled);
        assert!(summary.discovered <= 1);
    }
}
