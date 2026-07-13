use std::{
    collections::{HashMap, HashSet},
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use chrono::Utc;
use reqwest::{redirect, Client, Url};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::{
    errors::{AppError, AppResult},
    storage::Database,
};

use super::{
    repository::EnginePackageRepository, EnginePackageManifest, EnginePackageRecord,
    EnginePackageState, EnginePackageStatus, InstalledPackageFile,
};

const MAX_ARCHIVE_ENTRIES: usize = 512;
const MAX_ARCHIVE_SIZE: u64 = 2 * 1024 * 1024 * 1024;
const MAX_SINGLE_FILE_SIZE: u64 = 1024 * 1024 * 1024;
const MAX_UNCOMPRESSED_SIZE: u64 = 2 * 1024 * 1024 * 1024;
const HASH_BUFFER_SIZE: usize = 1024 * 1024;

pub struct EnginePackageService {
    repository: EnginePackageRepository,
    packages_root: PathBuf,
    downloads_root: PathBuf,
    manifests: HashMap<String, EnginePackageManifest>,
    client: Client,
}

impl EnginePackageService {
    pub fn new(database: Arc<Database>, data_directory: &Path) -> AppResult<Self> {
        let manifest: EnginePackageManifest = serde_json::from_str(include_str!(
            "../../manifests/llama-cpp-b9986-windows-x86_64-cpu.json"
        ))
        .map_err(|error| {
            AppError::EnginePackage(format!("the bundled package manifest is invalid: {error}"))
        })?;
        validate_manifest(&manifest)?;
        let packages_root = data_directory.join("engines");
        let downloads_root = data_directory.join("downloads").join("engines");
        std::fs::create_dir_all(&packages_root)?;
        std::fs::create_dir_all(&downloads_root)?;
        let client = Client::builder()
            .user_agent(concat!("NeuraLoc-Core/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(15 * 60))
            .redirect(redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= 5 {
                    return attempt.error("engine package redirect limit exceeded");
                }
                if approved_download_url(attempt.url()) {
                    attempt.follow()
                } else {
                    attempt.error("engine package redirect host is not approved")
                }
            }))
            .build()
            .map_err(|error| {
                AppError::EnginePackage(format!("the package downloader could not start: {error}"))
            })?;
        let service = Self {
            repository: EnginePackageRepository::new(database),
            packages_root,
            downloads_root,
            manifests: HashMap::from([(manifest.id.clone(), manifest)]),
            client,
        };
        service.reconcile_records()?;
        Ok(service)
    }

    pub fn statuses(&self) -> AppResult<Vec<EnginePackageStatus>> {
        let records: HashMap<_, _> = self
            .repository
            .list()?
            .into_iter()
            .map(|record| (record.id.clone(), record))
            .collect();
        let mut statuses: Vec<_> = self
            .manifests
            .values()
            .cloned()
            .map(|manifest| EnginePackageStatus {
                installation: records.get(&manifest.id).cloned(),
                manifest,
            })
            .collect();
        statuses.sort_by(|left, right| left.manifest.id.cmp(&right.manifest.id));
        Ok(statuses)
    }

    pub async fn install_download(
        &self,
        package_id: &str,
        internet_enabled: bool,
    ) -> AppResult<EnginePackageRecord> {
        if !internet_enabled {
            return Err(AppError::EnginePackage(
                "internet access is disabled in Settings".into(),
            ));
        }
        let manifest = self.manifest(package_id)?.clone();
        let partial_path = self
            .downloads_root
            .join(format!("{}.partial", manifest.archive_file_name));
        self.repository
            .upsert(&self.pending_record(&manifest, Some(manifest.source_url.clone())))?;

        let result = async {
            self.download(&manifest, &partial_path).await?;
            let install_root = self.packages_root.clone();
            let archive_path = partial_path.clone();
            let install_manifest = manifest.clone();
            let (install_path, files) = tokio::task::spawn_blocking(move || {
                install_archive(&install_manifest, &archive_path, &install_root)
            })
            .await
            .map_err(|error| {
                AppError::EnginePackage(format!("the package install task stopped: {error}"))
            })??;
            Ok(self.ready_record(
                &manifest,
                install_path,
                files,
                Some(manifest.source_url.clone()),
            ))
        }
        .await;

        let _ = tokio::fs::remove_file(&partial_path).await;
        self.finish_install(&manifest, result, Some(manifest.source_url.clone()))
    }

    pub async fn install_offline(
        &self,
        package_id: &str,
        archive_path: &str,
    ) -> AppResult<EnginePackageRecord> {
        let manifest = self.manifest(package_id)?.clone();
        let archive_path = validate_offline_archive(archive_path)?;
        self.repository
            .upsert(&self.pending_record(&manifest, None))?;
        let install_root = self.packages_root.clone();
        let install_manifest = manifest.clone();
        let result = tokio::task::spawn_blocking(move || {
            install_archive(&install_manifest, &archive_path, &install_root)
        })
        .await
        .map_err(|error| {
            AppError::EnginePackage(format!("the package install task stopped: {error}"))
        })?
        .map(|(install_path, files)| self.ready_record(&manifest, install_path, files, None));
        self.finish_install(&manifest, result, None)
    }

    pub async fn verify(&self, package_id: &str) -> AppResult<EnginePackageRecord> {
        let manifest = self.manifest(package_id)?.clone();
        let mut record = self.repository.get(package_id)?.ok_or_else(|| {
            AppError::EnginePackage(format!("engine package {package_id} is not installed"))
        })?;
        let expected_path = self.install_path(&manifest);
        if !paths_equal(Path::new(&record.install_path), &expected_path) {
            return Err(AppError::EnginePackage(
                "the stored package path does not match the bundled manifest".into(),
            ));
        }
        let files = record.files.clone();
        let verification_path = expected_path.clone();
        let result =
            tokio::task::spawn_blocking(move || verify_installed_files(&verification_path, &files))
                .await
                .map_err(|error| {
                    AppError::EnginePackage(format!("the package verify task stopped: {error}"))
                })?;
        match result {
            Ok(()) => {
                record.state = EnginePackageState::Ready;
                record.error = None;
                record.verified_at = Some(Utc::now().to_rfc3339());
                self.repository.upsert(&record)?;
                Ok(record)
            }
            Err(error) => {
                record.state = if expected_path.exists() {
                    EnginePackageState::Invalid
                } else {
                    EnginePackageState::Missing
                };
                record.error = Some(error.to_string());
                record.verified_at = Some(Utc::now().to_rfc3339());
                self.repository.upsert(&record)?;
                Err(error)
            }
        }
    }

    pub async fn uninstall(&self, package_id: &str) -> AppResult<()> {
        let manifest = self.manifest(package_id)?.clone();
        let record = self.repository.get(package_id)?.ok_or_else(|| {
            AppError::EnginePackage(format!("engine package {package_id} is not installed"))
        })?;
        let install_path = self.install_path(&manifest);
        if !paths_equal(Path::new(&record.install_path), &install_path) {
            return Err(AppError::EnginePackage(
                "the stored package path does not match the bundled manifest".into(),
            ));
        }
        let packages_root = self.packages_root.clone();
        tokio::task::spawn_blocking(move || {
            remove_internal_directory(&packages_root, &install_path)
        })
        .await
        .map_err(|error| {
            AppError::EnginePackage(format!("the package removal task stopped: {error}"))
        })??;
        self.repository.remove(package_id)?;
        Ok(())
    }

    fn manifest(&self, package_id: &str) -> AppResult<&EnginePackageManifest> {
        self.manifests.get(package_id).ok_or_else(|| {
            AppError::EnginePackage(format!("engine package {package_id} is not in the catalog"))
        })
    }

    fn install_path(&self, manifest: &EnginePackageManifest) -> PathBuf {
        self.packages_root.join(&manifest.id)
    }

    fn pending_record(
        &self,
        manifest: &EnginePackageManifest,
        source_url: Option<String>,
    ) -> EnginePackageRecord {
        EnginePackageRecord {
            id: manifest.id.clone(),
            engine_id: manifest.engine_id.clone(),
            version: manifest.version.clone(),
            platform: manifest.platform.clone(),
            architecture: manifest.architecture.clone(),
            route: manifest.route.clone(),
            install_path: self.install_path(manifest).to_string_lossy().into_owned(),
            archive_sha256: manifest.archive_sha256.clone(),
            files: Vec::new(),
            state: EnginePackageState::Installing,
            source_url,
            error: None,
            installed_at: None,
            verified_at: None,
        }
    }

    fn ready_record(
        &self,
        manifest: &EnginePackageManifest,
        install_path: PathBuf,
        files: Vec<InstalledPackageFile>,
        source_url: Option<String>,
    ) -> EnginePackageRecord {
        let now = Utc::now().to_rfc3339();
        EnginePackageRecord {
            id: manifest.id.clone(),
            engine_id: manifest.engine_id.clone(),
            version: manifest.version.clone(),
            platform: manifest.platform.clone(),
            architecture: manifest.architecture.clone(),
            route: manifest.route.clone(),
            install_path: install_path.to_string_lossy().into_owned(),
            archive_sha256: manifest.archive_sha256.clone(),
            files,
            state: EnginePackageState::Ready,
            source_url,
            error: None,
            installed_at: Some(now.clone()),
            verified_at: Some(now),
        }
    }

    fn finish_install(
        &self,
        manifest: &EnginePackageManifest,
        result: AppResult<EnginePackageRecord>,
        source_url: Option<String>,
    ) -> AppResult<EnginePackageRecord> {
        match result {
            Ok(record) => {
                self.repository.upsert(&record)?;
                Ok(record)
            }
            Err(error) => {
                let mut record = self.pending_record(manifest, source_url);
                record.state = EnginePackageState::Invalid;
                record.error = Some(error.to_string());
                record.verified_at = Some(Utc::now().to_rfc3339());
                self.repository.upsert(&record)?;
                Err(error)
            }
        }
    }

    fn reconcile_records(&self) -> AppResult<()> {
        for mut record in self.repository.list()? {
            let Some(manifest) = self.manifests.get(&record.id) else {
                continue;
            };
            let expected_path = self.install_path(manifest);
            let mut changed = false;
            if !paths_equal(Path::new(&record.install_path), &expected_path) {
                record.state = EnginePackageState::Invalid;
                record.error =
                    Some("the stored package path does not match the bundled manifest".into());
                changed = true;
            } else if record.state == EnginePackageState::Installing {
                record.state = EnginePackageState::Invalid;
                record.error = Some("the previous package install was interrupted".into());
                changed = true;
            } else if record.state == EnginePackageState::Ready && !expected_path.is_dir() {
                record.state = EnginePackageState::Missing;
                record.error = Some("the installed package directory is missing".into());
                changed = true;
            }
            if changed {
                record.verified_at = Some(Utc::now().to_rfc3339());
                self.repository.upsert(&record)?;
            }
        }
        Ok(())
    }

    async fn download(&self, manifest: &EnginePackageManifest, path: &Path) -> AppResult<()> {
        let url = Url::parse(&manifest.source_url).map_err(|error| {
            AppError::EnginePackage(format!("the package URL is invalid: {error}"))
        })?;
        if !approved_download_url(&url) {
            return Err(AppError::EnginePackage(
                "the package URL is not on an approved HTTPS host".into(),
            ));
        }
        let mut response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|error| AppError::EnginePackage(format!("download failed: {error}")))?
            .error_for_status()
            .map_err(|error| AppError::EnginePackage(format!("download failed: {error}")))?;
        if let Some(length) = response.content_length() {
            if length != manifest.archive_size_bytes {
                return Err(AppError::EnginePackage(format!(
                    "download size is {length} bytes; expected {}",
                    manifest.archive_size_bytes
                )));
            }
        }
        let mut output = tokio::fs::File::create(path).await?;
        let mut hasher = Sha256::new();
        let mut received = 0_u64;
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| AppError::EnginePackage(format!("download failed: {error}")))?
        {
            received = received.checked_add(chunk.len() as u64).ok_or_else(|| {
                AppError::EnginePackage("the package download size overflowed".into())
            })?;
            if received > manifest.archive_size_bytes {
                return Err(AppError::EnginePackage(
                    "the package download exceeded its expected size".into(),
                ));
            }
            hasher.update(&chunk);
            output.write_all(&chunk).await?;
        }
        output.sync_all().await?;
        if received != manifest.archive_size_bytes {
            return Err(AppError::EnginePackage(format!(
                "download ended at {received} bytes; expected {}",
                manifest.archive_size_bytes
            )));
        }
        let digest = finalize_sha256(hasher);
        if digest != manifest.archive_sha256 {
            return Err(AppError::EnginePackage(
                "the downloaded package checksum did not match the manifest".into(),
            ));
        }
        Ok(())
    }
}

