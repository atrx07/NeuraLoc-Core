import type { EngineRuntimeStatus, ModelRecord } from "../../types/domain";
import { formatBytes } from "../../utils/format";

const activeLifecycles = new Set([
  "starting",
  "loadingModel",
  "ready",
  "busy",
  "stopping",
  "recovering",
]);

export function groupChatModels(models: ModelRecord[]): {
  ready: ModelRecord[];
  unavailable: ModelRecord[];
} {
  const sorted = [...models].sort((left, right) => left.displayName.localeCompare(right.displayName));
  return {
    ready: sorted.filter((model) => model.verificationState === "ready"),
    unavailable: sorted.filter((model) => model.verificationState !== "ready"),
  };
}

export function chatModelLabel(model: ModelRecord): string {
  const details = [
    model.ggufMetadata?.quantization,
    formatBytes(model.sizeBytes),
  ].filter(Boolean);
  return details.length > 0
    ? `${model.displayName} - ${details.join(" / ")}`
    : model.displayName;
}

export function isEngineActive(status: EngineRuntimeStatus | null): boolean {
  return status !== null && activeLifecycles.has(status.lifecycle);
}

export function isSelectedModelReady(
  selectedModelId: string | null,
  status: EngineRuntimeStatus | null,
): boolean {
  return selectedModelId !== null
    && status?.modelId === selectedModelId
    && status.lifecycle === "ready";
}
