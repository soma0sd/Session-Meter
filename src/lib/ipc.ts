// Typed wrappers over Tauri IPC. All payloads use snake_case to match Rust serde defaults.
// Outside Tauri (a plain browser preview) the getters return mock data so the UIs render
// for visual development; this branch never runs in the packaged app.
import { invoke } from "@tauri-apps/api/core";

export interface WindowUsage {
  remaining: number;
  utilization: number;
  resets_at: string; // ISO-8601
}

export interface Bucket {
  key: string;
  label: string;
  remaining: number;
  utilization: number;
  resets_at: string;
}

export type UsageStatus = "ok" | "unauthorized" | "error";

export interface UsageSnapshot {
  five_hour: WindowUsage | null;
  weekly_primary: WindowUsage | null;
  buckets: Bucket[];
  organization_name: string;
  account_email: string;
  fetched_at: string;
  status: UsageStatus;
}

export interface HistoryPoint {
  at: string; // ISO-8601
  five_hour: number | null;
  weekly: number | null;
}

export interface NotifySettings {
  enabled: boolean;
  thresholds: number[];
  on_reset: boolean;
}

export interface Settings {
  theme: "light" | "dark" | "system";
  language: "auto" | "ko" | "en";
  widget_opacity: number;
  refresh_interval_min: number;
  always_on_top: boolean;
  move_lock: boolean;
  tray_display: "remaining" | "used";
  tray_bucket: "five_hour" | "weekly";
  notify: NotifySettings;
  history_retention_days: number;
  org_name: string;
  account_email: string;
}

export interface SessionStatus {
  logged_in: boolean;
  org_name: string;
  email: string;
}

export interface UpdateInfo {
  available: boolean;
  version: string;
  notes: string;
}

const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

function call<T>(cmd: string, args: Record<string, unknown> | undefined, mock: () => T): Promise<T> {
  if (!inTauri) return Promise.resolve(mock());
  return invoke<T>(cmd, args);
}

// --- mock data (browser preview only) ---
function iso(offsetMs: number): string {
  return new Date(Date.now() + offsetMs).toISOString();
}
function mockUsage(): UsageSnapshot {
  const b = (key: string, label: string, remaining: number, resetMs: number): Bucket => ({
    key,
    label,
    remaining,
    utilization: 100 - remaining,
    resets_at: iso(resetMs),
  });
  const buckets = [
    b("five_hour", "5-hour session", 62, 2 * 3600_000),
    b("seven_day", "Weekly (7 days)", 88, 4 * 86_400_000),
    b("seven_day_opus", "Weekly (Opus)", 41, 4 * 86_400_000),
  ];
  return {
    five_hour: { remaining: 62, utilization: 38, resets_at: buckets[0].resets_at },
    weekly_primary: { remaining: 88, utilization: 12, resets_at: buckets[1].resets_at },
    buckets,
    organization_name: "Preview Org",
    account_email: "you@example.com",
    fetched_at: iso(0),
    status: "ok",
  };
}
function mockSettings(): Settings {
  return {
    theme: "system",
    language: "auto",
    widget_opacity: 0.9,
    refresh_interval_min: 5,
    always_on_top: true,
    move_lock: false,
    tray_display: "remaining",
    tray_bucket: "five_hour",
    notify: { enabled: true, thresholds: [80, 95], on_reset: true },
    history_retention_days: 30,
    org_name: "Preview Org",
    account_email: "you@example.com",
  };
}
function mockHistory(): HistoryPoint[] {
  const pts: HistoryPoint[] = [];
  for (let i = 48; i >= 0; i--) {
    const t = -i * 30 * 60_000;
    pts.push({
      at: iso(t),
      five_hour: Math.max(0, 100 - ((48 - i) % 20) * 5),
      weekly: Math.max(0, 100 - (48 - i) * 0.8),
    });
  }
  return pts;
}

// --- usage ---
export const getUsage = () => call<UsageSnapshot | null>("get_usage", undefined, mockUsage);
export const refreshNow = () => call<UsageSnapshot>("refresh_now", undefined, mockUsage);
export const getHistory = (range: string) =>
  call<HistoryPoint[]>("get_history", { range }, mockHistory);

// --- settings ---
export const getSettings = () => call<Settings>("get_settings", undefined, mockSettings);
export const setSettings = (settings: Settings) =>
  call<void>("set_settings", { settings }, () => undefined);
export const getEffectiveLocale = () =>
  call<"ko" | "en">("get_effective_locale", undefined, () => "en");

// --- session ---
export const getSessionStatus = () =>
  call<SessionStatus>("get_session_status", undefined, () => ({
    logged_in: true,
    org_name: "Preview Org",
    email: "you@example.com",
  }));
export const openLoginWindow = () => call<void>("open_login_window", undefined, () => undefined);
export const captureSession = () =>
  call<SessionStatus>("capture_session", undefined, () => ({
    logged_in: true,
    org_name: "Preview Org",
    email: "you@example.com",
  }));
export const clearSession = () => call<void>("clear_session", undefined, () => undefined);

// --- windows / widget ---
export const openSettingsWindow = () =>
  call<void>("open_settings_window", undefined, () => undefined);
export const openStatsWindow = () => call<void>("open_stats_window", undefined, () => undefined);
export const toggleWidget = () => call<void>("toggle_widget", undefined, () => undefined);
export const setAlwaysOnTop = (on: boolean) =>
  call<void>("set_always_on_top", { on }, () => undefined);
export const setMoveLock = (locked: boolean) =>
  call<void>("set_move_lock", { locked }, () => undefined);
export const setWidgetOpacity = (alpha: number) =>
  call<void>("set_widget_opacity", { alpha }, () => undefined);

// --- system ---
export const setAutostart = (enabled: boolean) =>
  call<void>("set_autostart", { enabled }, () => undefined);
export const getAutostart = () => call<boolean>("get_autostart", undefined, () => false);
export const setTheme = (theme: string) => call<void>("set_theme", { theme }, () => undefined);
export const quitApp = () => call<void>("quit_app", undefined, () => undefined);

// --- auto-update ---
export const checkForUpdate = () =>
  call<UpdateInfo>("check_for_update", undefined, () => ({
    available: false,
    version: "",
    notes: "",
  }));
export const installUpdate = () => call<void>("install_update", undefined, () => undefined);
export const getUpdateState = () =>
  call<UpdateInfo | null>("get_update_state", undefined, () => null);
export const openNewsWindow = () => call<void>("open_news_window", undefined, () => undefined);
export const getChangelog = (locale: string) =>
  call<string>("get_changelog", { locale }, () => "");