fn validate_manifest(manifest: &EnginePackageManifest) -> AppResult<()> {
    if manifest.manifest_version != 1 {
        return Err(AppError::EnginePackage(
            "the package manifest version is unsupported".into(),
        ));
    }
    for value in [
        &manifest.id,
        &manifest.engine_id,
        &manifest.version,
        &manifest.platform,
        &manifest.architecture,
        &manifest.route,
    ] {
        if value.is_empty()
            || value.len() > 128
            || !value
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || ".-_".contains(character))
        {
            return Err(AppError::EnginePackage(
                "the package manifest contains an invalid identifier".into(),
            ));
        }
    }
    if manifest.platform != "windows" || manifest.architecture != "x86_64" {
        return Err(AppError::EnginePackage(
            "the bundled package does not target Windows x64".into(),
        ));
    }
    if manifest.archive_size_bytes == 0 || manifest.archive_size_bytes > MAX_ARCHIVE_SIZE {
        return Err(AppError::EnginePackage(
            "the package archive size exceeds its safety limit".into(),
        ));
    }
    if manifest.archive_sha256.len() != 64
        || !manifest
            .archive_sha256
            .chars()
            .all(|character| character.is_ascii_hexdigit() && !character.is_ascii_uppercase())
    {
        return Err(AppError::EnginePackage(
            "the package SHA-256 is malformed".into(),
        ));
    }
    if Path::new(&manifest.archive_file_name).file_name()
        != Some(manifest.archive_file_name.as_ref())
        || !manifest.archive_file_name.ends_with(".zip")
    {
        return Err(AppError::EnginePackage(
            "the package archive filename is invalid".into(),
        ));
    }
    let url = Url::parse(&manifest.source_url)
        .map_err(|error| AppError::EnginePackage(format!("the package URL is invalid: {error}")))?;
    if !approved_download_url(&url) {
        return Err(AppError::EnginePackage(
            "the package URL is not on an approved HTTPS host".into(),
        ));
    }
    if manifest.expected_files.is_empty() {
        return Err(AppError::EnginePackage(
            "the package manifest has no expected files".into(),
        ));
    }
    let mut unique = HashSet::new();
    for path in &manifest.expected_files {
        if !safe_relative_path(Path::new(path)) || !unique.insert(path.to_ascii_lowercase()) {
            return Err(AppError::EnginePackage(
                "the package manifest contains an unsafe or duplicate expected path".into(),
            ));
        }
    }
    Ok(())
}

