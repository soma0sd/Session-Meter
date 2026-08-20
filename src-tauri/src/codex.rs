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

use base64::{
    engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD},
    Engine as _,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tauri::{AppHandle, Emitter, Manager};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::api::{Bucket, UsageSnapshot, WindowUsage};
use crate::config;
use crate::error::AppError;
use crate::state::AppState;

const USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
const SESSION_URL: &str = "https://chatgpt.com/api/auth/session";
const CHATGPT_ORIGIN: &str = "https://chatgpt.com";
const CHATGPT_REFERER: &str = "https://chatgpt.com/";
const WEEKLY_WINDOW_SECONDS: u64 = 7 * 24 * 60 * 60;
const LOGIN_TIMEOUT: Duration = Duration::from_secs(295);
const LOGIN_RESULT_POLL_INTERVAL: Duration = Duration::from_millis(250);

// A Codex sign-in owns a disposable WebView2 profile. Do not allow two helpers to touch it or
// present duplicate login windows at the same time.
static CODEX_LOGIN_BUSY: AtomicBool = AtomicBool::new(false);
static CODEX_LOGIN_EPOCH: AtomicU64 = AtomicU64::new(0);
static CODEX_SESSION_LOCK: Mutex<()> = Mutex::new(());

/// The encrypted Codex credential deliberately persists only the browser session and the user
/// agent that established its Cloudflare challenge. OAuth bearer tokens are reacquired from the
/// session endpoint for every request and are never written to disk.
#[derive(Clone, Deserialize, Serialize)]
struct CodexSession {
    cookie: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    user_agent: Option<String>,
}

struct SessionCredentials {
    access_token: String,
    account_id: Option<String>,
}

impl CodexSession {
    fn decode(value: &str) -> Result<Self, AppError> {
        // v1.0.1 stored a raw cookie header. Keep it readable so an existing installation can
        // refresh successfully without forcing a new login solely for this format change.
        let session = serde_json::from_str::<Self>(value).unwrap_or_else(|_| Self {
            cookie: value.to_string(),
            user_agent: None,
        });
        if !is_safe_header_value(&session.cookie) {
            return Err(AppError::Parse(
                "Codex browser cookie is invalid".to_string(),
            ));
        }
        Ok(Self {
            cookie: session.cookie,
            user_agent: session.user_agent.filter(|agent| is_safe_user_agent(agent)),
        })
    }

    fn encode(&self) -> Result<String, AppError> {
        serde_json::to_string(self).map_err(|error| AppError::Parse(error.to_string()))
    }
}

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

/// Fetch Codex quota from a persisted browser session. ChatGPT's session endpoint returns a
/// short-lived bearer token; the token stays in memory and is exchanged for every poll instead
/// of being persisted beside the DPAPI-encrypted cookie.
pub async fn fetch_usage(
    client: &reqwest::Client,
    cookie_or_session: &str,
) -> Result<UsageSnapshot, AppError> {
    let session = CodexSession::decode(cookie_or_session)?;
    let credentials = fetch_session_credentials(client, &session).await?;

    let mut request = client
        .get(USAGE_URL)
        .header(reqwest::header::COOKIE, &session.cookie)
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", credentials.access_token),
        )
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::ORIGIN, CHATGPT_ORIGIN)
        .header(reqwest::header::REFERER, CHATGPT_REFERER)
        .header("OAI-Product-Sku", "codex")
        // Keep the older marker as a compatibility hint while the official Codex product SKU is
        // the authentication-relevant identifier.
        .header("OAI-App-Brand", "codex");
    if let Some(account_id) = credentials.account_id.as_deref() {
        request = request.header("ChatGPT-Account-ID", account_id);
    }
    if let Some(user_agent) = session.user_agent.as_deref() {
        request = request.header(reqwest::header::USER_AGENT, user_agent);
    }
    let resp = request.send().await?;
    ensure_success(resp.status().as_u16())?;
    let raw = resp
        .json::<Value>()
        .await
        .map_err(|e| AppError::Parse(e.to_string()))?;
    parse_usage(&raw)
}

async fn fetch_session_credentials(
    client: &reqwest::Client,
    session: &CodexSession,
) -> Result<SessionCredentials, AppError> {
    let mut request = client
        .get(SESSION_URL)
        .header(reqwest::header::COOKIE, &session.cookie)
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::ORIGIN, CHATGPT_ORIGIN)
        .header(reqwest::header::REFERER, CHATGPT_REFERER);
    if let Some(user_agent) = session.user_agent.as_deref() {
        request = request.header(reqwest::header::USER_AGENT, user_agent);
    }
    let resp = request.send().await?;
    ensure_success(resp.status().as_u16())?;
    let raw = resp
        .json::<Value>()
        .await
        .map_err(|error| AppError::Parse(error.to_string()))?;
    parse_session_credentials(&raw)
}

