//! Tauri command handlers (IPC surface). Payloads are snake_case to match the
//! TypeScript types in `src/lib/ipc.ts`.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt;

use crate::api::UsageSnapshot;
use crate::auth;
use crate::config::{self, Settings};
use crate::state::AppState;
use crate::{tray, windows};

#[derive(Serialize)]
pub struct SessionStatus {
    pub logged_in: bool,
    pub org_name: String,
    pub email: String,
}

/// Per-service login status + identity, for the settings account list and the stats/style
/// windows (which show only logged-in services).
#[derive(Serialize)]
pub struct ServiceStatus {
    pub id: String,
    pub name: String,
    pub logged_in: bool,
    pub org_name: String,
    pub email: String,
    /// Plan / subscription (e.g. "Claude Max 20x", "Gemini Pro"), from the last usage snapshot.
    pub subscription: String,
    /// The last snapshot's status ("ok" | "not_running" | "unauthorized" | ... ), or empty
    /// before the first poll completes. `logged_in` is meaningless for a login-less service
    /// like Antigravity (always true); this is what the Settings account row shows instead.
    pub live_status: String,
}

#[tauri::command]
pub fn get_usage(state: State<'_, AppState>, service: Option<String>) -> Option<UsageSnapshot> {
    let service = crate::service::normalize(service.as_deref());
    state.last_snapshot.lock().unwrap().get(&service).cloned()
}

#[tauri::command]
pub async fn refresh_now(
    app: AppHandle,
    state: State<'_, AppState>,
    service: Option<String>,
) -> Result<UsageSnapshot, String> {
    let service = crate::service::normalize(service.as_deref());
    let snapshot = crate::service::fetch(&app, &service, &state.client)
        .await
        .map_err(|e| e.to_string())?;
    crate::usage::apply_snapshot(&app, snapshot.clone());
    Ok(snapshot)
}