fn approved_download_url(url: &Url) -> bool {
    if url.scheme() != "https" || url.port().is_some() || !url.username().is_empty() {
        return false;
    }
    let Some(host) = url.host_str().map(str::to_ascii_lowercase) else {
        return false;
    };
    host == "github.com" || host.ends_with(".githubusercontent.com")
}

fn validate_offline_archive(raw_path: &str) -> AppResult<PathBuf> {
    if raw_path.trim().is_empty() || raw_path.contains('\0') {
        return Err(AppError::InvalidPath(
            "the selected package archive path is malformed".into(),
        ));
    }
    let normalized = raw_path.replace('/', "\\");
    if normalized.starts_with(r"\\.\") || normalized.starts_with(r"\\?\") {
        return Err(AppError::InvalidPath(
            "Windows device paths are not accepted for package imports".into(),
        ));
    }
    let path = Path::new(raw_path);
    if !path.is_absolute()
        || path
            .components()
            .any(|component| component == Component::ParentDir)
    {
        return Err(AppError::InvalidPath(
            "the selected package archive path must be absolute and traversal-free".into(),
        ));
    }
    let link_metadata = std::fs::symlink_metadata(path)?;
    if link_metadata.file_type().is_symlink() {
        return Err(AppError::InvalidPath(
            "symbolic links are not accepted for package imports".into(),
        ));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if link_metadata.file_attributes() & 0x400 != 0 {
            return Err(AppError::InvalidPath(
                "Windows reparse points are not accepted for package imports".into(),
            ));
        }
    }
    let canonical = std::fs::canonicalize(path)?;
    if !std::fs::metadata(&canonical)?.is_file()
        || canonical
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| !value.eq_ignore_ascii_case("zip"))
            .unwrap_or(true)
    {
        return Err(AppError::InvalidPath(
            "the selected package must be a regular .zip file".into(),
        ));
    }
    Ok(canonical)
}

