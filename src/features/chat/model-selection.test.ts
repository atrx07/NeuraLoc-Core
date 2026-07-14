import { describe, expect, it } from "vitest";
import type { EngineRuntimeStatus, ModelRecord } from "../../types/domain";
import { chatModelLabel, groupChatModels, isSelectedModelReady } from "./model-selection";

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

it("groups runnable models and formats selector details", () => {
  const groups = groupChatModels([model("missing", "missing"), model("ready", "ready")]);
  expect(groups.ready.map((entry) => entry.id)).toEqual(["ready"]);
  expect(groups.unavailable.map((entry) => entry.id)).toEqual(["missing"]);
  expect(chatModelLabel(groups.ready[0])).toContain("Q4_K_M");
  expect(chatModelLabel(groups.ready[0])).toContain("2.3 GB");
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
