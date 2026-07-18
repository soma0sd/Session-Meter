//! Central place to apply a freshly fetched snapshot: cache it, record history, notify
//! the frontend, and repaint the tray. Called by refresh, capture, initial load, and the
//! background poller so behavior stays consistent.

use tauri::{AppHandle, Emitter, Manager};

use crate::api::UsageSnapshot;
use crate::state::AppState;
use crate::{history, notify, tray};

pub fn apply_snapshot(app: &AppHandle, snapshot: UsageSnapshot) {
    if let Some(state) = app.try_state::<AppState>() {
        *state.last_snapshot.lock().unwrap() = Some(snapshot.clone());
        // Keep the persisted account identity (name + email) fresh so the settings
        // account panel reflects the signed-in account even after a restart.
        if snapshot.status == "ok" && !snapshot.organization_name.is_empty() {
            let mut settings = state.settings.lock().unwrap();
            if settings.org_name != snapshot.organization_name
                || settings.account_email != snapshot.account_email
            {
                settings.org_name = snapshot.organization_name.clone();
                settings.account_email = snapshot.account_email.clone();
                let _ = crate::config::save(app, &settings);
            }
        }
    }
    history::record(app, &snapshot);
    notify::evaluate(app, &snapshot);
    let _ = app.emit("usage://updated", &snapshot);
    tray::update_tray(app);
}
