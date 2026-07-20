// Format the time remaining until an ISO-8601 reset timestamp.
export function msUntil(isoResetsAt: string): number {
  const t = Date.parse(isoResetsAt);
  if (Number.isNaN(t)) return 0;
  return Math.max(0, t - Date.now());
}

export function formatCountdown(ms: number, locale: "ko" | "en" = "en"): string {
  // Guard against a missing/invalid reset time (e.g. a fresh session with no resets_at yet),
  // which would otherwise render as "NaN...".
  if (!Number.isFinite(ms)) return "-";
  const totalSec = Math.floor(Math.max(0, ms) / 1000);
  const d = Math.floor(totalSec / 86400);
  const h = Math.floor((totalSec % 86400) / 3600);
  const m = Math.floor((totalSec % 3600) / 60);
  const s = totalSec % 60;
  if (locale === "ko") {
    if (d > 0) return `${d}일 ${h}시간`;
    if (h > 0) return `${h}시간 ${m}분`;
    return `${m}분 ${s}초`;
  }
  if (d > 0) return `${d}d ${h}h`;
  if (h > 0) return `${h}h ${m}m`;
  return `${m}m ${s}s`;
}

// Clock-style remaining time for the compact widget infographics: "H:MM" when an hour or more
// remains, otherwise "M:SS" (minutes:seconds). Zero-padded and locale-independent. Days roll into
// the hour count (compact widgets only show the short primary window, so the value stays small).
export function formatClock(ms: number): string {
  if (!Number.isFinite(ms)) return "-";
  const totalSec = Math.floor(Math.max(0, ms) / 1000);
  const h = Math.floor(totalSec / 3600);
  const m = Math.floor((totalSec % 3600) / 60);
  const s = totalSec % 60;
  const pad = (n: number) => String(n).padStart(2, "0");
  return h > 0 ? `${h}:${pad(m)}` : `${m}:${pad(s)}`;
}

/**
 * Format a reset timestamp as a friendly date + time. Same-day resets read "Today HH:MM",
 * next-day "Tomorrow HH:MM"; anything further out shows the month + day + time.
 */
export function formatResetDateTime(
  iso: string,
  localeTag: string,
  todayLabel: string,
  tomorrowLabel: string,
): string {
  if (!iso) return "";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  const time = d.toLocaleTimeString(localeTag, { hour: "2-digit", minute: "2-digit" });
  const now = new Date();
  const startOfDay = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const diffDays = Math.round((startOfDay(d) - startOfDay(now)) / 86_400_000);
  if (diffDays === 0) return `${todayLabel} ${time}`;
  if (diffDays === 1) return `${tomorrowLabel} ${time}`;
  return d.toLocaleString(localeTag, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

// A store-friendly ticker: calls `cb` every second. Returns a stop function.
export function everySecond(cb: () => void): () => void {
  cb();
  const id = setInterval(cb, 1000);
  return () => clearInterval(id);
}
