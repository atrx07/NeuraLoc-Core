import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { confirm, open } from "@tauri-apps/plugin-dialog";
import type {
  AppSettings,
  AppSnapshot,
  ChatGenerationResult,
  ChatStateEvent,
  ChatTokenBatch,
  ChatUsageEvent,
  EnginePackageRecord,
  EnginePackageStatus,
  EngineHealth,
  EngineLogBatch,
  EngineLogSnapshot,
  EngineRuntimeStatus,
  EventEnvelope,
  HardwareSnapshot,
  ImportModelOutcome,
  ModelRecord,
  ModelScanProgress,
  ModelScanSummary,
  CompiledPrompt,
  ConversationDetail,
  ConversationExport,
  ConversationSummary,
  PromptExport,
  PromptExportMode,
  PromptMutationOutcome,
  PromptSummary,
  PromptVersionRecord,
  StartChatGenerationRequest,
} from "../types/domain";

const defaultSettings: AppSettings = {
  theme: "dark",
  performanceProfile: "balanced",
  keepModelsLoaded: false,
  idleUnloadMinutes: 15,
  internetAccess: false,
  webSearch: false,
  apiEnabled: false,
  apiPort: 11434,
  lanAccess: false,
};

let demoSettings = { ...defaultSettings };

function isTauri(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

function demoHardware(): HardwareSnapshot {
  return {
    capturedAt: new Date().toISOString(),
    source: "demo",
    cpu: {
      name: "Intel Core Ultra 9 (browser preview)",
      physicalCores: null,
      logicalCores: navigator.hardwareConcurrency || 1,
      utilizationPercent: 18,
    },
    memory: { totalBytes: 32 * 1024 ** 3, availableBytes: 21.4 * 1024 ** 3 },
    devices: [
      {
        id: "demo-rtx",
        kind: "gpu",
        name: "NVIDIA GeForce RTX 5070",
        vendor: "NVIDIA",
        memoryTotalBytes: 8 * 1024 ** 3,
        memoryAvailableBytes: 6.7 * 1024 ** 3,
        utilizationPercent: 7,
        temperatureCelsius: 46,
      },
      {
        id: "demo-npu",
        kind: "npu",
        name: "Intel AI Boost",
        vendor: "Intel",
        memoryTotalBytes: null,
        memoryAvailableBytes: null,
        utilizationPercent: 0,
        temperatureCelsius: null,
      },
    ],
    capabilities: [
      { id: "llm-cuda", label: "LLM / CUDA", status: "available", evidence: "Demo runtime probe" },
      { id: "image-cuda", label: "Images / CUDA", status: "available", evidence: "Demo runtime probe" },
      { id: "llm-vulkan", label: "LLM / Vulkan", status: "unknown", evidence: "Loader probe pending" },
      { id: "openvino-npu", label: "OpenVINO / NPU", status: "experimental", evidence: "Model compile required" },
      { id: "cpu", label: "CPU fallback", status: "available", evidence: "Native CPU engine supported" },
    ],
    warnings: ["Browser preview uses representative hardware data. Launch the Tauri app for native detection."],
  };
}

export const bridge = {
  async getAppSnapshot(): Promise<AppSnapshot> {
    if (isTauri()) return invoke<AppSnapshot>("get_app_snapshot");
    return { version: "0.1.0", databaseReady: true, firstRunComplete: false, runningEngines: 0, activeJobs: 0 };
  },

  async getHardwareSnapshot(refresh = false): Promise<HardwareSnapshot> {
    if (isTauri()) {
      return invoke<HardwareSnapshot>(refresh ? "refresh_hardware" : "get_hardware_snapshot");
    }
    return demoHardware();
  },

  async getSettings(): Promise<AppSettings> {
    if (isTauri()) return invoke<AppSettings>("get_settings");
    return { ...demoSettings };
  },

  async updateSettings(patch: Partial<AppSettings>): Promise<AppSettings> {
    if (isTauri()) return invoke<AppSettings>("update_settings", { patch });
    demoSettings = { ...demoSettings, ...patch };
    return { ...demoSettings };
  },

  async chooseModelFile(): Promise<string | null> {
    if (!isTauri()) return null;
    return open({
      title: "Import a GGUF model",
      multiple: false,
      filters: [{ name: "GGUF model", extensions: ["gguf"] }],
    });
  },

  async chooseModelFolder(): Promise<string | null> {
    if (!isTauri()) return null;
    return open({
      title: "Scan a folder for GGUF models",
      directory: true,
      recursive: true,
      multiple: false,
    });
  },

  async chooseEnginePackageArchive(): Promise<string | null> {
    if (!isTauri()) return null;
    return open({
      title: "Import a verified llama.cpp package",
      multiple: false,
      filters: [{ name: "ZIP archive", extensions: ["zip"] }],
    });
  },

  async choosePromptFile(): Promise<string | null> {
    if (!isTauri()) return null;
    return open({
      title: "Import a system prompt",
      multiple: false,
      filters: [{ name: "Prompt document", extensions: ["md", "txt"] }],
    });
  },

  async listEnginePackages(): Promise<EnginePackageStatus[]> {
    if (!isTauri()) return [demoEnginePackageStatus()];
    return invoke<EnginePackageStatus[]>("list_engine_packages");
  },

  async installEnginePackage(packageId: string): Promise<EnginePackageRecord> {
    if (!isTauri()) throw new Error("Engine packages are available in the desktop app.");
    return invoke<EnginePackageRecord>("install_engine_package", { request: { packageId } });
  },

  async importEnginePackage(packageId: string, path: string): Promise<EnginePackageRecord> {
    if (!isTauri()) throw new Error("Engine packages are available in the desktop app.");
    return invoke<EnginePackageRecord>("import_engine_package", { request: { packageId, path } });
  },

  async verifyEnginePackage(packageId: string): Promise<EnginePackageRecord> {
    if (!isTauri()) throw new Error("Engine packages are available in the desktop app.");
    return invoke<EnginePackageRecord>("verify_engine_package", { request: { packageId } });
  },

  async uninstallEnginePackage(packageId: string): Promise<void> {
    if (!isTauri()) throw new Error("Engine packages are available in the desktop app.");
    return invoke<void>("uninstall_engine_package", { request: { packageId } });
  },

  async getEngineStatus(): Promise<EngineRuntimeStatus> {
    if (!isTauri()) return demoEngineStatus();
    return invoke<EngineRuntimeStatus>("get_engine_status");
  },

  async getEngineHealth(): Promise<EngineHealth> {
    if (!isTauri()) throw new Error("Engine health is available in the desktop app.");
    return invoke<EngineHealth>("get_engine_health");
  },

  async startEngine(modelId: string): Promise<EngineRuntimeStatus> {
    if (!isTauri()) throw new Error("Local model loading is available in the desktop app.");
    return invoke<EngineRuntimeStatus>("start_engine", {
      request: { modelId, contextSize: null, threads: null },
    });
  },

  async stopEngine(sessionId: string): Promise<EngineRuntimeStatus> {
    if (!isTauri()) throw new Error("Local model loading is available in the desktop app.");
    return invoke<EngineRuntimeStatus>("stop_engine", { request: { sessionId } });
  },

  async getEngineLogs(sessionId: string): Promise<EngineLogSnapshot> {
    if (!isTauri()) return { sessionId, processId: "demo", lines: [] };
    return invoke<EngineLogSnapshot>("get_engine_logs", { request: { sessionId } });
  },

  async startChatGeneration(
    request: StartChatGenerationRequest,
  ): Promise<ChatGenerationResult> {
    if (!isTauri()) throw new Error("Local chat generation is available in the desktop app.");
    return invoke<ChatGenerationResult>("start_chat_generation", { request });
  },

  async cancelChatGeneration(jobId: string): Promise<boolean> {
    if (!isTauri()) return false;
    return invoke<boolean>("cancel_chat_generation", { request: { jobId } });
  },

  async onChatToken(
    callback: (batch: ChatTokenBatch, sequence: number) => void,
  ): Promise<UnlistenFn> {
    if (!isTauri()) return () => undefined;
    return listen<EventEnvelope<ChatTokenBatch>>("chat://token", (event) => {
      callback(event.payload.payload, event.payload.sequence);
    });
  },

  async onChatStateChanged(
    callback: (state: ChatStateEvent, sequence: number) => void,
  ): Promise<UnlistenFn> {
    if (!isTauri()) return () => undefined;
    return listen<EventEnvelope<ChatStateEvent>>("chat://state-changed", (event) => {
      callback(event.payload.payload, event.payload.sequence);
    });
  },

  async onChatUsage(
    callback: (usage: ChatUsageEvent, sequence: number) => void,
  ): Promise<UnlistenFn> {
    if (!isTauri()) return () => undefined;
    return listen<EventEnvelope<ChatUsageEvent>>("chat://usage", (event) => {
      callback(event.payload.payload, event.payload.sequence);
    });
  },

  async onEngineStateChanged(
    callback: (status: EngineRuntimeStatus) => void,
  ): Promise<UnlistenFn> {
    if (!isTauri()) return () => undefined;
    return listen<EventEnvelope<EngineRuntimeStatus>>("engine://state-changed", (event) => {
      callback(event.payload.payload);
    });
  },

  async onEngineLogLines(
    callback: (batch: EngineLogBatch) => void,
  ): Promise<UnlistenFn> {
    if (!isTauri()) return () => undefined;
    return listen<EventEnvelope<EngineLogBatch>>("engine://log-line", (event) => {
      callback(event.payload.payload);
    });
  },

  async listConversations(query = "", limit = 50, offset = 0): Promise<ConversationSummary[]> {
    if (!isTauri()) {
      const conversation = demoConversationSummary();
      const needle = query.trim().toLocaleLowerCase();
      const matches = !needle || [conversation.title, conversation.modelName, conversation.promptName]
        .some((value) => value?.toLocaleLowerCase().includes(needle));
      return matches && offset === 0 && limit > 0 ? [conversation] : [];
    }
    return invoke<ConversationSummary[]>("list_conversations", {
      request: { query: query || null, limit, offset },
    });
  },

  async getConversation(conversationId: string): Promise<ConversationDetail> {
    if (!isTauri()) {
      if (conversationId === demoConversationSummary().id) return demoConversationDetail();
      throw new Error("The demo conversation was not found.");
    }
    return invoke<ConversationDetail>("get_conversation", { request: { conversationId } });
  },

  async renameConversation(conversationId: string, title: string): Promise<void> {
    if (!isTauri()) throw new Error("Conversation editing is available in the desktop app.");
    return invoke<void>("rename_conversation", {
      request: { conversationId, title },
    });
  },

  async setConversationPinned(conversationId: string, pinned: boolean): Promise<void> {
    if (!isTauri()) throw new Error("Conversation pinning is available in the desktop app.");
    return invoke<void>("set_conversation_pinned", {
      request: { conversationId, pinned },
    });
  },

  async deleteConversation(conversationId: string): Promise<void> {
    if (!isTauri()) throw new Error("Conversation deletion is available in the desktop app.");
    return invoke<void>("delete_conversation", { request: { conversationId } });
  },

  async exportConversation(conversationId: string): Promise<ConversationExport> {
    if (!isTauri()) {
      if (conversationId !== demoConversationSummary().id) {
        throw new Error("The demo conversation was not found.");
      }
      return {
        fileName: "Review-the-persistence-design.md",
        mediaType: "text/markdown;charset=utf-8",
        content: "# Review the persistence design\n\n## 1. User\n\nReview the conversation persistence design.\n\n## 2. Assistant\n\nThe turn is persisted before inference and finalized after streaming completes.\n",
      };
    }
    return invoke<ConversationExport>("export_conversation", {
      request: { conversationId },
    });
  },

  async confirmDeleteConversation(title: string): Promise<boolean> {
    const message = `Delete ${title} and all of its messages from local history?`;
    if (!isTauri()) return window.confirm(message);
    return confirm(message, {
      title: "Delete conversation",
      kind: "warning",
      okLabel: "Delete",
      cancelLabel: "Keep",
    });
  },

  async confirmUninstallEnginePackage(version: string): Promise<boolean> {
    const message = `Uninstall the llama.cpp ${version} runtime? Local GGUF model files will not be removed.`;
    if (!isTauri()) return window.confirm(message);
    return confirm(message, {
      title: "Uninstall llama.cpp runtime",
      kind: "warning",
      okLabel: "Uninstall",
      cancelLabel: "Keep runtime",
    });
  },

  async listModels(): Promise<ModelRecord[]> {
    if (!isTauri()) return [];
    return invoke<ModelRecord[]>("list_models");
  },

  async importModel(path: string): Promise<ImportModelOutcome> {
    return invoke<ImportModelOutcome>("import_model", { request: { path } });
  },

  async scanModelFolder(scanId: string, path: string): Promise<ModelScanSummary> {
    return invoke<ModelScanSummary>("scan_model_folder", { request: { scanId, path } });
  },

  async cancelModelScan(scanId: string): Promise<boolean> {
    return invoke<boolean>("cancel_model_scan", { scanId });
  },

  async reverifyModel(modelId: string): Promise<ModelRecord> {
    return invoke<ModelRecord>("reverify_model", { request: { modelId } });
  },

  async removeModelRecord(modelId: string): Promise<void> {
    return invoke<void>("remove_model_record", { request: { modelId } });
  },

  async listPrompts(query = ""): Promise<PromptSummary[]> {
    if (!isTauri()) {
      const prompt = demoPromptSummary();
      const needle = query.trim().toLocaleLowerCase();
      return !needle || [prompt.stableName, prompt.collection, ...prompt.tags]
        .some((value) => value?.toLocaleLowerCase().includes(needle)) ? [prompt] : [];
    }
    return invoke<PromptSummary[]>("list_prompts", { request: { query: query || null } });
  },

  async importPrompt(path: string): Promise<PromptMutationOutcome> {
    if (!isTauri()) throw new Error("Prompt import is available in the desktop app.");
    return invoke<PromptMutationOutcome>("import_prompt", { request: { path } });
  },

  async createPrompt(name: string, content: string): Promise<PromptMutationOutcome> {
    if (!isTauri()) throw new Error("Prompt creation is available in the desktop app.");
    return invoke<PromptMutationOutcome>("create_prompt", { request: { name, content } });
  },

  async savePrompt(profileId: string, baseVersionId: string, document: string): Promise<PromptMutationOutcome> {
    if (!isTauri()) throw new Error("Prompt editing is available in the desktop app.");
    return invoke<PromptMutationOutcome>("save_prompt", {
      request: { profileId, baseVersionId, document },
    });
  },

  async getPromptVersion(versionId: string): Promise<PromptVersionRecord> {
    if (!isTauri()) {
      if (versionId === demoPromptSummary().latestVersionId) return demoPromptVersion();
      throw new Error("The demo prompt version was not found.");
    }
    return invoke<PromptVersionRecord>("get_prompt_version", { request: { versionId } });
  },

  async duplicatePrompt(versionId: string, name: string | null = null): Promise<PromptMutationOutcome> {
    if (!isTauri()) throw new Error("Prompt duplication is available in the desktop app.");
    return invoke<PromptMutationOutcome>("duplicate_prompt", { request: { versionId, name } });
  },

  async setPromptPinned(profileId: string, pinned: boolean): Promise<PromptSummary> {
    if (!isTauri()) throw new Error("Prompt pinning is available in the desktop app.");
    return invoke<PromptSummary>("set_prompt_pinned", { request: { profileId, pinned } });
  },

  async deletePrompt(profileId: string): Promise<void> {
    if (!isTauri()) throw new Error("Prompt deletion is available in the desktop app.");
    return invoke<void>("delete_prompt", { request: { profileId } });
  },

  async exportPrompt(versionId: string, mode: PromptExportMode): Promise<PromptExport> {
    if (!isTauri()) {
      const prompt = demoPromptVersion();
      if (versionId !== prompt.id) throw new Error("The demo prompt version was not found.");
      const normalizedMetadata = {
        name: prompt.metadata.name,
        description: prompt.metadata.description,
        tags: prompt.metadata.tags,
        collection: prompt.metadata.collection,
      };
      const normalized = `---\n${JSON.stringify(normalizedMetadata, null, 2)}\n---\n${prompt.content}`;
      return { fileName: "local-code-reviewer.md", content: mode === "original" ? prompt.rawDocument : normalized };
    }
    return invoke<PromptExport>("export_prompt", { request: { versionId, mode } });
  },

  async compilePrompt(versionId: string): Promise<CompiledPrompt> {
    if (!isTauri()) {
      const prompt = demoPromptVersion();
      if (versionId !== prompt.id) throw new Error("The demo prompt version was not found.");
      return { versionId, content: prompt.content, estimatedTokens: Math.ceil(prompt.content.length / 4), approximate: true };
    }
    return invoke<CompiledPrompt>("compile_prompt", { request: { versionId } });
  },

  async confirmDeletePrompt(name: string): Promise<boolean> {
    const message = `Delete ${name} from the prompt library? Existing historical references will remain readable.`;
    if (!isTauri()) return window.confirm(message);
    return confirm(message, {
      title: "Delete prompt",
      kind: "warning",
      okLabel: "Delete",
      cancelLabel: "Keep",
    });
  },

  async confirmPromptConversationChange(name: string): Promise<boolean> {
    const message = `Start a new conversation with ${name}? The current conversation will remain in local history.`;
    if (!isTauri()) return window.confirm(message);
    return confirm(message, {
      title: "Change system prompt",
      kind: "info",
      okLabel: "New conversation",
      cancelLabel: "Keep current",
    });
  },

  async confirmModelConversationChange(name: string): Promise<boolean> {
    const message = `Start a new conversation with ${name}? The current conversation will remain in local history.`;
    if (!isTauri()) return window.confirm(message);
    return confirm(message, {
      title: "Change chat model",
      kind: "info",
      okLabel: "New conversation",
      cancelLabel: "Keep current",
    });
  },

  async confirmRemoveModel(displayName: string): Promise<boolean> {
    const message = `Remove ${displayName} from the library? The GGUF file will stay on disk.`;
    if (!isTauri()) return window.confirm(message);
    return confirm(message, {
      title: "Remove model record",
      kind: "warning",
      okLabel: "Remove record",
      cancelLabel: "Keep",
    });
  },

  async onModelScanProgress(
    scanId: string,
    callback: (progress: ModelScanProgress) => void,
  ): Promise<UnlistenFn> {
    if (!isTauri()) return () => undefined;
    return listen<EventEnvelope<ModelScanProgress>>("model://scan-progress", (event) => {
      if (event.payload.payload.scanId === scanId) callback(event.payload.payload);
    });
  },
};

function demoEnginePackageStatus(): EnginePackageStatus {
  return {
    manifest: {
      manifestVersion: 1,
      id: "llama.cpp-b9986-windows-x86_64-cpu",
      engineId: "llama.cpp",
      version: "b9986",
      platform: "windows",
      architecture: "x86_64",
      route: "cpu",
      sourceUrl: "https://github.com/ggml-org/llama.cpp/releases/download/b9986/llama-b9986-bin-win-cpu-x64.zip",
      archiveFileName: "llama-b9986-bin-win-cpu-x64.zip",
      archiveSizeBytes: 18_245_837,
      archiveSha256: "df7b177d14697af9a1bd9a42e2d89455fc592e7206985ad1c672d19f3faa11d2",
      expectedFiles: ["llama-server.exe"],
    },
    installation: null,
  };
}

function demoEngineStatus(): EngineRuntimeStatus {
  return {
    engineId: "llama.cpp",
    packageId: "llama.cpp-b9986-windows-x86_64-cpu",
    lifecycle: "notInstalled",
    sessionId: null,
    processId: null,
    pid: null,
    modelId: null,
    modelName: null,
    backendVersion: null,
    contextSize: null,
    startedAt: null,
    endedAt: null,
    exitCode: null,
    detail: "Install and verify the llama.cpp CPU runtime to load a model.",
  };
}

function demoPromptSummary(): PromptSummary {
  return {
    profileId: "demo-prompt-profile",
    stableName: "Local code reviewer",
    collection: "Coding",
    pinned: true,
    latestVersionId: "demo-prompt-version-2",
    latestVersion: 2,
    description: "Reviews code for correctness, security, and missing tests.",
    tags: ["code", "review"],
    sourcePath: null,
    createdAt: "2026-07-14T09:00:00Z",
    updatedAt: "2026-07-15T09:00:00Z",
  };
}

function demoPromptVersion(): PromptVersionRecord {
  const document = "---\nname: Local code reviewer\ndescription: Reviews code for correctness, security, and missing tests.\ntags: [code, review]\ncollection: Coding\n---\nReview the user's code precisely. Lead with correctness and security findings, then identify missing tests.";
  return {
    id: "demo-prompt-version-2",
    profileId: "demo-prompt-profile",
    version: 2,
    sourcePath: null,
    sourceHash: "47232cb6e67540cabbb15115f53a243a64a77093db61308450600c78a72260f9",
    metadata: {
      name: "Local code reviewer",
      declaredVersion: null,
      description: "Reviews code for correctness, security, and missing tests.",
      tags: ["code", "review"],
      recommendedModels: [],
      temperature: null,
      topP: null,
      topK: null,
      contextReserve: null,
      collection: "Coding",
      extra: {},
    },
    content: "Review the user's code precisely. Lead with correctness and security findings, then identify missing tests.",
    rawDocument: document,
    sourceProfileId: null,
    sourceVersionId: null,
    createdAt: "2026-07-15T09:00:00Z",
  };
}

function demoConversationSummary(): ConversationSummary {
  return {
    id: "demo-conversation-1",
    title: "Review the persistence design",
    modelId: "demo-model-1",
    modelName: "Qwen3 4B",
    promptVersionId: "demo-prompt-version-2",
    promptName: "Local code reviewer",
    promptVersion: 2,
    contextStrategy: "full_history",
    pinned: true,
    messageCount: 2,
    createdAt: "2026-07-15T09:00:00Z",
    updatedAt: "2026-07-15T09:01:00Z",
  };
}

function demoConversationDetail(): ConversationDetail {
  const summary = demoConversationSummary();
  const { messageCount: _messageCount, ...conversation } = summary;
  return {
    conversation: {
      ...conversation,
      generationSettings: { maxOutputTokens: 1024 },
    },
    messages: [
      {
        id: "demo-user-message",
        conversationId: summary.id,
        parentId: null,
        role: "user",
        content: "Review the conversation persistence design.",
        state: "complete",
        jobId: null,
        tokenCount: null,
        usage: null,
        terminalReason: null,
        position: 1,
        createdAt: summary.createdAt,
        updatedAt: summary.createdAt,
      },
      {
        id: "demo-assistant-message",
        conversationId: summary.id,
        parentId: "demo-user-message",
        role: "assistant",
        content: "The turn is persisted before inference and finalized after streaming completes.",
        state: "complete",
        jobId: "demo-job",
        tokenCount: 14,
        usage: { promptTokens: 32, outputTokens: 14, tokensPerSecond: 18.4 },
        terminalReason: "completed",
        position: 2,
        createdAt: summary.updatedAt,
        updatedAt: summary.updatedAt,
      },
    ],
  };
}
