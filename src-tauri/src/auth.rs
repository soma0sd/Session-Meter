//! Session capture from embedded browser-login webviews.
//!
//! The `sessionKey` cookie is httpOnly, so it can't be read from page JS; we read it
//! from the webview's cookie store instead. `cookies_for_url` is blocking and can
//! deadlock if called on the main/UI thread on Windows, so callers must run this on a
//! blocking thread (this fn is async and offloads via `spawn_blocking`).

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager};

use crate::error::AppError;
use crate::state::AppState;

fn cookie_origin(service: &str) -> Result<url::Url, AppError> {
    let origin = match service {
        crate::service::CLAUDE => "https://claude.ai",
        crate::service::CODEX => "https://chatgpt.com",
        other => return Err(AppError::Other(format!("service has no browser session: {other}"))),
    };
    origin
        .parse()
        .map_err(|e: url::ParseError| AppError::Other(e.to_string()))
}

/// Collect all cookies for a browser-login service and build a `Cookie:` header string.
/// Claude's httpOnly `sessionKey` remains an early guard; Codex is validated by a quota
/// fetch below because ChatGPT can change the name of its authenticated browser cookie.
pub async fn capture_cookie(app: &AppHandle, service: &str) -> Result<String, AppError> {
    let win = app
        .get_webview_window("login")
        .ok_or_else(|| AppError::Other("login window is not open".to_string()))?;
    let url = cookie_origin(service)?;

    // Bound the (blocking, UI-thread-bound) cookie read so a hung/blank login webview
    // cannot stall the watcher indefinitely.
    let read = tauri::async_runtime::spawn_blocking(move || win.cookies_for_url(url));
    let cookies = tokio::time::timeout(Duration::from_secs(2), read)
        .await
        .map_err(|_| AppError::Other("cookie read timed out".to_string()))?
        .map_err(|e| AppError::Other(e.to_string()))?
        .map_err(|e| AppError::Other(e.to_string()))?;

    let has_candidate = if service == crate::service::CLAUDE {
        cookies
            .iter()
            .any(|c| c.name() == "sessionKey" && !c.value().is_empty())
    } else {
        cookies.iter().any(|c| !c.value().is_empty())
    };
    if !has_candidate {
        return Err(AppError::NoSession);
    }

    let header = cookies
        .iter()
        .map(|c| format!("{}={}", c.name(), c.value()))
        .collect::<Vec<_>>()
        .join("; ");
    Ok(header)
}

/// Watch the open login window for a valid browser session (Rust-driven, so it works
/// regardless of which UI opened the window). On success: validate, persist to the
/// session file, apply the snapshot, notify the frontend, and close the login window.
pub fn spawn_capture_watch(app: AppHandle, service: String) -> Option<u64> {
    let generation = {
        let state = app.try_state::<AppState>()?;
        let generation = state
            .login_capture_generation
            .fetch_add(1, Ordering::SeqCst)
            + 1;
        state.login_watching.store(true, Ordering::SeqCst);
        generation
    };
    tauri::async_runtime::spawn(async move {
        let started = Instant::now();
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            // Cancelled: the user closed the login window (which hides + clears this flag).
            // Stop before touching the webview so cookies_for_url can't race its teardown.
            if !capture_is_current(&app, generation) {
                eprintln!("[cg] login capture cancelled");
                break;
            }
            if app.get_webview_window("login").is_none() {
                eprintln!("[cg] login window closed before capture");
                break;
            }
            if started.elapsed() > Duration::from_secs(1800) {
                eprintln!("[cg] login capture timed out");
                break;
            }
            match capture_cookie(&app, &service).await {
                Ok(cookie) => {
                    let Some(client) = app.try_state::<AppState>().map(|s| s.client.clone()) else {
                        break;
                    };
                    match crate::service::fetch_with_cookie(&service, &client, &cookie).await {
                        Ok(snapshot) => {
                            if !capture_is_current(&app, generation) {
                                break;
                            }
                            if let Err(error) = crate::config::save_cookie(&app, &service, &cookie) {
                                eprintln!("[cg] could not save {service} session: {error}");
                                // A validated webview cookie is not a signed-in app session
                                // until its encrypted persistence succeeds.  Surface the failure
                                // without publishing a success event or creating a widget.
                                crate::usage::mark_status(&app, &service, "error");
                                if let Some(w) = app.get_webview_window("login") {
                                    let _ = w.close();
                                }
                                break;
                            }
                            let org = snapshot.organization_name.clone();
                            let email = snapshot.account_email.clone();
                            eprintln!(
                                "[cg] captured {service} session: org='{}' buckets={}",
                                org, snapshot.buckets.len()
                            );
                            crate::usage::apply_snapshot(&app, snapshot);
                            let _ = app.emit(
                                "session://changed",
                                serde_json::json!({ "service": service, "logged_in": true, "org_name": org, "email": email }),
                            );
                            let app_for_windows = app.clone();
                            let _ = app.run_on_main_thread(move || {
                                crate::windows::reconcile_widget_visibility(&app_for_windows);
                            });
                            if let Some(w) = app.get_webview_window("login") {
                                let _ = w.close();
                            }
                            break;
                        }
                        Err(e) => eprintln!("[cg] cookie captured but fetch failed: {e}"),
                    }
                }
                Err(AppError::NoSession) => { /* not signed in yet, keep waiting */ }
                Err(e) => eprintln!("[cg] capture watch error: {e}"),
            }
        }
        if let Some(st) = app.try_state::<AppState>() {
            if st.login_capture_generation.load(Ordering::SeqCst) == generation {
                st.login_watching.store(false, Ordering::SeqCst);
            }
        }
    });
    Some(generation)
}

/// Invalidate the active browser-login watcher before hiding or reusing the shared webview.
pub fn cancel_capture_watch(app: &AppHandle) {
    if let Some(state) = app.try_state::<AppState>() {
        state.login_watching.store(false, Ordering::SeqCst);
        state.login_capture_generation.fetch_add(1, Ordering::SeqCst);
    }
}

/// Whether a watcher or blank-page guard still belongs to the current shared login webview.
pub fn capture_is_current(app: &AppHandle, generation: u64) -> bool {
    app.try_state::<AppState>()
        .map(|state| {
            state.login_watching.load(Ordering::SeqCst)
                && state.login_capture_generation.load(Ordering::SeqCst) == generation
        })
        .unwrap_or(false)
}
