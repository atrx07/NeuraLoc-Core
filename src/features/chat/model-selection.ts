import type { EngineRuntimeStatus, ModelFitEstimate, ModelRecord } from "../../types/domain";
import { formatBytes } from "../../utils/format";

const activeLifecycles = new Set([
  "starting",
  "loadingModel",
  "ready",
  "busy",
  "stopping",
  "recovering",
]);

export function groupChatModels(models: ModelRecord[], estimates: ModelFitEstimate[]): {
  verified: ModelRecord[];
  recommended: ModelRecord[];
  tight: ModelRecord[];
  notRecommended: ModelRecord[];
  unavailable: ModelRecord[];
} {
  const sorted = [...models].sort((left, right) => left.displayName.localeCompare(right.displayName));
  const estimatesById = new Map(estimates.map((estimate) => [estimate.modelId, estimate]));
  const verified = sorted.filter((model) => model.verificationState === "ready");
  return {
    verified,
    recommended: verified.filter((model) => {
      const fit = estimatesById.get(model.id)?.fit;
      return fit === "excellent" || fit === "good";
    }),
    tight: verified.filter((model) => {
      const fit = estimatesById.get(model.id)?.fit;
      return fit === "tight" || fit === undefined;
    }),
    notRecommended: verified.filter(
      (model) => estimatesById.get(model.id)?.fit === "not_recommended",
    ),
    unavailable: sorted.filter((model) => model.verificationState !== "ready"),
  };
}

export function chatModelLabel(model: ModelRecord, estimate?: ModelFitEstimate): string {
  const details = [
    model.ggufMetadata?.quantization,
    formatBytes(model.sizeBytes),
    estimate && `${fitLabel(estimate.fit)} / ${formatBytes(estimate.estimatedRamBytes)} RAM`,
  ].filter(Boolean);
  return details.length > 0
    ? `${model.displayName} - ${details.join(" / ")}`
    : model.displayName;
}

export function fitLabel(fit: ModelFitEstimate["fit"]): string {
  return {
    excellent: "Excellent",
    good: "Good",
    tight: "Tight",
    not_recommended: "Not recommended",
  }[fit];
}

export function isModelFitBlocked(estimate: ModelFitEstimate | null | undefined): boolean {
  return estimate?.fit === "not_recommended";
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