fn parse_session_credentials(raw: &Value) -> Result<SessionCredentials, AppError> {
    let Some(access_token) = raw
        .get("accessToken")
        .or_else(|| raw.get("access_token"))
        .and_then(Value::as_str)
    else {
        return Err(missing_access_token_error(raw));
    };
    if !is_safe_header_value(access_token) {
        return Err(AppError::Parse("Codex access token is invalid".to_string()));
    }
    let account_id = raw
        .get("accountId")
        .or_else(|| raw.get("account_id"))
        .and_then(Value::as_str)
        .filter(|id| is_safe_header_value(id))
        .map(ToString::to_string)
        .or_else(|| account_id_from_jwt(&access_token))
        .or_else(|| {
            raw.get("idToken")
                .or_else(|| raw.get("id_token"))
                .and_then(Value::as_str)
                .and_then(account_id_from_jwt)
        });
    Ok(SessionCredentials {
        access_token: access_token.to_string(),
        account_id,
    })
}

/// A signed-out ChatGPT visit can return HTTP 200 with a warning banner or an empty NextAuth
/// session. Those shapes are a rejected browser credential, not an endpoint schema change. Keep
/// every other token-less shape as `Parse` so an authenticated response change stays visible.
fn missing_access_token_error(raw: &Value) -> AppError {
    if session_explicitly_unauthenticated(raw) {
        AppError::Unauthorized
    } else {
        AppError::Parse("Codex access token is missing".to_string())
    }
}

fn session_explicitly_unauthenticated(raw: &Value) -> bool {
    let Some(session) = raw.as_object() else {
        return false;
    };
    if session.is_empty() || session.get("user").is_some_and(Value::is_null) {
        return true;
    }
    if session.contains_key("WARNING_BANNER") {
        return true;
    }
    ["warning", "error", "code"]
        .iter()
        .filter_map(|key| session.get(*key).and_then(Value::as_str))
        .any(|code| code == "WARNING_BANNER")
        || session
            .get("error")
            .and_then(|error| error.get("code"))
            .and_then(Value::as_str)
            .is_some_and(|code| code == "WARNING_BANNER")
}

fn account_id_from_jwt(access_token: &str) -> Option<String> {
    let payload = access_token.split('.').nth(1)?;
    let decoded = URL_SAFE_NO_PAD
        .decode(payload)
        .or_else(|_| URL_SAFE.decode(payload))
        .ok()?;
    let claims = serde_json::from_slice::<Value>(&decoded).ok()?;
    claims
        .get("https://api.openai.com/auth")
        .and_then(|auth| auth.get("chatgpt_account_id"))
        .or_else(|| claims.get("chatgpt_account_id"))
        .and_then(Value::as_str)
        .filter(|id| is_safe_header_value(id))
        .map(ToString::to_string)
}

fn is_safe_header_value(value: &str) -> bool {
    !value.is_empty() && !value.contains(['\r', '\n'])
}

fn is_safe_user_agent(value: &str) -> bool {
    is_safe_header_value(value) && value.len() <= 512
}

/// Invalidate only the exact browser session that produced an unauthorised poll result. A
/// background poll can finish while a new login persists another session; the shared lock keeps
/// this comparison, invalidation, and UI transition ordered with login and logout.
///
/// `true` means this poll owned the current session and transitioned it to signed out. `false`
/// means a newer login or logout won the race, so its newer UI state is left untouched.
pub fn invalidate_session_if_current(
    app: &AppHandle,
    expected_session: &str,
) -> Result<bool, AppError> {
    let _session_guard = session_lock();
    let current_session = config::load_cookie(app, crate::service::CODEX);
    if !session_matches_expected(current_session.as_deref(), expected_session) {
        return Ok(false);
    }

    let invalidation_result = config::invalidate_cookie(app, crate::service::CODEX);
    // Keep the state/event update inside the same lock. If a login starts immediately after the
    // invalidation, it will persist its verified snapshot only after this old poll has finished.
    crate::usage::mark_status(app, crate::service::CODEX, "unauthorized");
    let _ = app.emit(
        "session://changed",
        serde_json::json!({
            "service": crate::service::CODEX,
            "logged_in": false,
            "org_name": "",
        }),
    );
    invalidation_result.map(|()| true)
}

