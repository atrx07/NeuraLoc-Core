import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { confirm, open } from "@tauri-apps/plugin-dialog";
import type {
  AppSettings,
  AppSnapshot,
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
    startedAt: null,
    endedAt: null,
    exitCode: null,
    detail: "Install and verify the llama.cpp CPU runtime to load a model.",
  };
}
