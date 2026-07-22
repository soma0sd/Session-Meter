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

export type UsageStatus = "ok" | "unauthorized" | "not_running" | "error";

export interface UsageSnapshot {
  service_id: string;
  five_hour: WindowUsage | null;
  weekly_primary: WindowUsage | null;
  primary_key: string | null;
  secondary_key: string | null;
  buckets: Bucket[];
  organization_name: string;
  account_email: string;
  subscription: string;
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
  session_threshold: number;
  weekly_threshold: number;
  on_reset: boolean;
}

export interface WidgetConfig {
  style: string;
  display_mode: "remaining" | "used";
  opacity: number;
  always_on_top: boolean;
  move_lock: boolean;
  visible: boolean;
  /** Antigravity-only: which model-group bucket pair ("gemini" | "3p") the widget/tray
   *  headline shows. Ignored by every other service. */
  headline_group: "gemini" | "3p";
}

/** Widget grid docking: several widgets snapped into a grid that move together. See the
 *  Widget Style window's Placement tab. */
export interface DockConfig {
  enabled: boolean;
  columns: number;
  /** Service ids in row-major placement order. */
  order: string[];
  anchor_x: number;
  anchor_y: number;
}

/** Everything `setDockConfig` can change - deliberately excludes the anchor, which only
 *  `dockMoveTo` (a live group drag) may write. */
export interface DockConfigPatch {
  enabled: boolean;
  columns: number;
  order: string[];
}

export interface Settings {
  theme: "light" | "dark" | "system";
  language: "auto" | "ko" | "en";
  refresh_interval_min: number;
  /** Per-service widget config, keyed by service id. */
  widgets: Record<string, WidgetConfig>;
  notify: NotifySettings;
  history_retention_days: number;
  org_name: string;
  account_email: string;
  dock: DockConfig;
}

/** WidgetConfig with defaults applied for a service missing from the map. */
export function widgetConfig(s: Settings, service: string): WidgetConfig {
  return (
    s.widgets?.[service] ?? {
      style: "focus-slim-detailed",
      display_mode: "remaining",
      opacity: 0.9,
      always_on_top: true,
      move_lock: false,
      visible: true,
      headline_group: "gemini",
    }
  );
}

export interface SessionStatus {
  logged_in: boolean;
  org_name: string;
  email: string;
}

