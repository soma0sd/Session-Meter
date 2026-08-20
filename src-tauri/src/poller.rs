//! Background polling loop. Fetches usage on startup and every `refresh_interval_min`
//! minutes. The interval is re-read every few seconds so settings changes apply promptly
//! without restarting the task.

use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use crate::api::{self, UsageSnapshot};
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
        // Keep the exact Codex credential that started the request. Every Codex result state
        // transition must be conditional on it still being persisted when the request returns.
        // The other providers retain their established fetch path and invalidation behavior.
        let (result, codex_session) = if svc == service::CODEX {
            match crate::config::load_cookie(app, &svc) {
                Some(session) => {
                    let result = service::fetch_with_cookie(&svc, &client, &session).await;
                    (result, Some(session))
                }
                None => (Err(AppError::NoSession), None),
            }
        } else {
            (service::fetch(app, &svc, &client).await, None)
        };
        match result {
            Ok(snapshot) => {
                let organization_name = snapshot.organization_name.clone();
                let bucket_count = snapshot.buckets.len();
                let primary_remaining = snapshot.five_hour.as_ref().map(|window| window.remaining);
                let applied = if svc == service::CODEX {
                    match codex_session.as_deref() {
                        Some(expected_session) => crate::codex::apply_snapshot_if_session_current(
                            app,
                            expected_session,
                            snapshot,
                        ),
                        None => false,
                    }
                } else {
                    usage::apply_snapshot(app, snapshot);
                    true
                };
                if !applied {
                    eprintln!("[cg] ignored stale Codex poll result");
                    continue;
                }
                eprintln!(
                    "[cg] poll ok: service='{}' org='{}' buckets={} primary={:?}",
                    svc, organization_name, bucket_count, primary_remaining,
                );
            }
            Err(AppError::Unauthorized) => {
                eprintln!("[cg] poll: unauthorized ({svc}, session expired)");
                if svc == service::CODEX {
                    let Some(expected_session) = codex_session.as_deref() else {
                        continue;
                    };
                    match crate::codex::invalidate_session_if_current(app, expected_session) {
                        Ok(true) => {}
                        Ok(false) => {
                            eprintln!("[cg] ignored stale Codex session expiry");
                        }
                        Err(error) => {
                            // The Codex helper already updated its status while holding its
                            // session lock. Preserve that fail-closed UI state without allowing
                            // a generic poll branch to overwrite a newer login.
                            eprintln!(
                                "[cg] could not fully invalidate expired Codex session: {error}"
                            );
                        }
                    }
                    continue;
                }
                if let Err(error) = crate::config::invalidate_cookie(app, &svc) {
                    // `invalidate_cookie` writes a fail-closed marker before deleting the
                    // credential whenever possible, so this error cannot resurrect an expired
                    // Codex session after restart.  The unauthorised snapshot below remains the
                    // visible state even if both filesystem operations failed.
                    eprintln!("[cg] could not fully invalidate expired {svc} session: {error}");
                }
                usage::mark_status(app, &svc, "unauthorized");
                let _ = app.emit(
                    "session://changed",
                    serde_json::json!({ "service": svc, "logged_in": false, "org_name": "" }),
                );
            }
            Err(AppError::NotRunning) => {
                // Antigravity IDE just isn't running right now - not a sign-out (the
                // has_session marker stays put), only "temporarily unavailable". Routing
                // through apply_snapshot keeps cache/history/tray/usage://updated consistent
                // with every other status change instead of a bespoke code path.
                let placeholder = UsageSnapshot {
                    service_id: svc.clone(),
                    five_hour: None,
                    weekly_primary: None,
                    primary_key: None,
                    secondary_key: None,
                    buckets: Vec::new(),
                    organization_name: String::new(),
                    account_email: String::new(),
                    subscription: String::new(),
                    fetched_at: api::now_iso(),
                    status: "not_running".to_string(),
                };
                usage::apply_snapshot(app, placeholder);
            }
            Err(e) => {
                eprintln!("[cg] poll error ({svc}): {e}");
                if svc == service::CODEX {
                    let transitioned = match codex_session.as_deref() {
                        Some(expected_session) => crate::codex::mark_status_if_session_current(
                            app,
                            expected_session,
                            "error",
                        ),
                        None => false,
                    };
                    if !transitioned {
                        eprintln!("[cg] ignored stale Codex poll error");
                    }
                } else {
                    usage::mark_status(app, &svc, "error");
                }
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
                // Correct any dock drift every CHECK_STEP_SECS, independent of the (possibly
                // much longer) usage refresh interval, so a docked group snaps back quickly.
                let a = app.clone();
                let _ = app.run_on_main_thread(move || crate::dock::watchdog_tick(&a));
                if waited >= interval_secs(&app) {
                    break;
                }
            }
        }
    });
}
