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

export type ModelVerificationState = "metadata_pending" | "ready" | "invalid" | "missing";

export interface GgufMetadata {
  version: number;
  tensorCount: number;
  metadataCount: number;
  architecture: string | null;
  name: string | null;
  fileType: number | null;
  quantization: string | null;
  parameterCount: number | null;
  contextLength: number | null;
  embeddingLength: number | null;
  layerCount: number | null;
  hasChatTemplate: boolean;
  metadataBytes: number;
  metadataPreview: Record<string, unknown>;
}

export interface ModelRecord {
  id: string;
  kind: string;
  displayName: string;
  family: string | null;
  format: string;
  path: string;
  sizeBytes: number;
  sha256: string | null;
  verificationState: ModelVerificationState;
  verificationError: string | null;
  ggufMetadata: GgufMetadata | null;
  modifiedAtUnixMs: number;
  importedAt: string;
  lastVerifiedAt: string | null;
}

export interface ImportModelOutcome {
  model: ModelRecord;
  alreadyIndexed: boolean;
}

export type ModelScanPhase = "discovering" | "importing" | "complete";

export interface ModelScanProgress {
  scanId: string;
  phase: ModelScanPhase;
  currentPath: string | null;
  discovered: number;
  processed: number;
  imported: number;
  duplicates: number;
  invalid: number;
}

export interface ModelScanIssue {
  path: string;
  message: string;
}

export interface ModelScanSummary {
  scanId: string;
  discovered: number;
  processed: number;
  imported: number;
  duplicates: number;
  invalid: number;
  cancelled: boolean;
  issues: ModelScanIssue[];
}

export interface EventEnvelope<T> {
  eventVersion: number;
  sequence: number;
  emittedAt: string;
  payload: T;
}