/// Apply a successful Codex poll only while the browser session that initiated it is still the
/// persisted session. The lock keeps this comparison and every `apply_snapshot` side effect
/// ordered with logout and completed login persistence.
///
/// `true` means the snapshot was applied. `false` means logout or a newer login won the race,
/// so history, notifications, tray, and frontend state remain untouched by the stale result.
pub fn apply_snapshot_if_session_current(
    app: &AppHandle,
    expected_session: &str,
    snapshot: UsageSnapshot,
) -> bool {
    let _session_guard = session_lock();
    let current_session = config::load_cookie(app, crate::service::CODEX);
    transition_if_session_matches(current_session.as_deref(), expected_session, || {
        crate::usage::apply_snapshot(app, snapshot);
    })
}

/// Mark a Codex poll failure only while the session that initiated the request remains current.
/// This uses the same critical section as successful snapshots so an old transport or parse
/// failure cannot replace a completed login or logout status with `error`.
pub fn mark_status_if_session_current(
    app: &AppHandle,
    expected_session: &str,
    status: &str,
) -> bool {
    let _session_guard = session_lock();
    let current_session = config::load_cookie(app, crate::service::CODEX);
    transition_if_session_matches(current_session.as_deref(), expected_session, || {
        crate::usage::mark_status(app, crate::service::CODEX, status);
    })
}

fn session_matches_expected(current_session: Option<&str>, expected_session: &str) -> bool {
    current_session == Some(expected_session)
}

/// Keep tests focused on the side-effect boundary while the public wrappers above own the
/// session lock and persistence read.
fn transition_if_session_matches(
    current_session: Option<&str>,
    expected_session: &str,
    transition: impl FnOnce(),
) -> bool {
    if !session_matches_expected(current_session, expected_session) {
        return false;
    }
    transition();
    true
}

