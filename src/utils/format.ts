export function formatBytes(value: number | null): string {
  if (value === null) return "Not reported";
  return `${(value / 1024 ** 3).toFixed(1)} GB`;
}
