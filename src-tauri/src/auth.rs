//! Session capture from the embedded claude.ai login webview.
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

/// Collect all claude.ai cookies from the `login` webview and build a `Cookie:` header
/// string. Fails if `sessionKey` is not present yet (user has not finished signing in).
pub async fn capture_cookie(app: &AppHandle) -> Result<String, AppError> {
    let win = app
        .get_webview_window("login")
        .ok_or_else(|| AppError::Other("login window is not open".to_string()))?;
    let url: url::Url = "https://claude.ai"
        .parse()
        .map_err(|e: url::ParseError| AppError::Other(e.to_string()))?;

    // Bound the (blocking, UI-thread-bound) cookie read so a hung/blank login webview
    // cannot stall the watcher indefinitely.
    let read = tauri::async_runtime::spawn_blocking(move || win.cookies_for_url(url));
    let cookies = tokio::time::timeout(Duration::from_secs(2), read)
        .await
        .map_err(|_| AppError::Other("cookie read timed out".to_string()))?
        .map_err(|e| AppError::Other(e.to_string()))?
        .map_err(|e| AppError::Other(e.to_string()))?;

    // The claude.ai auth cookie is `sessionKey` (httpOnly). Match it exactly so analytics
    // cookies like `activitySessionId` do not trigger premature API calls. Names are
    // logged to help diagnose if the cookie is ever renamed.
    let names: Vec<String> = cookies.iter().map(|c| c.name().to_string()).collect();
    let has_session = cookies
        .iter()
        .any(|c| c.name() == "sessionKey" && !c.value().is_empty());
    eprintln!(
        "[cg] login poll: {} cookies {names:?}, sessionKey={has_session}",
        cookies.len()
    );
    if !has_session {
        return Err(AppError::NoSession);
    }

    let header = cookies
        .iter()
        .map(|c| format!("{}={}", c.name(), c.value()))
        .collect::<Vec<_>>()
        .join("; ");
    Ok(header)
}

/// Watch the open login window for the session cookie (Rust-driven, so it works
/// regardless of which UI opened the window). On success: validate, persist to the
/// session file, apply the snapshot, notify the frontend, and close the login window.
pub fn spawn_capture_watch(app: AppHandle) {
    {
        let Some(state) = app.try_state::<AppState>() else {
            return;
        };
        if state.login_watching.swap(true, Ordering::SeqCst) {
            return; // a watcher is already running
        }
    }
    tauri::async_runtime::spawn(async move {
        let started = Instant::now();
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            // Cancelled: the user closed the login window (which hides + clears this flag).
            // Stop before touching the webview so cookies_for_url can't race its teardown.
            let active = app
                .try_state::<AppState>()
                .map(|s| s.login_watching.load(Ordering::SeqCst))
                .unwrap_or(false);
            if !active {
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
            match capture_cookie(&app).await {
                Ok(cookie) => {
                    let Some(client) = app.try_state::<AppState>().map(|s| s.client.clone()) else {
                        break;
                    };
                    match crate::api::fetch_usage(&client, &cookie).await {
                        Ok(snapshot) => {
                            let _ = crate::config::save_cookie(&app, &cookie);
                            let org = snapshot.organization_name.clone();
                            let email = snapshot.account_email.clone();
                            eprintln!(
                                "[cg] captured session: org='{}' buckets={}",
                                org,
                                snapshot.buckets.len()
                            );
                            // apply_snapshot persists org_name + account_email to settings.
                            crate::usage::apply_snapshot(&app, snapshot);
                            let _ = app.emit(
                                "session://changed",
                                serde_json::json!({ "logged_in": true, "org_name": org, "email": email }),
                            );
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
            st.login_watching.store(false, Ordering::SeqCst);
        }
    });
}
