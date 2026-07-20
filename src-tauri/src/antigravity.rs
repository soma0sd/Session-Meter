//! Antigravity / Gemini usage provider (parent side).
//!
//! There is no OAuth-token API for the Gemini subscription's aggregate current/weekly usage; the
//! numbers live behind the Google web session on gemini.google.com/usage. Google also blocks
//! embedded sign-in and, worse, loading the Google login page in the app's shared WebView2 UI
//! thread freezes the whole app. So all Gemini webview work is pushed into a SEPARATE PROCESS
//! (see `gemini_helper`): the main app relaunches its own binary with `SM_GEMINI_MODE=login`
//! (interactive sign-in) or `=scrape` (hidden /usage DOM scrape), reads a single `SM_RESULT` line
//! from the child's stdout, and kills it on timeout. The main app never hosts a Gemini webview, so
//! a wedged Google page can never freeze it.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde::Deserialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::api::{now_iso, Bucket, UsageSnapshot, WindowUsage};
use crate::error::AppError;
use crate::service::ANTIGRAVITY;

// A logged-in marker stored in the (main-app) session file so `service::has_session` reports the
// service as signed in. The real Google session lives only in the helper's WebView2 profile; the
// main app never handles the raw cookie.
const LOGGED_IN_MARKER: &str = "webview-profile";

// Serializes the helper processes: login and scrape both drive a Gemini webview against the SAME
// dedicated WebView2 profile, and two of them touching that profile at once (e.g. a poll-driven
// scrape firing while the login window is open, or right as the login process tears down) risks a
// WebView2 profile-lock/contention hang. Only one helper runs at a time.
static GEMINI_BUSY: AtomicBool = AtomicBool::new(false);

/// RAII guard for `GEMINI_BUSY`; releases on drop so every early-return path clears it.
struct BusyGuard;
impl BusyGuard {
    /// Non-blocking: returns None if a helper is already running (caller should skip).
    fn try_acquire() -> Option<BusyGuard> {
        if GEMINI_BUSY.swap(true, Ordering::SeqCst) {
            None
        } else {
            Some(BusyGuard)
        }
    }
    /// Blocking: waits up to `max` for an in-flight helper to finish, then takes the slot anyway
    /// (a stuck helper must not permanently block a user-initiated login).
    fn acquire_waiting(max: Duration) -> BusyGuard {
        let start = Instant::now();
        while GEMINI_BUSY.swap(true, Ordering::SeqCst) {
            if start.elapsed() > max {
                GEMINI_BUSY.store(true, Ordering::SeqCst);
                break;
            }
            std::thread::sleep(Duration::from_millis(300));
        }
        BusyGuard
    }
}
impl Drop for BusyGuard {
    fn drop(&mut self) {
        GEMINI_BUSY.store(false, Ordering::SeqCst);
    }
}

#[derive(Deserialize)]
struct ScrapeItem {
    pct: u8,
    #[serde(rename = "resetIso")]
    reset_iso: String,
}
#[derive(Deserialize)]
struct ScrapeResult {
    items: Vec<ScrapeItem>,
    #[serde(default)]
    plan: String,
}

/// Identity is not captured (the helper does not surface the account email).
pub fn account_email(_app: &AppHandle) -> String {
    String::new()
}

/// Dedicated WebView2 user-data-folder for the helper processes, isolated from the main app's
/// profile. Login persists here so a later scrape is already signed in.
fn helper_udf(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("gemini_profile")
}

/// Relaunch our own binary in a Gemini helper mode ("login" or "scrape") as a separate process.
fn spawn_helper(app: &AppHandle, mode: &str) -> std::io::Result<Child> {
    let exe = std::env::current_exe()?;
    Command::new(exe)
        .env("SM_GEMINI_MODE", mode)
        .env("SM_GEMINI_UDF", helper_udf(app))
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
}

/// Read the child's single `SM_RESULT <payload>` line, bounded by `timeout`; then kill the child
/// (closing its webview window) and reap it. Returns the payload, or None on timeout/EOF.
fn read_result(child: &mut Child, timeout: Duration) -> Option<String> {
    let stdout = child.stdout.take()?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Some(rest) = line.strip_prefix("SM_RESULT ") {
                let _ = tx.send(rest.to_string());
                break;
            }
        }
    });
    let payload = rx.recv_timeout(timeout).ok();
    let _ = child.kill();
    let _ = child.wait();
    payload
}