/// Usage history within the retention window, optionally narrowed to "24h"/"7d"/"30d".
#[tauri::command]
pub fn get_history(
    app: AppHandle,
    range: String,
    service: Option<String>,
) -> Vec<crate::history::HistoryPoint> {
    use time::{Duration, OffsetDateTime};
    let service = crate::service::normalize(service.as_deref());
    let mut points = crate::history::load(&app, &service);
    let cutoff = match range.as_str() {
        "24h" => Some(OffsetDateTime::now_utc() - Duration::hours(24)),
        "7d" => Some(OffsetDateTime::now_utc() - Duration::days(7)),
        "30d" => Some(OffsetDateTime::now_utc() - Duration::days(30)),
        _ => None,
    };
    if let Some(c) = cutoff {
        points.retain(|p| {
            crate::history::parse_iso(&p.at)
                .map(|t| t >= c)
                .unwrap_or(false)
        });
    }
    points
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

/// Persist settings and apply side effects: widget always-on-top, tray repaint, and
/// theme/settings change events. (Autostart is toggled via its own command; the poll
/// interval is picked up automatically by the poller.)
#[tauri::command]
pub fn set_settings(app: AppHandle, state: State<'_, AppState>, mut settings: Settings) -> Result<(), String> {
    let prev = state.settings.lock().unwrap().clone();

    // Preserve stored per-service widget config for any service the incoming payload omits. Only
    // the Widget Style window owns widget config; the Settings window sends the whole Settings
    // object and can carry a not-yet-loaded (empty) or stale `widgets` map (its `s` starts from
    // defaults and is filled by an async `getSettings()`). A blind replace would then wipe the
    // user's widget styles - observed as the widget style resetting after an update, when the
    // freshly re-created Settings window saved before its `widgets` had loaded. Merge instead:
    // services present in the payload win; services absent keep their stored config. This is the
    // only place a full `widgets` map is written from the frontend, so it is the single guard.
    for (svc, wc) in &prev.widgets {
        settings
            .widgets
            .entry(svc.clone())
            .or_insert_with(|| wc.clone());
    }

    // Docking is edited only through `set_dock_config` and the two dock toggles below; never let
    // a full-settings save from another window (which may hold an older copy) roll it back.
    settings.dock = prev.dock.clone();

    // Re-assert each service widget's layering (always-on-top + taskbar presence) from its
    // (possibly changed) config.
    for (svc, wc) in &settings.widgets {
        if let Some(win) = app.get_webview_window(&crate::windows::widget_label(svc)) {
            windows::apply_layering(&win, wc.always_on_top);
        }
    }

    *state.settings.lock().unwrap() = settings.clone();
    config::save(&app, &settings).map_err(|e| e.to_string())?;

    tray::update_tray(&app);
    if settings.theme != prev.theme {
        let _ = app.emit("theme://changed", &settings.theme);
    }
    let _ = app.emit("settings://changed", &settings);
    Ok(())
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    let manager = app.autolaunch();
    if enabled {
        manager.enable().map_err(|e| e.to_string())
    } else {
        manager.disable().map_err(|e| e.to_string())
    }
}

#[tauri::command]
pub fn get_autostart(app: AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

// --- auto-update (GitHub-based) ---

/// Check the update endpoint; returns whether a newer signed release is available.
#[tauri::command]
pub async fn check_for_update(app: AppHandle) -> Result<crate::update::UpdateInfo, String> {
    crate::update::check(&app).await
}

/// Download + install the pending update and relaunch.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    crate::update::install(&app).await
}

/// Latest available update info (set by the startup check), for the widget + tray menu.
#[tauri::command]
pub fn get_update_state(state: State<'_, AppState>) -> Option<crate::update::UpdateInfo> {
    state.update_available.lock().unwrap().clone()
}

/// Open the "what's new" (changelog) window.
#[tauri::command]
pub fn open_news_window(app: AppHandle) {
    windows::open_news(&app);
}

/// Fetch the changelog from the GitHub repository (raw markdown) for the given locale. The
/// news window falls back to a bundled copy when this fails (offline / repo not published).
#[tauri::command]
pub async fn get_changelog(state: State<'_, AppState>, locale: String) -> Result<String, String> {
    let file = if locale == "ko" {
        "CHANGELOG.md"
    } else {
        "CHANGELOG.en.md"
    };
    let url = format!("https://raw.githubusercontent.com/soma0sd/Session-Meter/main/{file}");
    let resp = state
        .client
        .get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status().as_u16()));
    }
    resp.text().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_session_status(
    app: AppHandle,
    state: State<'_, AppState>,
    service: Option<String>,
) -> SessionStatus {
    let service = crate::service::normalize(service.as_deref());
    let logged_in = crate::service::has_session(&app, &service);
    // Only Claude's identity is persisted in settings today; other services carry their own.
    let (org_name, email) = if service == crate::service::CLAUDE {
        let s = state.settings.lock().unwrap();
        (s.org_name.clone(), s.account_email.clone())
    } else {
        (String::new(), String::new())
    };
    SessionStatus {
        logged_in,
        org_name,
        email,
    }
}

/// Login status + identity for every known service (logged-in or not).
#[tauri::command]
pub fn get_services_status(app: AppHandle, state: State<'_, AppState>) -> Vec<ServiceStatus> {
    // Snapshot each service's identity + plan + live status, then drop the lock before
    // touching settings.
    let snap_info: std::collections::HashMap<String, (String, String, String, String)> = {
        let snaps = state.last_snapshot.lock().unwrap();
        snaps
            .iter()
            .map(|(k, s)| {
                (
                    k.clone(),
                    (
                        s.organization_name.clone(),
                        s.account_email.clone(),
                        s.subscription.clone(),
                        s.status.clone(),
                    ),
                )
            })
            .collect()
    };
    crate::service::all()
        .iter()
        .map(|&id| {
            let logged_in = crate::service::has_session(&app, id);
            let (snap_org, snap_email, subscription, live_status) =
                snap_info.get(id).cloned().unwrap_or_default();
            // Claude's identity is persisted in settings (captured on login). Other services
            // (Gemini) carry theirs on the snapshot: the email is best-effort from the sign-in
            // scrape, organization_name is the service label. The plan/subscription comes from the
            // snapshot for both, so the settings account row can show account + subscription.
            let (org_name, email) = if id == crate::service::CLAUDE {
                let s = state.settings.lock().unwrap();
                (s.org_name.clone(), s.account_email.clone())
            } else {
                (snap_org, snap_email)
            };
            ServiceStatus {
                id: id.to_string(),
                name: crate::service::display_name(id).to_string(),
                logged_in,
                org_name,
                email,
                subscription,
                live_status,
            }
        })
        .collect()
}

