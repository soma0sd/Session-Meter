//! Background polling loop. Fetches usage on startup and every `refresh_interval_min`
//! minutes. The interval is re-read every few seconds so settings changes apply promptly
//! without restarting the task.

use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use crate::error::AppError;
use crate::state::AppState;
use crate::{service, tray, usage};

const CHECK_STEP_SECS: u64 = 5;

fn interval_secs(app: &AppHandle) -> u64 {
    app.try_state::<AppState>()
        .map(|s| s.settings.lock().unwrap().refresh_interval_min)
        .unwrap_or(5)
        .max(1)
        * 60
}

async fn poll_once(app: &AppHandle) {
    let services = service::logged_in(app);
    if services.is_empty() {
        tray::update_tray(app);
        return;
    }
    let Some(client) = app.try_state::<AppState>().map(|s| s.client.clone()) else {
        return;
    };
    for svc in services {
        match service::fetch(app, &svc, &client).await {
            Ok(snapshot) => {
                eprintln!(
                    "[cg] poll ok: service='{}' org='{}' buckets={} primary={:?}",
                    svc,
                    snapshot.organization_name,
                    snapshot.buckets.len(),
                    snapshot.five_hour.as_ref().map(|w| w.remaining)
                );
                usage::apply_snapshot(app, snapshot);
            }
            Err(AppError::Unauthorized) => {
                eprintln!("[cg] poll: unauthorized ({svc}, session expired)");
                if let Some(state) = app.try_state::<AppState>() {
                    if let Some(s) = state.last_snapshot.lock().unwrap().get_mut(&svc) {
                        s.status = "unauthorized".to_string();
                    }
                }
                let _ = app.emit(
                    "session://changed",
                    serde_json::json!({ "service": svc, "logged_in": false, "org_name": "" }),
                );
                tray::update_tray(app);
            }
            Err(e) => {
                eprintln!("[cg] poll error ({svc}): {e}");
            }
        }
    }
}

pub fn start(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            poll_once(&app).await;
            // Re-assert the widget's desired visibility (recovers a window that drifted
            // off-screen or got hidden). Window ops run on the main thread.
            let a = app.clone();
            let _ = app.run_on_main_thread(move || crate::windows::reconcile_widget_visibility(&a));
            let mut waited = 0u64;
            loop {
                tokio::time::sleep(Duration::from_secs(CHECK_STEP_SECS)).await;
                waited += CHECK_STEP_SECS;
                if waited >= interval_secs(&app) {
                    break;
                }
            }
        }
    });
}