fn install_archive(
    manifest: &EnginePackageManifest,
    archive_path: &Path,
    packages_root: &Path,
) -> AppResult<(PathBuf, Vec<InstalledPackageFile>)> {
    let (archive_size, archive_sha256) = hash_file(archive_path)?;
    if archive_size != manifest.archive_size_bytes || archive_sha256 != manifest.archive_sha256 {
        return Err(AppError::EnginePackage(
            "the package archive does not match the pinned size and SHA-256".into(),
        ));
    }
    let target = packages_root.join(&manifest.id);
    let staging = packages_root.join(format!(".staging-{}", Uuid::new_v4()));
    let backup = packages_root.join(format!(".backup-{}", Uuid::new_v4()));
    std::fs::create_dir(&staging)?;
    let extraction = extract_archive(archive_path, &staging, manifest);
    let files = match extraction {
        Ok(files) => files,
        Err(error) => {
            let _ = remove_internal_directory(packages_root, &staging);
            return Err(error);
        }
    };

    let had_existing = target.exists();
    if had_existing {
        std::fs::rename(&target, &backup)?;
    }
    if let Err(error) = std::fs::rename(&staging, &target) {
        if had_existing {
            let _ = std::fs::rename(&backup, &target);
        }
        let _ = remove_internal_directory(packages_root, &staging);
        return Err(error.into());
    }
    if had_existing {
        let _ = remove_internal_directory(packages_root, &backup);
    }
    Ok((std::fs::canonicalize(target)?, files))
}

