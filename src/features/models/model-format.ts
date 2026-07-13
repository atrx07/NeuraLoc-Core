import type { ModelRecord } from "../../types/domain";

export function formatParameterCount(value: number | null | undefined): string | null {
  if (!value) return null;
  if (value >= 1_000_000_000) return `${trimDecimal(value / 1_000_000_000)}B params`;
  if (value >= 1_000_000) return `${trimDecimal(value / 1_000_000)}M params`;
  return `${value.toLocaleString()} params`;
}

export function formatContextLength(value: number | null | undefined): string | null {
  if (!value) return null;
  if (value >= 1024) return `${trimDecimal(value / 1024)}K context`;
  return `${value.toLocaleString()} context`;
}

export function modelMetadataLabels(model: ModelRecord): string[] {
  const metadata = model.ggufMetadata;
  return [
    model.family?.toUpperCase() ?? null,
    metadata?.quantization ?? null,
    formatParameterCount(metadata?.parameterCount),
    formatContextLength(metadata?.contextLength),
    metadata?.layerCount ? `${metadata.layerCount} layers` : null,
  ].filter((value): value is string => Boolean(value));
}

function trimDecimal(value: number): string {
  return value.toFixed(value >= 10 ? 0 : 1).replace(/\.0$/, "");
}
