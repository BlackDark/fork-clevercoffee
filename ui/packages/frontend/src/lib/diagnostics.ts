/** Formatting and timing for the Diagnostics page. */

/** Delay before reopening the metrics stream after the device refused it. */
export const RECONNECT_MS = 2_000;

/** `90061000` ms as `1d 01:01:01`. */
export function formatUptime(ms: number): string {
  const total = Math.floor(ms / 1000);
  const days = Math.floor(total / 86400);
  const clock = [
    Math.floor((total % 86400) / 3600),
    Math.floor((total % 3600) / 60),
    total % 60,
  ]
    .map((n) => String(n).padStart(2, "0"))
    .join(":");
  return days > 0 ? `${days}d ${clock}` : clock;
}

/** Bytes as whole KiB. */
export function formatKiB(bytes: number): string {
  return `${Math.round(bytes / 1024)} KiB`;
}

/** The device reports 0 until its first stack scan. */
export function formatStack(bytes: number): string {
  return bytes === 0 ? "not measured" : formatKiB(bytes);
}

export function formatMicros(us: number): string {
  return us >= 1000 ? `${(us / 1000).toFixed(1)} ms` : `${us} µs`;
}

const SAVE_RESULTS = ["none yet", "saved", "failed"] as const;

export function saveResultName(code: number): string {
  return SAVE_RESULTS[code] ?? "unknown";
}