fn extract_archive(
    archive_path: &Path,
    staging: &Path,
    manifest: &EnginePackageManifest,
) -> AppResult<Vec<InstalledPackageFile>> {
    let file = File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file).map_err(|error| {
        AppError::EnginePackage(format!("the package ZIP could not be opened: {error}"))
    })?;
    if archive.is_empty() || archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err(AppError::EnginePackage(format!(
            "the package ZIP exceeds the {MAX_ARCHIVE_ENTRIES}-entry safety limit"
        )));
    }
    let mut total_uncompressed = 0_u64;
    let extraction_limit = manifest
        .archive_size_bytes
        .saturating_mul(12)
        .min(MAX_UNCOMPRESSED_SIZE);
    let mut files = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| {
            AppError::EnginePackage(format!("the package ZIP entry is invalid: {error}"))
        })?;
        let relative = entry.enclosed_name().ok_or_else(|| {
            AppError::EnginePackage("the package ZIP contains an unsafe path".into())
        })?;
        if !safe_relative_path(&relative) || entry.is_symlink() {
            return Err(AppError::EnginePackage(
                "the package ZIP contains a traversal path or symbolic link".into(),
            ));
        }
        if entry.size() > MAX_SINGLE_FILE_SIZE {
            return Err(AppError::EnginePackage(
                "a package file exceeds the per-file extraction limit".into(),
            ));
        }
        total_uncompressed = total_uncompressed
            .checked_add(entry.size())
            .ok_or_else(|| AppError::EnginePackage("package size overflow".into()))?;
        if total_uncompressed > extraction_limit {
            return Err(AppError::EnginePackage(
                "the package exceeds the total extraction limit".into(),
            ));
        }
        let destination = staging.join(&relative);
        if entry.is_dir() {
            std::fs::create_dir_all(&destination)?;
            continue;
        }
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)?;
        let mut hasher = Sha256::new();
        let mut written = 0_u64;
        let mut buffer = vec![0_u8; HASH_BUFFER_SIZE];
        loop {
            let read = entry.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            written = written
                .checked_add(read as u64)
                .ok_or_else(|| AppError::EnginePackage("extracted package size overflow".into()))?;
            if written > entry.size() || written > MAX_SINGLE_FILE_SIZE {
                return Err(AppError::EnginePackage(
                    "an extracted package file exceeded its declared size".into(),
                ));
            }
            hasher.update(&buffer[..read]);
            output.write_all(&buffer[..read])?;
        }
        output.sync_all()?;
        if written != entry.size() {
            return Err(AppError::EnginePackage(
                "an extracted package file ended before its declared size".into(),
            ));
        }
        files.push(InstalledPackageFile {
            path: relative.to_string_lossy().replace('\\', "/"),
            size_bytes: written,
            sha256: finalize_sha256(hasher),
        });
    }
    let actual: HashSet<_> = files
        .iter()
        .map(|file| file.path.to_ascii_lowercase())
        .collect();
    for expected in &manifest.expected_files {
        if !actual.contains(&expected.to_ascii_lowercase()) {
            return Err(AppError::EnginePackage(format!(
                "the package is missing expected file {expected}"
            )));
        }
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(files)
}

