use std::{path::Path, sync::Arc};

use crate::{
    conversations::ConversationService, engine_packages::EnginePackageService,
    engines::EngineRuntimeService, errors::AppResult, events::EventEmitter,
    hardware::HardwareService, models::ModelService, processes::ProcessManager,
    prompts::PromptService, settings::SettingsService, storage::Database,
};

pub struct AppState {
    pub database: Arc<Database>,
    pub conversations: Arc<ConversationService>,
    pub events: Arc<EventEmitter>,
    pub engine_packages: Arc<EnginePackageService>,
    pub engines: Arc<EngineRuntimeService>,
    pub hardware: HardwareService,
    pub models: Arc<ModelService>,
    pub processes: Arc<ProcessManager>,
    pub prompts: Arc<PromptService>,
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
        let engine_packages = Arc::new(EnginePackageService::new(
            Arc::clone(&database),
            data_directory,
        )?);
        let models = Arc::new(ModelService::new(Arc::clone(&database)));
        let conversations = Arc::new(ConversationService::new(Arc::clone(&database))?);
        let processes = Arc::new(ProcessManager::default());
        let prompts = Arc::new(PromptService::new(Arc::clone(&database)));
        let engines = Arc::new(EngineRuntimeService::new(
            Arc::clone(&engine_packages),
            Arc::clone(&models),
            Arc::clone(&processes),
        )?);
        let hardware = HardwareService::new(Arc::clone(&processes));
        let settings = SettingsService::load(Arc::clone(&database))?;
        Ok(Self {
            database,
            conversations,
            events,
            engine_packages,
            engines,
            hardware,
            models,
            processes,
            prompts,
            settings,
        })
    }
}
