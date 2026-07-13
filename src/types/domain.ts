export type NavigationId =
  | "chat"
  | "images"
  | "speech"
  | "tts"
  | "models"
  | "prompts"
  | "gallery"
  | "hardware"
  | "downloads"
  | "settings"
  | "logs";

export type CapabilityStatus = "available" | "unavailable" | "unknown" | "experimental";

export interface Capability {
  id: string;
  label: string;
  status: CapabilityStatus;
  evidence: string;
}

export interface DeviceInfo {
  id: string;
  kind: "cpu" | "gpu" | "igpu" | "npu";
  name: string;
  vendor: string;
  memoryTotalBytes: number | null;
  memoryAvailableBytes: number | null;
  utilizationPercent: number | null;
  temperatureCelsius: number | null;
}

export interface HardwareSnapshot {
  capturedAt: string;
  source: "native" | "demo";
  cpu: {
    name: string;
    physicalCores: number | null;
    logicalCores: number;
    utilizationPercent: number | null;
  };
  memory: { totalBytes: number; availableBytes: number };
  devices: DeviceInfo[];
  capabilities: Capability[];
  warnings: string[];
}

export type Theme = "dark" | "light" | "system";
export type PerformanceProfile = "maximum" | "balanced" | "low_power" | "quiet" | "manual";

export interface AppSettings {
  theme: Theme;
  performanceProfile: PerformanceProfile;
  keepModelsLoaded: boolean;
  idleUnloadMinutes: number;
  internetAccess: boolean;
  webSearch: boolean;
  apiEnabled: boolean;
  apiPort: number;
  lanAccess: boolean;
}

export interface AppSnapshot {
  version: string;
  databaseReady: boolean;
  firstRunComplete: boolean;
  runningEngines: number;
  activeJobs: number;
}

export interface IpcError {
  code: string;
  message: string;
  suggestion?: string;
}
