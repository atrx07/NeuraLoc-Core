use std::{
    fs::Metadata,
    path::{Component, Path, PathBuf},
    time::UNIX_EPOCH,
};

use crate::errors::{AppError, AppResult};

pub struct GrantedModelFile {
    pub canonical_path: PathBuf,
    pub display_name: String,
    pub size_bytes: u64,
    pub modified_at_unix_ms: i64,
    pub file_identity: Option<String>,
}

pub struct PathGrantService;

impl PathGrantService {
    pub fn model_file(raw_path: &str) -> AppResult<GrantedModelFile> {
        let path = validate_input_path(raw_path)?;
        let link_metadata = std::fs::symlink_metadata(path)?;
        reject_reparse_point(&link_metadata)?;

        let canonical_path = canonicalize_for_storage(path)?;
        let metadata = std::fs::metadata(&canonical_path)?;
        if !metadata.is_file() {
            return Err(AppError::InvalidPath(
                "the selected path is not a regular file".into(),
            ));
        }
        if canonical_path
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| !value.eq_ignore_ascii_case("gguf"))
            .unwrap_or(true)
        {
            return Err(AppError::InvalidPath(
                "only .gguf model files can be imported".into(),
            ));
        }

        let display_name = canonical_path
            .file_stem()
            .and_then(|value| value.to_str())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("Unnamed GGUF model")
            .to_string();
        let modified_at_unix_ms = metadata
            .modified()
            .ok()
            .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
            .and_then(|value| i64::try_from(value.as_millis()).ok())
            .unwrap_or(0);

        checked_sqlite_size(metadata.len())?;
        let file_identity = file_identity(&canonical_path, &metadata);
        Ok(GrantedModelFile {
            canonical_path,
            display_name,
            size_bytes: metadata.len(),
            modified_at_unix_ms,
            file_identity,
        })
    }

    pub fn folder(raw_path: &str) -> AppResult<PathBuf> {
        let path = validate_input_path(raw_path)?;
        let link_metadata = std::fs::symlink_metadata(path)?;
        reject_reparse_point(&link_metadata)?;
        let canonical_path = canonicalize_for_storage(path)?;
        if !std::fs::metadata(&canonical_path)?.is_dir() {
            return Err(AppError::InvalidPath(
                "the selected path is not a folder".into(),
            ));
        }
        Ok(canonical_path)
    }
}

fn validate_input_path(raw_path: &str) -> AppResult<&Path> {
    if raw_path.trim().is_empty() || raw_path.contains('\0') {
        return Err(AppError::InvalidPath(
            "the selected path is empty or malformed".into(),
        ));
    }
    let normalized = raw_path.replace('/', "\\");
    if normalized.starts_with(r"\\.\") || normalized.starts_with(r"\\?\") {
        return Err(AppError::InvalidPath(
            "Windows device paths are not accepted".into(),
        ));
    }
    let path = Path::new(raw_path);
    if !path.is_absolute() {
        return Err(AppError::InvalidPath(
            "the selected path must be absolute".into(),
        ));
    }
    if path
        .components()
        .any(|component| component == Component::ParentDir)
    {
        return Err(AppError::InvalidPath(
            "parent-directory traversal is not accepted".into(),
        ));
    }
    Ok(path)
}

fn canonicalize_for_storage(path: &Path) -> AppResult<PathBuf> {
    let canonical = std::fs::canonicalize(path)?;
    #[cfg(windows)]
    {
        let value = canonical.to_string_lossy();
        if let Some(stripped) = value.strip_prefix("\\\\?\\UNC\\") {
            return Ok(PathBuf::from(format!("\\\\{stripped}")));
        }
        if let Some(stripped) = value.strip_prefix("\\\\?\\") {
            return Ok(PathBuf::from(stripped));
        }
    }
    Ok(canonical)
}

fn reject_reparse_point(metadata: &Metadata) -> AppResult<()> {
    if metadata.file_type().is_symlink() {
        return Err(AppError::InvalidPath(
            "symbolic links are not accepted for model imports".into(),
        ));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(AppError::InvalidPath(
                "Windows reparse points are not accepted for model imports".into(),
            ));
        }
    }
    Ok(())
}

fn checked_sqlite_size(size: u64) -> AppResult<i64> {
    i64::try_from(size)
        .map_err(|_| AppError::InvalidModel("the file is too large to index safely".into()))
}

#[cfg(windows)]
fn file_identity(path: &Path, _metadata: &Metadata) -> Option<String> {
    use std::{fs::File, mem::MaybeUninit, os::windows::io::AsRawHandle};
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
    };

    let file = File::open(path).ok()?;
    let mut information = MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::zeroed();
    let succeeded =
        unsafe { GetFileInformationByHandle(file.as_raw_handle(), information.as_mut_ptr()) };
    if succeeded == 0 {
        return None;
    }
    let information = unsafe { information.assume_init() };
    let index = ((information.nFileIndexHigh as u64) << 32) | information.nFileIndexLow as u64;
    Some(format!(
        "windows:{:08x}:{index:016x}",
        information.dwVolumeSerialNumber
    ))
}

#[cfg(unix)]
fn file_identity(_path: &Path, metadata: &Metadata) -> Option<String> {
    use std::os::unix::fs::MetadataExt;
    Some(format!("unix:{}:{}", metadata.dev(), metadata.ino()))
}

#[cfg(not(any(windows, unix)))]
fn file_identity(_path: &Path, _metadata: &Metadata) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_sizes_that_do_not_fit_sqlite_integers() {
        assert!(checked_sqlite_size(i64::MAX as u64).is_ok());
        assert!(checked_sqlite_size(i64::MAX as u64 + 1).is_err());
    }

    #[test]
    fn rejects_relative_and_traversing_paths() {
        assert!(validate_input_path("model.gguf").is_err());
        let traversing = if cfg!(windows) {
            r"C:\models\..\model.gguf"
        } else {
            "/models/../model.gguf"
        };
        assert!(validate_input_path(traversing).is_err());
    }

    #[cfg(any(windows, unix))]
    #[test]
    fn rejects_symbolic_link_files() {
        use uuid::Uuid;

        let directory = std::env::temp_dir().join(format!("neuraloc-link-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let target = directory.join("target.gguf");
        let link = directory.join("link.gguf");
        std::fs::write(&target, b"GGUF").unwrap();

        #[cfg(windows)]
        let created = std::os::windows::fs::symlink_file(&target, &link);
        #[cfg(unix)]
        let created = std::os::unix::fs::symlink(&target, &link);

        match created {
            Ok(()) => assert!(PathGrantService::model_file(link.to_str().unwrap()).is_err()),
            Err(error)
                if error.kind() == std::io::ErrorKind::PermissionDenied
                    || (cfg!(windows) && error.raw_os_error() == Some(1314)) => {}
            Err(error) => panic!("symbolic link fixture failed: {error}"),
        }
        let _ = std::fs::remove_dir_all(directory);
    }
}
