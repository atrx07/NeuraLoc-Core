use std::{path::Path, sync::Arc};

use crate::{
    errors::AppResult, hardware::HardwareService, processes::ProcessManager,
    settings::SettingsService, storage::Database,
};

pub struct AppState {
    pub database: Arc<Database>,
    pub hardware: HardwareService,
    pub processes: Arc<ProcessManager>,
    pub settings: SettingsService,
}

impl AppState {
    pub fn new(data_directory: &Path) -> AppResult<Self> {
        std::fs::create_dir_all(data_directory)?;
        for child in ["models", "outputs", "prompts", "downloads", "cache", "logs"] {
            std::fs::create_dir_all(data_directory.join(child))?;
        }
        let database = Arc::new(Database::open(&data_directory.join("neuraloc-core.db"))?);
        let processes = Arc::new(ProcessManager::default());
        let hardware = HardwareService::new(Arc::clone(&processes));
        let settings = SettingsService::load(Arc::clone(&database))?;
        Ok(Self {
            database,
            hardware,
            processes,
            settings,
        })
    }
}