export interface ServiceStatus {
  id: string;
  name: string;
  logged_in: boolean;
  org_name: string;
  email: string;
  /** Plan / subscription (e.g. "Claude Max 20x", "Gemini Pro"). */
  subscription: string;
  /** Last snapshot's status ("ok" | "not_running" | "unauthorized" | ""), for services (like
   *  Antigravity) where `logged_in` alone doesn't say whether it's currently reachable. */
  live_status: string;
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
function mockUsage(service?: string): UsageSnapshot {
  const b = (key: string, label: string, remaining: number, resetMs: number): Bucket => ({
    key,
    label,
    remaining,
    utilization: 100 - remaining,
    resets_at: iso(resetMs),
  });
  if (service === "gemini") {
    const buckets = [
      b("gemini-3.1-pro-preview", "Gemini 3.1 Pro Preview", 100, 6 * 3600_000),
      b("gemini-3-flash-preview", "Gemini 3 Flash Preview", 74, 6 * 3600_000),
      b("gemini-3.1-flash-lite", "Gemini 3.1 Flash Lite", 92, 6 * 3600_000),
    ];
    return {
      service_id: "gemini",
      five_hour: { remaining: 100, utilization: 0, resets_at: buckets[0].resets_at },
      weekly_primary: { remaining: 74, utilization: 26, resets_at: buckets[1].resets_at },
      primary_key: "gemini-3.1-pro-preview",
      secondary_key: "gemini-3-flash-preview",
      buckets,
      organization_name: "you@example.com",
      account_email: "you@example.com",
      subscription: "Gemini Code Assist in Google One AI Pro",
      fetched_at: iso(0),
      status: "ok",
    };
  }
  if (service === "antigravity_ide") {
    const buckets = [
      b("gemini-5h", "Gemini 5-hour", 85, 3 * 3600_000),
      b("gemini-weekly", "Gemini weekly", 98, 5 * 86_400_000),
      b("3p-5h", "Claude/GPT 5-hour", 100, 3 * 3600_000),
      b("3p-weekly", "Claude/GPT weekly", 67, 5 * 86_400_000),
    ];
    return {
      service_id: "antigravity_ide",
      five_hour: { remaining: 85, utilization: 15, resets_at: buckets[0].resets_at },
      weekly_primary: { remaining: 98, utilization: 2, resets_at: buckets[1].resets_at },
      primary_key: "gemini-5h",
      secondary_key: "gemini-weekly",
      buckets,
      organization_name: "",
      account_email: "",
      subscription: "",
      fetched_at: iso(0),
      status: "ok",
    };
  }
  const buckets = [
    b("five_hour", "5-hour session", 62, 2 * 3600_000),
    b("seven_day", "Weekly (7 days)", 88, 4 * 86_400_000),
    b("seven_day_opus", "Weekly (Opus)", 41, 4 * 86_400_000),
  ];
  return {
    service_id: "claude",
    five_hour: { remaining: 62, utilization: 38, resets_at: buckets[0].resets_at },
    weekly_primary: { remaining: 88, utilization: 12, resets_at: buckets[1].resets_at },
    primary_key: "five_hour",
    secondary_key: "seven_day",
    buckets,
    organization_name: "Preview Org",
    account_email: "you@example.com",
    subscription: "Claude Max 20x",
    fetched_at: iso(0),
    status: "ok",
  };
}
function mockSettings(): Settings {
  return {
    theme: "system",
    language: "auto",
    refresh_interval_min: 5,
    widgets: {
      claude: {
        style: "focus-slim-detailed",
        display_mode: "remaining",
        opacity: 0.9,
        always_on_top: true,
        move_lock: false,
        visible: true,
        headline_group: "gemini",
      },
    },
    notify: { enabled: true, session_threshold: 80, weekly_threshold: 80, on_reset: true },
    history_retention_days: 30,
    org_name: "Preview Org",
    account_email: "you@example.com",
    dock: { enabled: false, columns: 2, order: [], anchor_x: 0, anchor_y: 0 },
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

// --- usage (per service; omit the argument for the default Claude service) ---
export const getUsage = (service?: string) =>
  call<UsageSnapshot | null>("get_usage", { service }, () => mockUsage(service));
export const refreshNow = (service?: string) =>
  call<UsageSnapshot>("refresh_now", { service }, () => mockUsage(service));
export const getHistory = (range: string, service?: string) =>
  call<HistoryPoint[]>("get_history", { range, service }, mockHistory);

// --- settings ---
export const getSettings = () => call<Settings>("get_settings", undefined, mockSettings);
export const setSettings = (settings: Settings) =>
  call<void>("set_settings", { settings }, () => undefined);
export const getEffectiveLocale = () =>
  call<"ko" | "en">("get_effective_locale", undefined, () => "en");

// --- session (per service; omit the argument for the default Claude service) ---
export const getSessionStatus = (service?: string) =>
  call<SessionStatus>("get_session_status", { service }, () => ({
    logged_in: true,
    org_name: "Preview Org",
    email: "you@example.com",
  }));
export const getServicesStatus = () =>
  call<ServiceStatus[]>("get_services_status", undefined, () => [
    { id: "claude", name: "Claude", logged_in: true, org_name: "Preview Org", email: "you@example.com", subscription: "Claude Max 20x", live_status: "ok" },
    { id: "gemini", name: "Gemini", logged_in: true, org_name: "Gemini", email: "you@gmail.com", subscription: "Gemini Pro", live_status: "ok" },
    { id: "antigravity_ide", name: "Antigravity", logged_in: true, org_name: "", email: "", subscription: "", live_status: "ok" },
  ]);
export const openLoginWindow = (service?: string) =>
  call<void>("open_login_window", { service }, () => undefined);
export const captureSession = (service?: string) =>
  call<SessionStatus>("capture_session", { service }, () => ({
    logged_in: true,
    org_name: "Preview Org",
    email: "you@example.com",
  }));
export const clearSession = (service?: string) =>
  call<void>("clear_session", { service }, () => undefined);

// --- windows / widget ---
export const openSettingsWindow = () =>
  call<void>("open_settings_window", undefined, () => undefined);
export const openStatsWindow = () => call<void>("open_stats_window", undefined, () => undefined);
export const openStyleWindow = () => call<void>("open_style_window", undefined, () => undefined);
export const toggleWidget = () => call<void>("toggle_widget", undefined, () => undefined);
export const setAlwaysOnTop = (service: string, on: boolean) =>
  call<void>("set_always_on_top", { service, on }, () => undefined);
export const setMoveLock = (service: string, locked: boolean) =>
  call<void>("set_move_lock", { service, locked }, () => undefined);
export const setWidgetOpacity = (service: string, alpha: number) =>
  call<void>("set_widget_opacity", { service, alpha }, () => undefined);
export const setWidgetVisible = (service: string, visible: boolean) =>
  call<void>("set_widget_visible", { service, visible }, () => undefined);

// --- widget grid docking ---
export const setDockConfig = (patch: DockConfigPatch) =>
  call<void>("set_dock_config", { patch }, () => undefined);
export const dockMoveTo = (service: string, x: number, y: number) =>
  call<void>("dock_move_to", { service, x, y }, () => undefined);
export const dockMoveEnd = () => call<void>("dock_move_end", undefined, () => undefined);
export const dockRelayout = () => call<void>("dock_relayout", undefined, () => undefined);

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