/// Start a disposable, isolated Codex login webview. The helper detects an authenticated browser
/// session, then this parent process validates its returned cookie with a fresh OAuth bearer
/// request before DPAPI-backed persistence. No helper output alone can create a signed-in state.
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

        let Some(session) = result.as_deref().and_then(helper_session) else {
            if login_is_current(epoch) && !matches!(result.as_deref(), Some("CANCELLED")) {
                crate::usage::mark_status(&app, crate::service::CODEX, "error");
            }
            return;
        };
        let serialized_session = match session.encode() {
            Ok(value) => value,
            Err(_) => {
                if login_is_current(epoch) {
                    crate::usage::mark_status(&app, crate::service::CODEX, "error");
                }
                return;
            }
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
        let snapshot =
            match tauri::async_runtime::block_on(fetch_usage(&client, &serialized_session)) {
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
        if config::save_cookie(&app, crate::service::CODEX, &serialized_session).is_err() {
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
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return None;
    };
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

/// Accept only the helper's structured single-line transport. The cookie and browser user agent
/// are stored together so a Cloudflare-cleared browser session is replayed with its original UA.
/// Never log this payload.
fn helper_session(payload: &str) -> Option<CodexSession> {
    let serialized = payload.strip_prefix("COOKIE ")?;
    if serialized.contains(['\r', '\n']) {
        return None;
    }
    CodexSession::decode(serialized).ok()
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
    fn session_credentials_read_bearer_and_account_id_from_jwt() {
        let payload = URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&json!({
                "https://api.openai.com/auth": { "chatgpt_account_id": "workspace-123" }
            }))
            .expect("JWT claims serialize"),
        );
        let token = format!("header.{payload}.signature");
        let credentials = parse_session_credentials(&json!({ "accessToken": token }))
            .expect("session credentials");

        assert_eq!(credentials.account_id.as_deref(), Some("workspace-123"));
        assert!(credentials.access_token.starts_with("header."));
    }

    #[test]
    fn session_credentials_accept_snake_case_and_bearer_only_fallback() {
        let credentials = parse_session_credentials(&json!({
            "access_token": "not-a-jwt-but-valid-for-bearer"
        }))
        .expect("bearer-only session is allowed");

        assert_eq!(credentials.access_token, "not-a-jwt-but-valid-for-bearer");
        assert!(credentials.account_id.is_none());
    }

    #[test]
    fn session_credentials_fall_back_to_id_token_account_claim() {
        let payload = URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&json!({
                "https://api.openai.com/auth": { "chatgpt_account_id": "workspace-from-id-token" }
            }))
            .expect("JWT claims serialize"),
        );
        let credentials = parse_session_credentials(&json!({
            "accessToken": "opaque-access-token",
            "idToken": format!("header.{payload}.signature")
        }))
        .expect("session credentials");

        assert_eq!(
            credentials.account_id.as_deref(),
            Some("workspace-from-id-token")
        );
    }

    #[test]
    fn explicitly_signed_out_session_shapes_are_unauthorized() {
        for session in [
            json!({}),
            json!({ "user": null, "expires": "2026-08-20T00:00:00.000Z" }),
            json!({ "WARNING_BANNER": "Sign in required" }),
            json!({ "error": { "code": "WARNING_BANNER" } }),
        ] {
            assert!(matches!(
                parse_session_credentials(&session),
                Err(AppError::Unauthorized)
            ));
        }
    }

    #[test]
    fn malformed_authenticated_session_shapes_remain_parse_errors() {
        for session in [
            json!({ "accessToken": 42 }),
            json!({ "accessToken": "" }),
            json!({ "user": { "id": "user-123" } }),
        ] {
            assert!(matches!(
                parse_session_credentials(&session),
                Err(AppError::Parse(_))
            ));
        }
    }

    #[test]
    fn session_credentials_do_not_use_user_account_id_as_workspace_header() {
        let credentials = parse_session_credentials(&json!({
            "accessToken": "opaque-access-token",
            "account": { "id": "user-account-id" }
        }))
        .expect("bearer-only session is allowed");

        assert!(credentials.account_id.is_none());
    }

    #[test]
    fn poll_snapshot_matches_the_session_that_started_it() {
        let old_session = r#"{"cookie":"old","user_agent":"UA"}"#;
        let mut applied = false;

        assert!(transition_if_session_matches(
            Some(old_session),
            old_session,
            || {
                applied = true;
            }
        ));
        assert!(applied);
    }

    #[test]
    fn stale_poll_snapshot_is_skipped_after_logout() {
        let old_session = r#"{"cookie":"old","user_agent":"UA"}"#;

        // Logout removes the persisted credential while this request is in flight.
        assert!(!session_matches_expected(None, old_session));
    }

    #[test]
    fn stale_poll_snapshot_is_skipped_after_new_login() {
        let old_session = r#"{"cookie":"old","user_agent":"UA"}"#;
        let new_session = r#"{"cookie":"new","user_agent":"UA"}"#;

        // The completed replacement login owns the persisted credential before this poll returns.
        assert!(!session_matches_expected(Some(new_session), old_session));
    }

    #[test]
    fn stale_poll_error_is_skipped_after_logout() {
        let old_session = r#"{"cookie":"old","user_agent":"UA"}"#;
        let mut marked_error = false;

        assert!(!transition_if_session_matches(None, old_session, || {
            marked_error = true;
        }));
        assert!(!marked_error);
    }

    #[test]
    fn stale_poll_error_is_skipped_after_new_login() {
        let old_session = r#"{"cookie":"old","user_agent":"UA"}"#;
        let new_session = r#"{"cookie":"new","user_agent":"UA"}"#;
        let mut marked_error = false;

        assert!(!transition_if_session_matches(
            Some(new_session),
            old_session,
            || {
                marked_error = true;
            },
        ));
        assert!(!marked_error);
    }

    #[test]
    fn stored_session_preserves_browser_user_agent() {
        let stored = CodexSession {
            cookie: "session=value".to_string(),
            user_agent: Some("Mozilla/5.0 Test WebView2".to_string()),
        }
        .encode()
        .expect("stored session");
        let decoded = CodexSession::decode(&stored).expect("decode stored session");

        assert_eq!(decoded.cookie, "session=value");
        assert_eq!(
            decoded.user_agent.as_deref(),
            Some("Mozilla/5.0 Test WebView2")
        );
    }

    #[test]
    fn legacy_raw_cookie_session_remains_readable() {
        let decoded =
            CodexSession::decode("session=value; cf_clearance=abc").expect("legacy cookie session");

        assert_eq!(decoded.cookie, "session=value; cf_clearance=abc");
        assert!(decoded.user_agent.is_none());
    }

    #[test]
    fn accepts_only_structured_single_line_helper_cookie_transport() {
        let session =
            helper_session(r#"COOKIE {"cookie":"session=value","user_agent":"Browser UA"}"#)
                .expect("valid helper session");
        assert_eq!(session.cookie, "session=value");
        assert_eq!(session.user_agent.as_deref(), Some("Browser UA"));
        assert!(helper_session("COOKIE ").is_none());
        assert!(helper_session("COOKIE {\"cookie\":\"session=value\\nnext\"}").is_none());
        assert!(helper_session("CANCELLED").is_none());
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
