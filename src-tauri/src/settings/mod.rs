use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::{
    errors::{AppError, AppResult},
    storage::Database,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    Dark,
    Light,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PerformanceProfile {
    Maximum,
    Balanced,
    LowPower,
    Quiet,
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub theme: Theme,
    pub performance_profile: PerformanceProfile,
    pub keep_models_loaded: bool,
    pub idle_unload_minutes: u16,
    pub internet_access: bool,
    pub web_search: bool,
    pub api_enabled: bool,
    pub api_port: u16,
    pub lan_access: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: Theme::Dark,
            performance_profile: PerformanceProfile::Balanced,
            keep_models_loaded: false,
            idle_unload_minutes: 15,
            internet_access: false,
            web_search: false,
            api_enabled: false,
            api_port: 11434,
            lan_access: false,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SettingsPatch {
    pub theme: Option<Theme>,
    pub performance_profile: Option<PerformanceProfile>,
    pub keep_models_loaded: Option<bool>,
    pub idle_unload_minutes: Option<u16>,
    pub internet_access: Option<bool>,
    pub web_search: Option<bool>,
    pub api_enabled: Option<bool>,
    pub api_port: Option<u16>,
    pub lan_access: Option<bool>,
}

pub struct SettingsService {
    database: Arc<Database>,
    current: RwLock<AppSettings>,
}

impl SettingsService {
    pub fn load(database: Arc<Database>) -> AppResult<Self> {
        let current = match database.get_setting("app")? {
            Some(value) => serde_json::from_str(&value).map_err(|error| {
                AppError::InvalidSetting(format!("stored settings are corrupt: {error}"))
            })?,
            None => AppSettings::default(),
        };
        Ok(Self {
            database,
            current: RwLock::new(current),
        })
    }

    pub fn get(&self) -> AppSettings {
        self.current.read().clone()
    }

    pub fn update(&self, patch: SettingsPatch) -> AppResult<AppSettings> {
        let mut next = self.get();
        if let Some(value) = patch.theme {
            next.theme = value;
        }
        if let Some(value) = patch.performance_profile {
            next.performance_profile = value;
        }
        if let Some(value) = patch.keep_models_loaded {
            next.keep_models_loaded = value;
        }
        if let Some(value) = patch.idle_unload_minutes {
            if !(1..=240).contains(&value) {
                return Err(AppError::InvalidSetting(
                    "idle unload must be between 1 and 240 minutes".into(),
                ));
            }
            next.idle_unload_minutes = value;
        }
        if let Some(value) = patch.internet_access {
            next.internet_access = value;
        }
        if let Some(value) = patch.web_search {
            next.web_search = value;
        }
        if let Some(value) = patch.api_enabled {
            next.api_enabled = value;
        }
        if let Some(value) = patch.api_port {
            if value < 1024 {
                return Err(AppError::InvalidSetting(
                    "API port must be 1024 or higher".into(),
                ));
            }
            next.api_port = value;
        }
        if let Some(value) = patch.lan_access {
            next.lan_access = value;
        }
        if !next.internet_access {
            next.web_search = false;
        }
        if !next.api_enabled {
            next.lan_access = false;
        }

        let json = serde_json::to_string(&next).map_err(|error| {
            AppError::InvalidSetting(format!("settings could not be serialized: {error}"))
        })?;
        self.database.put_setting("app", &json)?;
        *self.current.write() = next.clone();
        Ok(next)
    }
}