/// Open the login window for a service. Gemini and Codex run in an isolated helper process;
/// Claude uses the shared main-process browser window. Antigravity has no login.
#[tauri::command]
pub fn open_login_window(app: AppHandle, service: Option<String>) -> Result<(), String> {
    let service = crate::service::normalize(service.as_deref());
    // Gemini and Codex sign in via a separate helper process so a wedged external page cannot
    // block the main application's shared WebView2 UI thread.
    if service == crate::service::GEMINI {
        crate::gemini::start_login(&app);
        return Ok(());
    }
    if service == crate::service::CODEX {
        crate::codex::start_login(&app);
        return Ok(());
    }
    // One browser-login capture may run at a time. Keep the visible page and its watcher intact
    // rather than re-navigating it to another provider mid-capture.
    if let Some(win) = app.get_webview_window("login") {
        if matches!(win.is_visible(), Ok(true)) {
            let _ = win.set_focus();
            return Ok(());
        }
    }
    // Claude reuses a hidden-on-close webview only after its previous watcher has been cancelled
    // by the close event.
    let app2 = app.clone();
    app.run_on_main_thread(move || windows::create_login_window(&app2, &service))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn capture_session(
    app: AppHandle,
    state: State<'_, AppState>,
    service: Option<String>,
) -> Result<SessionStatus, String> {
    let service = crate::service::normalize(service.as_deref());
    if service == crate::service::CODEX {
        return Err("Codex sign-in is captured by the isolated login helper".to_string());
    }
    let cookie = auth::capture_cookie(&app, &service)
        .await
        .map_err(|e| e.to_string())?;
    let snapshot = crate::service::fetch_with_cookie(&service, &state.client, &cookie)
        .await
        .map_err(|e| e.to_string())?;
    if let Err(error) = config::save_cookie(&app, &service, &cookie) {
        // Keep this public command fail-closed like the browser watcher: the snapshot must not
        // become a signed-in UI state until the encrypted cookie has been persisted.
        crate::usage::mark_status(&app, &service, "error");
        return Err(error.to_string());
    }
    eprintln!(
        "[cg] captured session: org='{}' buckets={}",
        snapshot.organization_name,
        snapshot.buckets.len()
    );

    let org_name = snapshot.organization_name.clone();
    let email = snapshot.account_email.clone();
    if service == crate::service::CLAUDE {
        let mut settings = state.settings.lock().unwrap();
        settings.org_name = org_name.clone();
        settings.account_email = email.clone();
        let _ = config::save(&app, &settings);
    }
    crate::usage::apply_snapshot(&app, snapshot);
    let _ = app.emit(
        "session://changed",
        serde_json::json!({ "service": &service, "logged_in": true, "org_name": &org_name, "email": &email }),
    );
    let app_for_windows = app.clone();
    let _ = app.run_on_main_thread(move || {
        windows::reconcile_widget_visibility(&app_for_windows);
    });

    if let Some(win) = app.get_webview_window("login") {
        let _ = win.close();
    }
    Ok(SessionStatus {
        logged_in: true,
        org_name,
        email,
    })
}

#[tauri::command]
pub fn clear_session(
    app: AppHandle,
    _state: State<'_, AppState>,
    service: Option<String>,
) -> Result<(), String> {
    let service = crate::service::normalize(service.as_deref());
    if service == crate::service::CODEX {
        return crate::codex::clear_session(&app).map_err(|error| error.to_string());
    }
    config::clear_cookie(&app, &service).map_err(|e| e.to_string())?;
    crate::usage::mark_status(&app, &service, "not_logged_in");
    // Drops the signed-out service's own widget (or its cell in the docked window). This
    // command is synchronous, so it already runs on the main thread the window ops need.
    windows::reconcile_widget_visibility(&app);
    let _ = app.emit(
        "session://changed",
        serde_json::json!({ "service": service, "logged_in": false, "org_name": "", "email": "" }),
    );
    Ok(())
}

// --- window control ---

#[tauri::command]
pub fn open_settings_window(app: AppHandle) {
    windows::open_settings(&app);
}

#[tauri::command]
pub fn open_stats_window(app: AppHandle) {
    windows::open_stats(&app);
}

#[tauri::command]
pub fn open_style_window(app: AppHandle) {
    windows::open_style(&app);
}

#[tauri::command]
pub fn toggle_widget(app: AppHandle) {
    windows::toggle_widget(&app);
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    windows::persist_widgets_before_exit(&app);
    app.exit(0);
}

// --- widget control ---

#[tauri::command]
pub fn set_always_on_top(
    app: AppHandle,
    state: State<'_, AppState>,
    service: Option<String>,
    on: bool,
) -> Result<(), String> {
    let service = crate::service::normalize(service.as_deref());
    if let Some(win) = app.get_webview_window(&crate::windows::widget_label(&service)) {
        windows::apply_layering(&win, on);
    }
    let updated = {
        let mut settings = state.settings.lock().unwrap();
        let mut wc = settings.widget(&service);
        wc.always_on_top = on;
        settings.widgets.insert(service.clone(), wc);
        config::save(&app, &settings).map_err(|e| e.to_string())?;
        settings.clone()
    };
    // Keep an open Settings/Style window in sync when toggled from the widget.
    let _ = app.emit("settings://changed", &updated);
    Ok(())
}

#[tauri::command]
pub fn set_move_lock(
    app: AppHandle,
    state: State<'_, AppState>,
    service: Option<String>,
    locked: bool,
) -> Result<(), String> {
    let service = crate::service::normalize(service.as_deref());
    let updated = {
        let mut settings = state.settings.lock().unwrap();
        let mut wc = settings.widget(&service);
        wc.move_lock = locked;
        settings.widgets.insert(service.clone(), wc);
        config::save(&app, &settings).map_err(|e| e.to_string())?;
        settings.clone()
    };
    let _ = app.emit("settings://changed", &updated);
    Ok(())
}

/// Switch which Antigravity model-group bucket pair the widget headline shows, from the widget
/// itself. Mirrors `set_move_lock`: persist, then broadcast so an open Widget Style window
/// follows along. Presentation only - the snapshot still carries all four buckets, and history
/// and the tray stay pinned to the Gemini group (see `antigravity::parse_snapshot`).
#[tauri::command]
pub fn set_widget_headline_group(
    app: AppHandle,
    state: State<'_, AppState>,
    service: Option<String>,
    group: String,
) -> Result<(), String> {
    // Rejected rather than normalized: any other value would build a bucket key (`{group}-5h`)
    // that matches nothing, silently blanking the widget headline back to the Gemini fallback.
    if group != "gemini" && group != "3p" {
        return Err(format!("unknown headline group: {group}"));
    }
    let service = crate::service::normalize(service.as_deref());
    let updated = {
        let mut settings = state.settings.lock().unwrap();
        let mut wc = settings.widget(&service);
        wc.headline_group = group;
        settings.widgets.insert(service.clone(), wc);
        config::save(&app, &settings).map_err(|e| e.to_string())?;
        settings.clone()
    };
    let _ = app.emit("settings://changed", &updated);
    Ok(())
}

#[tauri::command]
pub fn set_widget_opacity(
    app: AppHandle,
    state: State<'_, AppState>,
    service: Option<String>,
    alpha: f64,
) -> Result<(), String> {
    let service = crate::service::normalize(service.as_deref());
    let clamped = alpha.clamp(0.2, 1.0);
    let updated = {
        let mut settings = state.settings.lock().unwrap();
        let mut wc = settings.widget(&service);
        wc.opacity = clamped;
        settings.widgets.insert(service.clone(), wc);
        config::save(&app, &settings).map_err(|e| e.to_string())?;
        settings.clone()
    };
    let _ = app.emit("settings://changed", &updated);
    Ok(())
}

/// Show or hide a single service's widget from the Widget Style window. Persists the choice and
/// applies it to the live window immediately (the watchdog keeps it in sync afterward).
#[tauri::command]
pub fn set_widget_visible(
    app: AppHandle,
    service: Option<String>,
    visible: bool,
) -> Result<(), String> {
    let service = crate::service::normalize(service.as_deref());
    windows::apply_widget_visible(&app, &service, visible);
    Ok(())
}

// --- widget docking ---

/// Everything the Widget Style window's "Placement" tab can change. Deliberately excludes
/// `anchor_x`/`anchor_y`: the docked window's position is only ever written by the window
/// itself being dragged (`window.json`), so a stale anchor cached in an open settings window
/// can never be round-tripped back over a more recent drag through this command.
#[derive(Deserialize)]
pub struct DockConfigPatch {
    pub enabled: bool,
    pub columns: u32,
    pub order: Vec<String>,
    pub opacity: f64,
    pub always_on_top: bool,
    pub move_lock: bool,
}

#[tauri::command]
pub fn set_dock_config(
    app: AppHandle,
    state: State<'_, AppState>,
    patch: DockConfigPatch,
) -> Result<(), String> {
    let updated = {
        let mut settings = state.settings.lock().unwrap();
        settings.dock.enabled = patch.enabled;
        settings.dock.columns = patch.columns.clamp(1, 12);
        settings.dock.order = patch.order;
        settings.dock.opacity = patch.opacity.clamp(0.2, 1.0);
        settings.dock.always_on_top = patch.always_on_top;
        settings.dock.move_lock = patch.move_lock;
        config::save(&app, &settings).map_err(|e| e.to_string())?;
        settings.clone()
    };
    if let Some(win) = app.get_webview_window(windows::DOCK_LABEL) {
        windows::apply_layering(&win, updated.dock.always_on_top);
    }
    let _ = app.emit("settings://changed", &updated);
    // Turning docking on/off, or changing who is in it, moves services between their own
    // windows and the docked one. Synchronous command, so this is already the main thread.
    windows::reconcile_widget_visibility(&app);
    Ok(())
}

/// Always-on-top for the docked window, toggled from the widget itself. Mirrors
/// `set_always_on_top` for a single service's window.
#[tauri::command]
pub fn set_dock_always_on_top(app: AppHandle, state: State<'_, AppState>, on: bool) -> Result<(), String> {
    if let Some(win) = app.get_webview_window(windows::DOCK_LABEL) {
        windows::apply_layering(&win, on);
    }
    let updated = {
        let mut settings = state.settings.lock().unwrap();
        settings.dock.always_on_top = on;
        config::save(&app, &settings).map_err(|e| e.to_string())?;
        settings.clone()
    };
    let _ = app.emit("settings://changed", &updated);
    Ok(())
}

/// Move lock for the docked window, toggled from the widget itself.
#[tauri::command]
pub fn set_dock_move_lock(app: AppHandle, state: State<'_, AppState>, locked: bool) -> Result<(), String> {
    let updated = {
        let mut settings = state.settings.lock().unwrap();
        settings.dock.move_lock = locked;
        config::save(&app, &settings).map_err(|e| e.to_string())?;
        settings.clone()
    };
    let _ = app.emit("settings://changed", &updated);
    Ok(())
}

/// The services the docked window shows right now, in placement order. Pulled once when the
/// docked window mounts; it follows `dock://members` afterwards.
#[tauri::command]
pub fn get_dock_members(app: AppHandle) -> Vec<String> {
    crate::dock::current_members(&app)
}

// --- theme / locale ---

#[tauri::command]
pub fn set_theme(app: AppHandle, state: State<'_, AppState>, theme: String) {
    {
        let mut settings = state.settings.lock().unwrap();
        settings.theme = theme.clone();
        let _ = config::save(&app, &settings);
    }
    let _ = app.emit("theme://changed", &theme);
    tray::update_tray(&app);
}

/// Effective UI locale: the explicit choice, or the OS locale mapped to ko/en when auto.
#[tauri::command]
pub fn get_effective_locale(state: State<'_, AppState>) -> String {
    let lang = state.settings.lock().unwrap().language.clone();
    match lang.as_str() {
        "ko" => "ko".to_string(),
        "en" => "en".to_string(),
        _ => {
            let loc = tauri_plugin_os::locale().unwrap_or_default();
            if loc.to_lowercase().starts_with("ko") {
                "ko".to_string()
            } else {
                "en".to_string()
            }
        }
    }
}