fn verify_installed_files(root: &Path, files: &[InstalledPackageFile]) -> AppResult<()> {
    let root_metadata = std::fs::symlink_metadata(root).map_err(|_| {
        AppError::EnginePackage("the installed package directory is missing".into())
    })?;
    if root_metadata.file_type().is_symlink()
        || is_reparse_point(&root_metadata)
        || !root_metadata.is_dir()
        || files.is_empty()
    {
        return Err(AppError::EnginePackage(
            "the installed package directory or file inventory is missing".into(),
        ));
    }
    let actual_paths = collect_installed_paths(root)?;
    let expected_paths: HashSet<_> = files
        .iter()
        .map(|file| file.path.to_ascii_lowercase())
        .collect();
    if actual_paths != expected_paths {
        return Err(AppError::EnginePackage(
            "the installed package contains missing or untracked files".into(),
        ));
    }
    for expected in files {
        let relative = Path::new(&expected.path);
        if !safe_relative_path(relative) {
            return Err(AppError::EnginePackage(
                "the installed package inventory contains an unsafe path".into(),
            ));
        }
        let path = root.join(relative);
        let metadata = std::fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() || is_reparse_point(&metadata) || !metadata.is_file() {
            return Err(AppError::EnginePackage(format!(
                "installed package file {} is missing or unsafe",
                expected.path
            )));
        }
        let (size, sha256) = hash_file(&path)?;
        if size != expected.size_bytes || sha256 != expected.sha256 {
            return Err(AppError::EnginePackage(format!(
                "installed package file {} failed verification",
                expected.path
            )));
        }
    }
    Ok(())
}

