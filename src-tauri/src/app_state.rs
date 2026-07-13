use std::{path::Path, sync::Arc};

use crate::{
    errors::AppResult, events::EventEmitter, hardware::HardwareService, models::ModelService,
    processes::ProcessManager, settings::SettingsService, storage::Database,
};

pub struct AppState {
    pub database: Arc<Database>,
    pub events: Arc<EventEmitter>,
    pub hardware: HardwareService,
    pub models: Arc<ModelService>,
    pub processes: Arc<ProcessManager>,
    pub settings: SettingsService,
}

impl AppState {
    pub fn new(data_directory: &Path) -> AppResult<Self> {
        std::fs::create_dir_all(data_directory)?;
        for child in [
            "models/llm",
            "models/image",
            "models/speech",
            "models/tts",
            "outputs/images",
            "outputs/transcripts",
            "outputs/speech",
            "prompts",
            "downloads",
            "cache",
            "logs",
        ] {
            std::fs::create_dir_all(data_directory.join(child))?;
        }
        let database = Arc::new(Database::open(&data_directory.join("neuraloc-core.db"))?);
        let events = Arc::new(EventEmitter::default());
        let models = Arc::new(ModelService::new(Arc::clone(&database)));
        let processes = Arc::new(ProcessManager::default());
        let hardware = HardwareService::new(Arc::clone(&processes));
        let settings = SettingsService::load(Arc::clone(&database))?;
        Ok(Self {
            database,
            events,
            hardware,
            models,
            processes,
            settings,
        })
    }
}
