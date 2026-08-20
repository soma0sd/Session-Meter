//! ChatGPT Codex subscription-quota provider.
//!
//! Codex subscription limits are not exposed through the public OpenAI API. The Codex
//! desktop client currently reads the authenticated ChatGPT web endpoint below. Keep this
//! adapter deliberately small and defensive: SessionMeter intentionally exposes only the
//! seven-day Codex subscription window. The endpoint's `primary_window` and
//! `secondary_window` names describe roles, not durations, so their
//! `limit_window_seconds` value determines which response window is eligible.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Map, Value};
use tauri::{AppHandle, Emitter, Manager};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::api::{Bucket, UsageSnapshot, WindowUsage};
use crate::config;
use crate::error::AppError;
use crate::state::AppState;

const USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
const WEEKLY_WINDOW_SECONDS: u64 = 7 * 24 * 60 * 60;
const LOGIN_TIMEOUT: Duration = Duration::from_secs(295);
const LOGIN_RESULT_POLL_INTERVAL: Duration = Duration::from_millis(250);

// A Codex sign-in owns a disposable WebView2 profile. Do not allow two helpers to touch it or
// present duplicate login windows at the same time.
static CODEX_LOGIN_BUSY: AtomicBool = AtomicBool::new(false);
static CODEX_LOGIN_EPOCH: AtomicU64 = AtomicU64::new(0);
static CODEX_SESSION_LOCK: Mutex<()> = Mutex::new(());

struct LoginGuard;

impl LoginGuard {
    fn try_acquire() -> Option<Self> {
        (!CODEX_LOGIN_BUSY.swap(true, Ordering::SeqCst)).then_some(Self)
    }
}

impl Drop for LoginGuard {
    fn drop(&mut self) {
        CODEX_LOGIN_BUSY.store(false, Ordering::SeqCst);
    }
}

fn session_lock() -> MutexGuard<'static, ()> {
    CODEX_SESSION_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn login_is_current(epoch: u64) -> bool {
    CODEX_LOGIN_EPOCH.load(Ordering::SeqCst) == epoch
}

fn reset_iso(value: &Value) -> Result<String, AppError> {
    let secs = value
        .as_i64()
        .or_else(|| {
            value
                .as_f64()
                .filter(|v| v.is_finite())
                .map(|v| v.round() as i64)
        })
        .ok_or_else(|| AppError::Parse("Codex reset_at is invalid".to_string()))?;
    let time = OffsetDateTime::from_unix_timestamp(secs)
        .map_err(|_| AppError::Parse("Codex reset_at is out of range".to_string()))?;
    time.format(&Rfc3339)
        .map_err(|e| AppError::Parse(e.to_string()))
}

fn weekly_bucket_from(raw: &Value) -> Result<Bucket, AppError> {
    let used = raw
        .get("used_percent")
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite() && (0.0..=100.0).contains(v))
        .ok_or_else(|| AppError::Parse("Codex used_percent is invalid".to_string()))?;
    let seconds = raw
        .get("limit_window_seconds")
        .and_then(Value::as_u64)
        .filter(|&v| v > 0)
        .ok_or_else(|| AppError::Parse("Codex limit_window_seconds is invalid".to_string()))?;
    if seconds != WEEKLY_WINDOW_SECONDS {
        return Err(AppError::Parse(
            "Codex weekly limit_window_seconds is invalid".to_string(),
        ));
    }
    let resets_at = reset_iso(
        raw.get("reset_at")
            .ok_or_else(|| AppError::Parse("Codex reset_at is missing".to_string()))?,
    )?;
    let utilization = used.round() as u8;
    Ok(Bucket {
        key: "codex-weekly".to_string(),
        label: "Codex weekly".to_string(),
        remaining: 100u8.saturating_sub(utilization),
        utilization,
        resets_at,
    })
}

fn weekly_window<'a>(limits: &'a Map<String, Value>) -> Option<&'a Value> {
    ["primary_window", "secondary_window"]
        .iter()
        .filter_map(|key| limits.get(*key))
        .find(|window| {
            window.get("limit_window_seconds").and_then(Value::as_u64)
                == Some(WEEKLY_WINDOW_SECONDS)
        })
}