fn collect_installed_paths(root: &Path) -> AppResult<HashSet<String>> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = HashSet::new();
    let mut entries_seen = 0_usize;
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory)? {
            let entry = entry?;
            entries_seen += 1;
            if entries_seen > MAX_ARCHIVE_ENTRIES {
                return Err(AppError::EnginePackage(
                    "the installed package exceeds its entry limit".into(),
                ));
            }
            let path = entry.path();
            let metadata = std::fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
                return Err(AppError::EnginePackage(
                    "the installed package contains a link or reparse point".into(),
                ));
            }
            if metadata.is_dir() {
                pending.push(path);
            } else if metadata.is_file() {
                let relative = path.strip_prefix(root).map_err(|_| {
                    AppError::EnginePackage(
                        "an installed package path escaped the package root".into(),
                    )
                })?;
                if !safe_relative_path(relative) {
                    return Err(AppError::EnginePackage(
                        "the installed package contains an unsafe path".into(),
                    ));
                }
                files.insert(
                    relative
                        .to_string_lossy()
                        .replace('\\', "/")
                        .to_ascii_lowercase(),
                );
            } else {
                return Err(AppError::EnginePackage(
                    "the installed package contains a non-file entry".into(),
                ));
            }
        }
    }
    Ok(files)
}

fn hash_file(path: &Path) -> AppResult<(u64, String)> {
    let mut file = File::open(path)?;
    let mut buffer = vec![0_u8; HASH_BUFFER_SIZE];
    let mut hasher = Sha256::new();
    let mut size = 0_u64;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        size = size
            .checked_add(read as u64)
            .ok_or_else(|| AppError::EnginePackage("file size overflow".into()))?;
        hasher.update(&buffer[..read]);
    }
    Ok((size, finalize_sha256(hasher)))
}

fn finalize_sha256(hasher: Sha256) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = hasher.finalize();
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn safe_relative_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path.components().all(|component| match component {
            Component::Normal(value) => safe_windows_name(&value.to_string_lossy()),
            _ => false,
        })
}

fn paths_equal(left: &Path, right: &Path) -> bool {
    let left = std::fs::canonicalize(left).unwrap_or_else(|_| left.to_path_buf());
    let right = std::fs::canonicalize(right).unwrap_or_else(|_| right.to_path_buf());
    #[cfg(windows)]
    {
        normalize_windows_path(&left).eq_ignore_ascii_case(&normalize_windows_path(&right))
    }
    #[cfg(not(windows))]
    {
        left == right
    }
}

#[cfg(windows)]
fn normalize_windows_path(path: &Path) -> String {
    let value = path.to_string_lossy();
    if let Some(stripped) = value.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{stripped}")
    } else if let Some(stripped) = value.strip_prefix(r"\\?\") {
        stripped.to_owned()
    } else {
        value.into_owned()
    }
}

fn safe_windows_name(value: &str) -> bool {
    if value.is_empty()
        || !value.is_ascii()
        || value.ends_with(['.', ' '])
        || value
            .chars()
            .any(|character| character <= '\u{1f}' || "<>:\"/\\|?*".contains(character))
    {
        return false;
    }
    let stem = value
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end_matches(['.', ' '])
        .to_ascii_uppercase();
    !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        && !(stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit()
            && stem.as_bytes()[3] != b'0')
}

fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        let _ = metadata;
        false
    }
}

