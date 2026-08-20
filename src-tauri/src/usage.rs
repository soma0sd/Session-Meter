//! Central place to apply a freshly fetched snapshot: cache it, record history, notify
//! the frontend, and repaint the tray. Called by refresh, capture, initial load, and the
//! background poller so behavior stays consistent.

use tauri::{AppHandle, Emitter, Manager};

use crate::api::UsageSnapshot;
use crate::state::AppState;
use crate::{history, notify, tray};

pub fn apply_snapshot(app: &AppHandle, snapshot: UsageSnapshot) {
    if let Some(state) = app.try_state::<AppState>() {
        state
            .last_snapshot
            .lock()
            .unwrap()
            .insert(snapshot.service_id.clone(), snapshot.clone());
        // Keep the persisted account identity (name + email) fresh so the settings
        // account panel reflects the signed-in account even after a restart. Claude's
        // identity backs the (single) settings account fields; other services carry their
        // own identity elsewhere.
        if snapshot.service_id == crate::service::CLAUDE
            && snapshot.status == "ok"
            && !snapshot.organization_name.is_empty()
        {
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

/// Preserve the most recent values while exposing a temporary fetch failure or expired
/// browser session to every open view. Unlike `apply_snapshot`, this does not append an
/// artificial point to quota history or evaluate notifications.
pub fn mark_status(app: &AppHandle, service: &str, status: &str) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let snapshot = {
        let mut snapshots = state.last_snapshot.lock().unwrap();
        let snapshot = snapshots
            .entry(service.to_string())
            .or_insert_with(|| UsageSnapshot {
                service_id: service.to_string(),
                five_hour: None,
                weekly_primary: None,
                primary_key: None,
                secondary_key: None,
                buckets: Vec::new(),
                organization_name: crate::service::display_name(service).to_string(),
                account_email: String::new(),
                subscription: String::new(),
                fetched_at: crate::api::now_iso(),
                status: status.to_string(),
            });
        snapshot.status = status.to_string();
        snapshot.clone()
    };
    let _ = app.emit("usage://updated", &snapshot);
    tray::update_tray(app);
}