fn to_window(bucket: &Bucket) -> WindowUsage {
    WindowUsage {
        remaining: bucket.remaining,
        utilization: bucket.utilization,
        resets_at: bucket.resets_at.clone(),
    }
}

pub fn parse_usage(raw: &Value) -> Result<UsageSnapshot, AppError> {
    let limits = raw
        .get("rate_limit")
        .and_then(Value::as_object)
        .ok_or_else(|| AppError::Parse("Codex rate_limit missing".to_string()))?;
    let weekly = weekly_window(limits)
        .ok_or_else(|| AppError::Parse("Codex weekly window missing".to_string()))?;
    let weekly = weekly_bucket_from(weekly)?;

    Ok(UsageSnapshot {
        service_id: crate::service::CODEX.to_string(),
        // `five_hour` is the legacy serialization slot for a service's first headline
        // window. It carries Codex's weekly quota here and does not imply a 5-hour session.
        five_hour: Some(to_window(&weekly)),
        weekly_primary: None,
        primary_key: Some(weekly.key.clone()),
        secondary_key: None,
        buckets: vec![weekly],
        organization_name: "Codex".to_string(),
        account_email: String::new(),
        subscription: raw
            .get("plan_type")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        fetched_at: crate::api::now_iso(),
        status: "ok".to_string(),
    })
}

pub async fn fetch_usage(
    client: &reqwest::Client,
    cookie: &str,
) -> Result<UsageSnapshot, AppError> {
    let resp = client
        .get(USAGE_URL)
        .header(reqwest::header::COOKIE, cookie)
        .header(reqwest::header::ACCEPT, "application/json")
        .header("OAI-App-Brand", "codex")
        .send()
        .await?;
    ensure_success(resp.status().as_u16())?;
    let raw = resp
        .json::<Value>()
        .await
        .map_err(|e| AppError::Parse(e.to_string()))?;
    parse_usage(&raw)
}

/// Start a disposable, isolated Codex login webview. The helper must first prove that its browser
/// session can fetch the usage endpoint. This parent process then validates the returned cookie a
/// second time before DPAPI-backed persistence, so no helper output can create a signed-in state
/// on its own.
pub fn start_login(app: &AppHandle) {
    // Take this synchronously so a second Settings click cannot enqueue an otherwise harmless
    // helper that would invalidate the first attempt's completion epoch.
    let Some(login_guard) = LoginGuard::try_acquire() else {
        return;
    };
    let epoch = {
        let _session_guard = session_lock();
        CODEX_LOGIN_EPOCH.fetch_add(1, Ordering::SeqCst) + 1
    };
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _login_guard = login_guard;

        let profile = helper_profile_dir();
        if std::fs::create_dir_all(&profile).is_err() {
            if login_is_current(epoch) {
                crate::usage::mark_status(&app, crate::service::CODEX, "error");
            }
            return;
        }

        let mut child = {
            // If logout won the race before this background task launched, never present a stale
            // helper window. Holding the lock only around `spawn` also orders this with logout
            // without blocking it during user interaction.
            let _session_guard = session_lock();
            if !login_is_current(epoch) {
                let _ = std::fs::remove_dir_all(&profile);
                return;
            }
            spawn_login_helper(&profile)
        };
        let result = match child.as_mut() {
            Ok(child) => read_result(child, LOGIN_TIMEOUT, epoch),
            Err(_) => None,
        };
        // The helper is reaped by `read_result` before this directory is removed. The profile is
        // intentionally temporary because the encrypted session file is the sole persisted copy.
        let _ = std::fs::remove_dir_all(&profile);

        let Some(cookie) = result.as_deref().and_then(helper_cookie) else {
            if login_is_current(epoch) && !matches!(result.as_deref(), Some("CANCELLED")) {
                crate::usage::mark_status(&app, crate::service::CODEX, "error");
            }
            return;
        };

        if !login_is_current(epoch) {
            return;
        }

        let Some(client) = app
            .try_state::<AppState>()
            .map(|state| state.client.clone())
        else {
            if login_is_current(epoch) {
                crate::usage::mark_status(&app, crate::service::CODEX, "error");
            }
            return;
        };
        let snapshot = match tauri::async_runtime::block_on(fetch_usage(&client, cookie)) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                let status = if matches!(error, AppError::Unauthorized) {
                    "unauthorized"
                } else {
                    "error"
                };
                if login_is_current(epoch) {
                    crate::usage::mark_status(&app, crate::service::CODEX, status);
                }
                return;
            }
        };

        // Serialize persistence and UI transition with logout. A late helper result must never
        // restore a session that the user explicitly removed while the login was open.
        let _session_guard = session_lock();
        if !login_is_current(epoch) {
            return;
        }
        if config::save_cookie(&app, crate::service::CODEX, cookie).is_err() {
            crate::usage::mark_status(&app, crate::service::CODEX, "error");
            return;
        }

        let org_name = snapshot.organization_name.clone();
        let email = snapshot.account_email.clone();
        crate::usage::apply_snapshot(&app, snapshot);
        let _ = app.emit(
            "session://changed",
            serde_json::json!({
                "service": crate::service::CODEX,
                "logged_in": true,
                "org_name": org_name,
                "email": email,
            }),
        );
        let app_for_windows = app.clone();
        drop(_session_guard);
        let _ = app.run_on_main_thread(move || {
            crate::windows::reconcile_widget_visibility(&app_for_windows);
        });
    });
}

