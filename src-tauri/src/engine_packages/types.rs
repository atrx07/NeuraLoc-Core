use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnginePackageManifest {
    pub manifest_version: u16,
    pub id: String,
    pub engine_id: String,
    pub version: String,
    pub platform: String,
    pub architecture: String,
    pub route: String,
    pub source_url: String,
    pub archive_file_name: String,
    pub archive_size_bytes: u64,
    pub archive_sha256: String,
    pub expected_files: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnginePackageState {
    Installing,
    Ready,
    Invalid,
    Missing,
}

impl EnginePackageState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Installing => "installing",
            Self::Ready => "ready",
            Self::Invalid => "invalid",
            Self::Missing => "missing",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "installing" => Some(Self::Installing),
            "ready" => Some(Self::Ready),
            "invalid" => Some(Self::Invalid),
            "missing" => Some(Self::Missing),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledPackageFile {
    pub path: String,
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnginePackageRecord {
    pub id: String,
    pub engine_id: String,
    pub version: String,
    pub platform: String,
    pub architecture: String,
    pub route: String,
    pub install_path: String,
    pub archive_sha256: String,
    pub files: Vec<InstalledPackageFile>,
    pub state: EnginePackageState,
    pub source_url: Option<String>,
    pub error: Option<String>,
    pub installed_at: Option<String>,
    pub verified_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnginePackageStatus {
    pub manifest: EnginePackageManifest,
    pub installation: Option<EnginePackageRecord>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnginePackageIdRequest {
    pub package_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportEnginePackageRequest {
    pub package_id: String,
    pub path: String,
}
