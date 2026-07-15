import { describe, expect, it } from "vitest";
import type { EngineRuntimeStatus, ModelFitEstimate, ModelRecord } from "../../types/domain";
import {
  chatModelLabel,
  fitLabel,
  groupChatModels,
  isModelFitBlocked,
  isSelectedModelReady,
} from "./model-selection";

function model(id: string, state: ModelRecord["verificationState"]): ModelRecord {
  return {
    id,
    kind: "llm",
    displayName: id === "ready" ? "Qwen3 4B" : "Missing model",
    family: "qwen3",
    format: "gguf",
    path: `C:\\models\\${id}.gguf`,
    sizeBytes: 2.3 * 1024 ** 3,
    sha256: null,
    verificationState: state,
    verificationError: null,
    ggufMetadata: {
      version: 3,
      tensorCount: 1,
      metadataCount: 1,
      architecture: "qwen3",
      name: "Qwen3 4B",
      fileType: 15,
      quantization: "Q4_K_M",
      parameterCount: null,
      contextLength: 40_960,
      embeddingLength: null,
      layerCount: 36,
      hasChatTemplate: true,
      metadataBytes: 100,
      metadataPreview: {},
    },
    modifiedAtUnixMs: 1,
    importedAt: "2026-07-14T00:00:00Z",
    lastVerifiedAt: "2026-07-14T00:00:00Z",
  };
}

function estimate(modelId: string, fit: ModelFitEstimate["fit"]): ModelFitEstimate {
  return {
    modelId,
    route: "cpu",
    fit,
    confidence: "medium",
    contextSize: 4096,
    estimatedRamBytes: 4.2 * 1024 ** 3,
    availableRamBytes: 12 * 1024 ** 3,
    reservedRamBytes: 3.2 * 1024 ** 3,
    weightBytes: 2.3 * 1024 ** 3,
    kvCacheBytes: 1.4 * 1024 ** 3,
    runtimeOverheadBytes: 0.5 * 1024 ** 3,
    headroomBytes: 4.6 * 1024 ** 3,
    reason: "Fits the CPU route.",
  };
}

it("groups runnable models and formats selector details", () => {
  const groups = groupChatModels(
    [model("missing", "missing"), model("ready", "ready")],
    [estimate("ready", "good")],
  );
  expect(groups.recommended.map((entry) => entry.id)).toEqual(["ready"]);
  expect(groups.unavailable.map((entry) => entry.id)).toEqual(["missing"]);
  expect(chatModelLabel(groups.recommended[0], estimate("ready", "good"))).toContain("Q4_K_M");
  expect(chatModelLabel(groups.recommended[0], estimate("ready", "good"))).toContain("4.2 GB RAM");
});

it("separates tight and over-budget models", () => {
  const tight = model("tight", "ready");
  const oversized = model("oversized", "ready");
  const groups = groupChatModels(
    [oversized, tight],
    [estimate("tight", "tight"), estimate("oversized", "not_recommended")],
  );

  expect(groups.tight.map((entry) => entry.id)).toEqual(["tight"]);
  expect(groups.notRecommended.map((entry) => entry.id)).toEqual(["oversized"]);
  expect(fitLabel("not_recommended")).toBe("Not recommended");
  expect(isModelFitBlocked(estimate("oversized", "not_recommended"))).toBe(true);
  expect(isModelFitBlocked(estimate("tight", "tight"))).toBe(false);
});

describe("selected model readiness", () => {
  const status: EngineRuntimeStatus = {
    engineId: "llama.cpp",
    packageId: "cpu",
    lifecycle: "ready",
    sessionId: "session",
    processId: "process",
    pid: 42,
    modelId: "ready",
    modelName: "Qwen3 4B",
    backendVersion: "b9986",
    contextSize: 4096,
    startedAt: "2026-07-14T00:00:00Z",
    endedAt: null,
    exitCode: null,
    detail: "Ready",
  };

  it("requires both ready lifecycle and matching identity", () => {
    expect(isSelectedModelReady("ready", status)).toBe(true);
    expect(isSelectedModelReady("other", status)).toBe(false);
    expect(isSelectedModelReady("ready", { ...status, lifecycle: "busy" })).toBe(false);
  });
});
