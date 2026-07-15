use std::{
    fs::Metadata,
    path::{Component, Path, PathBuf},
};

use crate::errors::{AppError, AppResult};

use super::parser::MAX_PROMPT_BYTES;

pub(crate) struct GrantedPromptFile {
    pub canonical_path: PathBuf,
    pub fallback_name: String,
}

pub(crate) fn prompt_file(raw_path: &str) -> AppResult<GrantedPromptFile> {
    if raw_path.trim().is_empty() || raw_path.contains('\0') {
        return Err(AppError::InvalidPath(
            "the selected prompt path is empty or malformed".into(),
        ));
    }
    let normalized = raw_path.replace('/', "\\");
    if normalized.starts_with(r"\\.\") || normalized.starts_with(r"\\?\") {
        return Err(AppError::InvalidPath(
            "Windows device paths are not accepted".into(),
        ));
    }
    let path = Path::new(raw_path);
    if !path.is_absolute() || path.components().any(|part| part == Component::ParentDir) {
        return Err(AppError::InvalidPath(
            "the selected prompt path must be absolute and cannot traverse parent directories"
                .into(),
        ));
    }
    let link_metadata = std::fs::symlink_metadata(path)?;
    reject_link(&link_metadata)?;
    let canonical_path = canonicalize_for_storage(path)?;
    let metadata = std::fs::metadata(&canonical_path)?;
    if !metadata.is_file() {
        return Err(AppError::InvalidPath(
            "the selected prompt path is not a regular file".into(),
        ));
    }
    let extension = canonical_path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if !extension.eq_ignore_ascii_case("md") && !extension.eq_ignore_ascii_case("txt") {
        return Err(AppError::InvalidPath(
            "only UTF-8 .md and .txt prompt files can be imported".into(),
        ));
    }
    if metadata.len() > MAX_PROMPT_BYTES as u64 {
        return Err(AppError::InvalidPrompt(format!(
            "the prompt file exceeds the {MAX_PROMPT_BYTES}-byte limit"
        )));
    }
    let fallback_name = canonical_path
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("Untitled Prompt")
        .to_string();
    Ok(GrantedPromptFile {
        canonical_path,
        fallback_name,
    })
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

fn reject_link(metadata: &Metadata) -> AppResult<()> {
    if metadata.file_type().is_symlink() {
        return Err(AppError::InvalidPath(
            "symbolic links are not accepted for prompt imports".into(),
        ));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(AppError::InvalidPath(
                "Windows reparse points are not accepted for prompt imports".into(),
            ));
        }
    }
    Ok(())
}