/// Clear the persisted Codex session and invalidate any helper attempt that was already open.
/// The same lock used by `start_login` prevents a late helper result from restoring this session.
pub fn clear_session(app: &AppHandle) -> Result<(), AppError> {
    let _session_guard = session_lock();
    CODEX_LOGIN_EPOCH.fetch_add(1, Ordering::SeqCst);
    config::clear_cookie(app, crate::service::CODEX)?;
    crate::usage::mark_status(app, crate::service::CODEX, "not_logged_in");
    crate::windows::hide_runtime_widget(app, crate::service::CODEX);
    let _ = app.emit(
        "session://changed",
        serde_json::json!({
            "service": crate::service::CODEX,
            "logged_in": false,
            "org_name": "",
            "email": "",
        }),
    );
    Ok(())
}

/// Return the short-lived WebView2 user-data folder for one helper invocation. A time component
/// prevents stale folders from a killed process colliding with a later user sign-in.
fn helper_profile_dir() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "sessionmeter-codex-login-{}-{nonce}",
        std::process::id()
    ))
}

/// Spawn the same executable in its bare tao+wry helper mode. Explicitly remove the Gemini-only
/// client-hint override in case this application was started from an environment that supplied it.
fn spawn_login_helper(profile: &Path) -> std::io::Result<Child> {
    let exe = std::env::current_exe()?;
    Command::new(exe)
        .env("SM_CODEX_MODE", "login")
        .env("SM_CODEX_UDF", profile)
        .env_remove("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
}

/// Receive a helper result within the deadline, then always terminate and reap the helper. The
/// short polling interval also observes logout promptly, so its stale helper process does not
/// keep the single-login guard occupied until the full interactive timeout elapses.
fn read_result(child: &mut Child, timeout: Duration, epoch: u64) -> Option<String> {
    let stdout = child.stdout.take()?;
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Some(payload) = line.strip_prefix("SM_RESULT ") {
                let _ = sender.send(payload.to_string());
                break;
            }
        }
    });
    let payload = wait_for_result(&receiver, timeout, || !login_is_current(epoch));
    let _ = child.kill();
    let _ = child.wait();
    payload
}

fn wait_for_result<F>(
    receiver: &mpsc::Receiver<String>,
    timeout: Duration,
    mut is_stale: F,
) -> Option<String>
where
    F: FnMut() -> bool,
{
    let started = Instant::now();
    loop {
        if is_stale() {
            return None;
        }
        let remaining = timeout.checked_sub(started.elapsed())?;
        match receiver.recv_timeout(result_poll_delay(remaining)) {
            Ok(payload) => return Some(payload),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return None,
        }
    }
}

fn result_poll_delay(remaining: Duration) -> Duration {
    remaining.min(LOGIN_RESULT_POLL_INTERVAL)
}

/// Accept only the helper's single-line cookie transport. Never log this payload.
fn helper_cookie(payload: &str) -> Option<&str> {
    payload
        .strip_prefix("COOKIE ")
        .filter(|cookie| !cookie.is_empty() && !cookie.contains(['\r', '\n']))
}

