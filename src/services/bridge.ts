import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { confirm, open } from "@tauri-apps/plugin-dialog";
import type {
  AppSettings,
  AppSnapshot,
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
