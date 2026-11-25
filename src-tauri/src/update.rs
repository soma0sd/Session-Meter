//! GitHub-based auto-update (Tauri v2 updater).
//!
//! The app is signed with a keypair: the public key is embedded via
//! `plugins.updater.pubkey` in tauri.conf.json, and the private key signs release artifacts
//! in CI (`TAURI_SIGNING_PRIVATE_KEY`). On startup we check the GitHub `releases/latest`
//! manifest (`latest.json`) and emit `update://available` when a newer signed release
//! exists. Users can install it manually or enable automatic installation in Settings.

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::state::AppState;

#[derive(Serialize, Clone)]
pub struct UpdateInfo {
    pub available: bool,
    pub version: String,
    pub notes: String,
}

/// Query the update endpoint. Returns availability + the new version/notes when one exists.
pub async fn check(app: &AppHandle) -> Result<UpdateInfo, String> {
    Ok(match find_update(app).await? {
        Some(update) => update_info(&update),
        None => UpdateInfo {
            available: false,
            version: String::new(),
            notes: String::new(),
        },
    })
}

async fn find_update(app: &AppHandle) -> Result<Option<Update>, String> {
    app.updater_builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|error| error.to_string())?
        .check()
        .await
        .map_err(|error| error.to_string())
}

fn update_info(update: &Update) -> UpdateInfo {
    UpdateInfo {
        available: true,
        version: update.version.clone(),
        notes: update.body.clone().unwrap_or_default(),
    }
}

/// Install the available GitHub update on request.
pub async fn install(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let _install = state.update_install.lock().await;
    if let Some(update) = find_update(app).await? {
        install_release(app, &update, || true).await?;
    }
    Ok(())
}

async fn install_release(
    app: &AppHandle,
    update: &Update,
    should_install: impl FnOnce() -> bool,
) -> Result<(), String> {
    // The updater verifies the release signature before returning the bytes.
    let installer = update
        .download(|_, _| {}, || {})
        .await
        .map_err(|error| error.to_string())?;
    if should_install() {
        // On Windows the updater exits as soon as it starts the NSIS installer.
        crate::windows::persist_widgets_before_exit(app);
        update
            .install(installer)
            .map_err(|error| error.to_string())?;
        app.restart();
    }
    Ok(())
}

async fn check_periodically(app: &AppHandle) {
    let state = app.state::<AppState>();
    let Ok(_install) = state.update_install.try_lock() else {
        return;
    };
    let Ok(Some(update)) = find_update(app).await else {
        return;
    };
    let info = update_info(&update);
    *state.update_available.lock().unwrap() = Some(info.clone());
    let _ = app.emit("update://available", info);

    let auto_install_enabled = || {
        !cfg!(debug_assertions) && state.settings.lock().unwrap().github_auto_install_updates
    };
    if auto_install_enabled() {
        // Recheck consent after download in case the setting changed while it was in progress.
        if let Err(error) = install_release(app, &update, auto_install_enabled).await {
            eprintln!("Automatic update failed: {error}");
        }
    }
}

/// Check for updates on startup and every 10 minutes on GitHub builds.
pub fn start_periodic(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            check_periodically(&app).await;
            tokio::time::sleep(std::time::Duration::from_secs(600)).await;
        }
    });
}