fn ensure_success(code: u16) -> Result<(), AppError> {
    match code {
        200..=299 => Ok(()),
        401 | 403 => Err(AppError::Unauthorized),
        _ => Err(AppError::Http(format!("HTTP {code}"))),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn selects_the_weekly_window_and_ignores_a_short_window() {
        let snap = parse_usage(&json!({
            "plan_type": "pro",
            "rate_limit": {
                "primary_window": {"used_percent": 34.6, "reset_at": 1_800_000_000, "limit_window_seconds": 18_000},
                "secondary_window": {"used_percent": 75, "reset_at": 1_800_300_000, "limit_window_seconds": 604_800}
            }
        }))
        .expect("valid Codex usage");

        assert_eq!(snap.service_id, crate::service::CODEX);
        assert_eq!(snap.five_hour.as_ref().map(|w| w.remaining), Some(25));
        assert!(snap.weekly_primary.is_none());
        assert_eq!(snap.buckets.len(), 1);
        assert_eq!(snap.buckets[0].key, "codex-weekly");
        assert_eq!(snap.primary_key.as_deref(), Some("codex-weekly"));
        assert!(snap.secondary_key.is_none());
        assert_eq!(snap.subscription, "pro");
        assert!(snap.buckets[0].resets_at.starts_with("2027-01-"));
    }

    #[test]
    fn accepts_a_weekly_primary_window_without_secondary_window() {
        let snap = parse_usage(&json!({
            "rate_limit": {"primary_window": {"used_percent": 0, "reset_at": 1_800_000_000, "limit_window_seconds": 604_800}}
        }))
        .expect("weekly primary window is sufficient");

        assert!(snap.weekly_primary.is_none());
        assert!(snap.secondary_key.is_none());
        assert_eq!(snap.buckets.len(), 1);
        assert_eq!(snap.buckets[0].key, "codex-weekly");
    }

    #[test]
    fn rejects_missing_or_invalid_weekly_window() {
        assert!(parse_usage(&json!({"rate_limit": {}})).is_err());
        assert!(parse_usage(&json!({"rate_limit": {"primary_window": {}}})).is_err());
        assert!(parse_usage(&json!({
            "rate_limit": {"primary_window": {"used_percent": 101, "reset_at": 1_800_000_000, "limit_window_seconds": 604_800}}
        }))
        .is_err());
        assert!(parse_usage(&json!({
            "rate_limit": {"primary_window": {"used_percent": 10, "reset_at": "tomorrow", "limit_window_seconds": 604_800}}
        }))
        .is_err());
        assert!(parse_usage(&json!({
            "rate_limit": {"primary_window": {"used_percent": 10, "reset_at": 1_800_000_000, "limit_window_seconds": 0}}
        }))
        .is_err());
        assert!(parse_usage(&json!({
            "rate_limit": {"primary_window": {"used_percent": 10, "reset_at": 1_800_000_000, "limit_window_seconds": 18_000}}
        }))
        .is_err());
    }

    #[test]
    fn maps_auth_statuses_to_session_expiry() {
        assert!(matches!(ensure_success(401), Err(AppError::Unauthorized)));
        assert!(matches!(ensure_success(403), Err(AppError::Unauthorized)));
        assert!(matches!(ensure_success(500), Err(AppError::Http(_))));
        assert!(ensure_success(200).is_ok());
    }

    #[test]
    fn accepts_only_single_line_helper_cookie_transport() {
        assert_eq!(helper_cookie("COOKIE session=value"), Some("session=value"));
        assert!(helper_cookie("COOKIE ").is_none());
        assert!(helper_cookie("COOKIE session=value\nnext").is_none());
        assert!(helper_cookie("CANCELLED").is_none());
    }

    #[test]
    fn helper_result_poll_is_bounded_and_cancellable() {
        assert_eq!(
            result_poll_delay(Duration::from_secs(1)),
            LOGIN_RESULT_POLL_INTERVAL
        );
        assert_eq!(
            result_poll_delay(Duration::from_millis(10)),
            Duration::from_millis(10)
        );

        let (_sender, receiver) = std::sync::mpsc::channel::<String>();
        assert!(wait_for_result(&receiver, Duration::from_secs(1), || true).is_none());
    }
}