fn to_window(b: &Bucket) -> WindowUsage {
    WindowUsage {
        remaining: b.remaining,
        utilization: b.utilization,
        resets_at: b.resets_at.clone(),
    }
}

/// Build a snapshot from the helper's scrape JSON (`{"items":[{pct,resetIso}...],"plan":"..."}`).
fn build_snapshot(json: &str) -> Result<UsageSnapshot, AppError> {
    let parsed: ScrapeResult =
        serde_json::from_str(json).map_err(|e| AppError::Parse(e.to_string()))?;
    let labels = ["Current usage", "Weekly limit"];
    let keys = ["current", "weekly"];
    let mut buckets = Vec::new();
    for (i, it) in parsed.items.iter().take(2).enumerate() {
        let utilization = it.pct.min(100);
        buckets.push(Bucket {
            key: keys[i].to_string(),
            label: labels[i].to_string(),
            remaining: 100u8.saturating_sub(utilization),
            utilization,
            resets_at: it.reset_iso.clone(),
        });
    }
    if buckets.is_empty() {
        return Err(AppError::Other("no usage found".to_string()));
    }
    let five_hour = buckets.first().map(to_window);
    let weekly_primary = buckets.get(1).map(to_window);
    Ok(UsageSnapshot {
        service_id: ANTIGRAVITY.to_string(),
        five_hour,
        weekly_primary,
        primary_key: Some("current".to_string()),
        secondary_key: Some("weekly".to_string()),
        buckets,
        organization_name: "Antigravity".to_string(),
        account_email: String::new(),
        subscription: if parsed.plan.is_empty() {
            String::new()
        } else {
            format!("Gemini {}", parsed.plan)
        },
        fetched_at: now_iso(),
        status: "ok".to_string(),
    })
}

/// Open the Gemini sign-in in a SEPARATE process. On success, mark the service signed in and kick
/// an immediate scrape so the widget populates. Runs entirely off the main thread.
pub fn start_login(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        // Hold the helper slot only for the login itself; release it before the kick-scrape below
        // so that scrape can take the slot cleanly (no overlap on the shared profile).
        let result = {
            let _guard = BusyGuard::acquire_waiting(Duration::from_secs(8));
            let mut child = match spawn_helper(&app, "login") {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[cg][ag] login helper spawn error: {e}");
                    return;
                }
            };
            eprintln!("[cg][ag] login helper spawned");
            read_result(&mut child, Duration::from_secs(295))
        };
        match result.as_deref() {
            Some("LOGIN_OK") => {
                eprintln!("[cg][ag] google session captured (helper)");
                let _ = crate::config::save_cookie(&app, ANTIGRAVITY, LOGGED_IN_MARKER);
                let _ = app.emit(
                    "session://changed",
                    serde_json::json!({ "service": ANTIGRAVITY, "logged_in": true, "org_name": "Antigravity", "email": "" }),
                );
                let app2 = app.clone();
                tauri::async_runtime::spawn(async move {
                    if let Ok(snap) = fetch(&app2, &reqwest::Client::new()).await {
                        crate::usage::apply_snapshot(&app2, snap);
                    }
                });
            }
            other => eprintln!("[cg][ag] login helper ended: {other:?}"),
        }
    });
}

/// Single entry point for `service::fetch`: scrape /usage in a separate process.
pub async fn fetch(app: &AppHandle, _client: &reqwest::Client) -> Result<UsageSnapshot, AppError> {
    if crate::config::load_cookie(app, ANTIGRAVITY).is_none() {
        return Err(AppError::NoSession);
    }
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        // Skip this scrape if a login window or another scrape is already driving the profile.
        let _guard = match BusyGuard::try_acquire() {
            Some(g) => g,
            None => return Err(AppError::Other("gemini helper busy".to_string())),
        };
        let mut child =
            spawn_helper(&app, "scrape").map_err(|e| AppError::Other(e.to_string()))?;
        match read_result(&mut child, Duration::from_secs(45)) {
            Some(payload) => match payload.as_str() {
                "LOGIN" => Err(AppError::Unauthorized),
                "TIMEOUT" | "CLOSED" => {
                    Err(AppError::Other("gemini usage scrape timed out".to_string()))
                }
                json => build_snapshot(json),
            },
            None => Err(AppError::Other("gemini scrape helper no result".to_string())),
        }
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}
