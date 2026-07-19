//! Tauri command handlers (IPC surface). Payloads are snake_case to match the
//! TypeScript types in `src/lib/ipc.ts`.

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt;

use crate::api::{self, UsageSnapshot};
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

#[tauri::command]
pub fn get_usage(state: State<'_, AppState>) -> Option<UsageSnapshot> {
    state.last_snapshot.lock().unwrap().clone()
}

#[tauri::command]
pub async fn refresh_now(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<UsageSnapshot, String> {
    let cookie = config::load_cookie(&app).ok_or_else(|| "not signed in".to_string())?;
    let snapshot = api::fetch_usage(&state.client, &cookie)
        .await
        .map_err(|e| e.to_string())?;
    crate::usage::apply_snapshot(&app, snapshot.clone());
    Ok(snapshot)
}

/// Usage history within the retention window, optionally narrowed to "24h" or "7d".
#[tauri::command]
pub fn get_history(
    app: AppHandle,
    range: String,
) -> Vec<crate::history::HistoryPoint> {
    use time::{Duration, OffsetDateTime};
    let mut points = crate::history::load(&app);
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
pub fn set_settings(app: AppHandle, state: State<'_, AppState>, settings: Settings) -> Result<(), String> {
    let prev = state.settings.lock().unwrap().clone();

    if settings.always_on_top != prev.always_on_top {
        if let Some(win) = app.get_webview_window("widget") {
            let _ = win.set_always_on_top(settings.always_on_top);
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
pub fn get_session_status(app: AppHandle, state: State<'_, AppState>) -> SessionStatus {
    let logged_in = config::load_cookie(&app).is_some();
    let (org_name, email) = {
        let s = state.settings.lock().unwrap();
        (s.org_name.clone(), s.account_email.clone())
    };
    SessionStatus {
        logged_in,
        org_name,
        email,
    }
}

/// Open the claude.ai login window (remote URL, IPC-isolated). Window creation is
/// dispatched to the main thread (required on Windows).
#[tauri::command]
pub fn open_login_window(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("login") {
        let _ = win.set_focus();
        return Ok(());
    }
    let app2 = app.clone();
    app.run_on_main_thread(move || windows::create_login_window(&app2))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn capture_session(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<SessionStatus, String> {
    let cookie = auth::capture_cookie(&app).await.map_err(|e| e.to_string())?;
    let snapshot = api::fetch_usage(&state.client, &cookie)
        .await
        .map_err(|e| e.to_string())?;
    config::save_cookie(&app, &cookie).map_err(|e| e.to_string())?;
    eprintln!(
        "[cg] captured session: org='{}' buckets={}",
        snapshot.organization_name,
        snapshot.buckets.len()
    );

    let org_name = snapshot.organization_name.clone();
    let email = snapshot.account_email.clone();
    {
        let mut settings = state.settings.lock().unwrap();
        settings.org_name = org_name.clone();
        settings.account_email = email.clone();
        let _ = config::save(&app, &settings);
    }
    crate::usage::apply_snapshot(&app, snapshot);

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
pub fn clear_session(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    config::clear_cookie(&app).map_err(|e| e.to_string())?;
    *state.last_snapshot.lock().unwrap() = None;
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
pub fn toggle_widget(app: AppHandle) {
    windows::toggle_widget(&app);
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    windows::save_widget_pos(&app);
    app.exit(0);
}

// --- widget control ---

#[tauri::command]
pub fn set_always_on_top(app: AppHandle, state: State<'_, AppState>, on: bool) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("widget") {
        win.set_always_on_top(on).map_err(|e| e.to_string())?;
    }
    let updated = {
        let mut settings = state.settings.lock().unwrap();
        settings.always_on_top = on;
        config::save(&app, &settings).map_err(|e| e.to_string())?;
        settings.clone()
    };
    // Keep an open Settings window's checkbox in sync when toggled from the widget.
    let _ = app.emit("settings://changed", &updated);
    Ok(())
}

#[tauri::command]
pub fn set_move_lock(app: AppHandle, state: State<'_, AppState>, locked: bool) -> Result<(), String> {
    let updated = {
        let mut settings = state.settings.lock().unwrap();
        settings.move_lock = locked;
        config::save(&app, &settings).map_err(|e| e.to_string())?;
        settings.clone()
    };
    let _ = app.emit("settings://changed", &updated);
    Ok(())
}

#[tauri::command]
pub fn set_widget_opacity(app: AppHandle, state: State<'_, AppState>, alpha: f64) -> Result<(), String> {
    let clamped = alpha.clamp(0.2, 1.0);
    let updated = {
        let mut settings = state.settings.lock().unwrap();
        settings.widget_opacity = clamped;
        config::save(&app, &settings).map_err(|e| e.to_string())?;
        settings.clone()
    };
    let _ = app.emit("settings://changed", &updated);
    Ok(())
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