fn remove_internal_directory(root: &Path, path: &Path) -> AppResult<()> {
    if path.parent() != Some(root) {
        return Err(AppError::EnginePackage(
            "refusing to remove a directory outside the package root".into(),
        ));
    }
    if path.exists() {
        let metadata = std::fs::symlink_metadata(path)?;
        if metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
            return Err(AppError::EnginePackage(
                "refusing to remove a linked package directory".into(),
            ));
        }
        std::fs::remove_dir_all(path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use zip::{write::SimpleFileOptions, ZipWriter};

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("neuraloc-package-test-{}", Uuid::new_v4()));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn write_archive(path: &Path, entry_name: &str, content: &[u8]) -> (u64, String) {
        let file = File::create(path).unwrap();
        let mut zip = ZipWriter::new(file);
        zip.start_file(entry_name, SimpleFileOptions::default())
            .unwrap();
        zip.write_all(content).unwrap();
        zip.finish().unwrap();
        hash_file(path).unwrap()
    }

    fn fixture_manifest(size: u64, sha256: String) -> EnginePackageManifest {
        EnginePackageManifest {
            manifest_version: 1,
            id: "llama.cpp-test-windows-x86_64-cpu".into(),
            engine_id: "llama.cpp".into(),
            version: "test".into(),
            platform: "windows".into(),
            architecture: "x86_64".into(),
            route: "cpu".into(),
            source_url: "https://github.com/example/package.zip".into(),
            archive_file_name: "package.zip".into(),
            archive_size_bytes: size,
            archive_sha256: sha256,
            expected_files: vec!["llama-server.exe".into()],
        }
    }

    #[test]
    fn bundled_manifest_is_valid() {
        let manifest: EnginePackageManifest = serde_json::from_str(include_str!(
            "../../manifests/llama-cpp-b9986-windows-x86_64-cpu.json"
        ))
        .unwrap();
        validate_manifest(&manifest).unwrap();
    }

    #[test]
    fn installs_and_detects_tampering_from_the_file_inventory() {
        let directory = TestDirectory::new();
        let archive = directory.0.join("package.zip");
        let (size, sha256) = write_archive(&archive, "llama-server.exe", b"fixture server");
        let manifest = fixture_manifest(size, sha256);
        let packages_root = directory.0.join("engines");
        std::fs::create_dir(&packages_root).unwrap();

        let (installed, files) = install_archive(&manifest, &archive, &packages_root).unwrap();
        verify_installed_files(&installed, &files).unwrap();
        std::fs::write(installed.join("untracked.dll"), b"unexpected").unwrap();
        assert!(verify_installed_files(&installed, &files).is_err());
        std::fs::remove_file(installed.join("untracked.dll")).unwrap();
        std::fs::write(installed.join("llama-server.exe"), b"tampered").unwrap();
        assert!(verify_installed_files(&installed, &files).is_err());
    }

    #[test]
    fn rejects_archives_with_traversal_entries() {
        let directory = TestDirectory::new();
        let archive = directory.0.join("package.zip");
        let (size, sha256) = write_archive(&archive, "../escape.exe", b"unsafe");
        let mut manifest = fixture_manifest(size, sha256);
        manifest.expected_files = vec!["escape.exe".into()];
        let packages_root = directory.0.join("engines");
        std::fs::create_dir(&packages_root).unwrap();

        assert!(install_archive(&manifest, &archive, &packages_root).is_err());
        assert!(!directory.0.join("escape.exe").exists());
    }

    #[test]
    fn rejects_archives_with_the_wrong_checksum() {
        let directory = TestDirectory::new();
        let archive = directory.0.join("package.zip");
        let (size, _) = write_archive(&archive, "llama-server.exe", b"fixture server");
        let manifest = fixture_manifest(size, "0".repeat(64));
        let packages_root = directory.0.join("engines");
        std::fs::create_dir(&packages_root).unwrap();
        assert!(install_archive(&manifest, &archive, &packages_root).is_err());
    }

    #[test]
    fn rejects_windows_device_names_and_alternate_data_streams() {
        assert!(!safe_relative_path(Path::new("CON.dll")));
        assert!(!safe_relative_path(Path::new("server.exe:payload")));
        assert!(!safe_relative_path(Path::new("folder/LPT1.txt")));
        assert!(!safe_relative_path(Path::new("folder/llam\u{e1}.dll")));
        assert!(safe_relative_path(Path::new("folder/llama-server.exe")));
        assert!(validate_offline_archive(r"\\?\C:\package.zip").is_err());
    }

    #[tokio::test]
    #[ignore = "downloads the pinned official llama.cpp archive"]
    async fn installs_verifies_and_uninstalls_the_pinned_package() {
        let directory = TestDirectory::new();
        let database = Arc::new(Database::open(&directory.0.join("test.db")).unwrap());
        let service = EnginePackageService::new(database, &directory.0).unwrap();
        let package_id = "llama.cpp-b9986-windows-x86_64-cpu";

        let installed = service.install_download(package_id, true).await.unwrap();
        assert_eq!(installed.state, EnginePackageState::Ready);
        assert!(installed
            .files
            .iter()
            .any(|file| file.path == "llama-server.exe"));
        service.verify(package_id).await.unwrap();
        service.uninstall(package_id).await.unwrap();
        assert!(service.statuses().unwrap()[0].installation.is_none());
    }
}
