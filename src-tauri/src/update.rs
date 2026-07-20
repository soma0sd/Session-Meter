//! GitHub-based auto-update (Tauri v2 updater).
//!
//! The app is signed with a keypair: the public key is embedded via
//! `plugins.updater.pubkey` in tauri.conf.json, and the private key signs release artifacts
//! in CI (`TAURI_SIGNING_PRIVATE_KEY`). On startup we check the GitHub `releases/latest`
//! manifest (`latest.json`) and emit `update://available` when a newer signed release
//! exists; the Settings window also offers a manual check + install.

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;

use crate::state::AppState;

#[derive(Serialize, Clone)]
pub struct UpdateInfo {
    pub available: bool,
    pub version: String,
    pub notes: String,
}

/// Query the update endpoint. Returns availability + the new version/notes when one exists.
pub async fn check(app: &AppHandle) -> Result<UpdateInfo, String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    match updater.check().await.map_err(|e| e.to_string())? {
        Some(update) => Ok(UpdateInfo {
            available: true,
            version: update.version.clone(),
            notes: update.body.clone().unwrap_or_default(),
        }),
        None => Ok(UpdateInfo {
            available: false,
            version: String::new(),
            notes: String::new(),
        }),
    }
}

/// Download + install the pending update, then relaunch. Returns only when no update exists.
pub async fn install(app: &AppHandle) -> Result<(), String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    if let Some(update) = updater.check().await.map_err(|e| e.to_string())? {
        update
            .download_and_install(|_, _| {}, || {})
            .await
            .map_err(|e| e.to_string())?;
        // Flush widget position(s) before the updater relaunches us. The widget is hidden
        // during teardown, so without this the last on-screen position is lost and the
        // widget returns to its default corner after the update+restart.
        crate::windows::persist_widgets_before_exit(app);
        app.restart();
    }
    Ok(())
}

/// Background startup check: store the result in `AppState` and emit `update://available`
/// (with the info) when a newer release exists, so the widget and tray menu can surface an
/// install button. Silent on any error (offline, endpoint missing, no release yet, etc.).
pub async fn check_and_emit(app: &AppHandle) {
    if let Ok(info) = check(app).await {
        if info.available {
            if let Some(state) = app.try_state::<AppState>() {
                *state.update_available.lock().unwrap() = Some(info.clone());
            }
            let _ = app.emit("update://available", info);
        }
    }
}

/// Check for updates on startup and then every 10 minutes, so a release published while the app
/// is running is surfaced (widget + tray install button) without needing a restart. Idempotent:
/// `check_and_emit` only emits when an update is available, so repeated calls are harmless.
pub fn start_periodic(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            check_and_emit(&app).await;
            tokio::time::sleep(std::time::Duration::from_secs(600)).await;
        }
    });
}
