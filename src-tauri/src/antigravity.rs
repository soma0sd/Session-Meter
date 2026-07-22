//! Antigravity usage provider - a local process, not a remote account. Antigravity IDE (a
//! Windsurf/Codeium-based VS Code fork) exposes its own coding-quota numbers through its
//! bundled `language_server.exe`'s local Connect-RPC API. There is no login and no opt-in
//! step: `service::has_session` always reports this service as "on" (see its doc comment), so
//! its widget appears automatically the same way Claude's does before sign-in, and every poll
//! simply tries to reach the IDE fresh - reachable is `status: "ok"`, not-running-right-now is
//! `AppError::NotRunning` / `status: "not_running"` (see `poller.rs`).
//!
//! Protocol (reverse-engineered + cross-checked against CodexBar/openusage/antigravity-usage;
//! see the project's local memory notes for the source list): POST
//! `/exa.language_server_pb.LanguageServerService/RetrieveUserQuotaSummary` with headers
//! `Content-Type: application/json`, `Connect-Protocol-Version: 1`,
//! `X-Codeium-Csrf-Token: <token>` (that exact header name - `x-csrf-token` is a confirmed
//! 401), body `{"forceRefresh": true}`. The process listens on two loopback ports (one HTTPS
//! self-signed, one plain HTTP); which port is which is discovered by probing, not assumed.
//! Both the CSRF token and the ports are re-discovered on every call and never cached: the
//! IDE reassigns both whenever it restarts.

use std::sync::OnceLock;
use std::time::Duration;

use serde_json::Value;
use tauri::AppHandle;

use crate::api::{now_iso, Bucket, UsageSnapshot, WindowUsage};
use crate::error::AppError;

const ENDPOINT_PATH: &str =
    "/exa.language_server_pb.LanguageServerService/RetrieveUserQuotaSummary";
/// Account identity (email/plan) - best-effort only: unlike `ENDPOINT_PATH`, this endpoint's
/// exact request/response shape was not independently re-verified with the same rigor, so
/// `parse_user_status` treats every field as optional and a failure here never blocks the
/// quota fetch (see `discover_and_fetch`).
const USER_STATUS_PATH: &str = "/exa.language_server_pb.LanguageServerService/GetUserStatus";

#[derive(Clone, Copy)]
enum Scheme {
    Https,
    Http,
}

struct Candidate {
    token: String,
    ports: Vec<u16>,
}

// --- process/port discovery (Windows-only for now; see the module doc) ---

#[cfg(windows)]
fn discover_candidates() -> Result<Vec<Candidate>, AppError> {
    use std::io::Read;
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    // Windows CREATE_NO_WINDOW: suppress the console flash from spawning powershell headless.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    // One PowerShell round trip does all three steps: find language_server.exe processes,
    // pull the --csrf_token argument out of each one's command line, and list the LISTEN
    // ports owned by that PID. `@( ... )` + `ConvertTo-Json -InputObject` (not piped) is
    // required so a single match still serializes as a JSON array instead of a bare object.
    const SCRIPT: &str = r#"
$out = @(
  Get-CimInstance Win32_Process -Filter "Name='language_server.exe'" |
  ForEach-Object {
    if ($_.CommandLine -match '--csrf_token\s+"?([0-9a-fA-F-]{36})"?') {
      $ports = @(Get-NetTCPConnection -OwningProcess $_.ProcessId -State Listen -ErrorAction SilentlyContinue |
                  Select-Object -ExpandProperty LocalPort)
      [PSCustomObject]@{ Token = $Matches[1]; Ports = $ports }
    }
  }
)
ConvertTo-Json -InputObject $out -Compress -Depth 4
"#;

    let mut child = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| AppError::Other(format!("powershell spawn failed: {e}")))?;

    // Bounded wait (same shape as gemini.rs's helper reader): a wedged/slow powershell.exe
    // must never stall the poll loop indefinitely.
    let (tx, rx) = std::sync::mpsc::channel();
    if let Some(mut out) = child.stdout.take() {
        std::thread::spawn(move || {
            let mut buf = String::new();
            let _ = out.read_to_string(&mut buf);
            let _ = tx.send(buf);
        });
    }
    let output = rx.recv_timeout(Duration::from_secs(5)).ok();
    let _ = child.kill();
    let _ = child.wait();

    let Some(raw) = output.filter(|s| !s.trim().is_empty()) else {
        return Ok(Vec::new()); // no language_server.exe match, or powershell timed out
    };
    let parsed: Value = serde_json::from_str(&raw).map_err(|e| AppError::Parse(e.to_string()))?;
    // A single match can still serialize as a bare object despite the @()+ConvertTo-Json
    // guard above (observed on some PowerShell builds) - accept both shapes defensively.
    let items: Vec<Value> = match parsed {
        Value::Array(a) => a,
        obj @ Value::Object(_) => vec![obj],
        _ => Vec::new(),
    };
    Ok(items
        .into_iter()
        .filter_map(|v| {
            let token = v.get("Token")?.as_str()?.to_string();
            let ports: Vec<u16> = v
                .get("Ports")?
                .as_array()?
                .iter()
                .filter_map(Value::as_u64)
                .map(|p| p as u16)
                .collect();
            if ports.is_empty() {
                None
            } else {
                Some(Candidate { token, ports })
            }
        })
        .collect())
}

#[cfg(not(windows))]
fn discover_candidates() -> Result<Vec<Candidate>, AppError> {
    Err(AppError::Other(
        "Antigravity IDE detection is Windows-only".to_string(),
    ))
}

// --- HTTP probe ---

/// A dedicated client for this module's loopback-only calls, never shared with the app-wide
/// `AppState.client` (which talks to claude.ai/Google and must keep full certificate
/// validation). Antigravity's local HTTPS port uses a self-signed certificate;
/// `danger_accept_invalid_certs` is scoped to exactly this client, whose requests only ever
/// target a `127.0.0.1:<port>` URL this module builds itself - never a user- or
/// remote-supplied one - so the relaxed verification cannot be abused from outside.
fn local_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(Duration::from_secs(3))
            .connect_timeout(Duration::from_secs(2))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new())
    })
}

async fn post_json(
    scheme: Scheme,
    port: u16,
    token: &str,
    path: &str,
    body: &'static str,
) -> Result<Value, AppError> {
    let scheme_str = match scheme {
        Scheme::Https => "https",
        Scheme::Http => "http",
    };
    let url = format!("{scheme_str}://127.0.0.1:{port}{path}");
    let resp = local_client()
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Connect-Protocol-Version", "1")
        .header("X-Codeium-Csrf-Token", token)
        .body(body)
        .send()
        .await
        .map_err(|e| AppError::Http(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(AppError::Http(format!("HTTP {}", resp.status().as_u16())));
    }
    resp.json::<Value>()
        .await
        .map_err(|e| AppError::Parse(e.to_string()))
}

async fn try_quota_request(scheme: Scheme, port: u16, token: &str) -> Result<Value, AppError> {
    post_json(scheme, port, token, ENDPOINT_PATH, r#"{"forceRefresh": true}"#).await
}

async fn try_user_status_request(scheme: Scheme, port: u16, token: &str) -> Result<Value, AppError> {
    post_json(
        scheme,
        port,
        token,
        USER_STATUS_PATH,
        r#"{"metadata":{"ideName":"antigravity"}}"#,
    )
    .await
}

/// Try every discovered (port, scheme) combination until one answers. Each port only speaks
/// one scheme in practice, but which is which is never assumed - HTTPS is tried first,
/// per-port, falling back to HTTP. Returns which (scheme, port, token) worked alongside the
/// response, so a follow-up call (account identity) can reuse it instead of re-probing.
async fn probe_and_fetch(candidates: &[Candidate]) -> Result<(Value, Scheme, u16, String), AppError> {
    for c in candidates {
        for &port in &c.ports {
            for scheme in [Scheme::Https, Scheme::Http] {
                if let Ok(v) = try_quota_request(scheme, port, &c.token).await {
                    return Ok((v, scheme, port, c.token.clone()));
                }
            }
        }
    }
    Err(AppError::NotRunning)
}

// --- response parsing ---

fn groups_of(raw: &Value) -> Vec<&Value> {
    let root = raw
        .get("response")
        .or_else(|| raw.get("summary"))
        .unwrap_or(raw);
    root.get("groups")
        .and_then(Value::as_array)
        .map(|a| a.iter().collect())
        .unwrap_or_default()
}

fn remaining_fraction(bucket: &Value) -> Option<f64> {
    bucket
        .get("remainingFraction")
        .and_then(Value::as_f64)
        .or_else(|| {
            bucket
                .pointer("/remaining/remainingFraction")
                .and_then(Value::as_f64)
        })
}

fn parse_reset_time(bucket: &Value) -> String {
    match bucket.get("resetTime") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n
            .as_i64()
            .and_then(|secs| time::OffsetDateTime::from_unix_timestamp(secs).ok())
            .and_then(|t| t.format(&time::format_description::well_known::Rfc3339).ok())
            .unwrap_or_default(),
        _ => String::new(),
    }
}

/// Fixed sort order so the widget/tray/history always see the same bucket in the same slot
/// regardless of what order the API happens to return groups/buckets in. An unrecognized
/// bucket id (a future API change) sorts last instead of being dropped or panicking.
fn bucket_sort_key(key: &str) -> u8 {
    match key {
        "gemini-5h" => 0,
        "gemini-weekly" => 1,
        "3p-5h" => 2,
        "3p-weekly" => 3,
        _ => 99,
    }
}

fn humanize(key: &str) -> String {
    match key {
        "gemini-5h" => "Gemini 5-hour".to_string(),
        "gemini-weekly" => "Gemini weekly".to_string(),
        "3p-5h" => "Claude/GPT 5-hour".to_string(),
        "3p-weekly" => "Claude/GPT weekly".to_string(),
        other => other.replace('-', " "),
    }
}

fn to_window(b: &Bucket) -> WindowUsage {
    WindowUsage {
        remaining: b.remaining,
        utilization: b.utilization,
        resets_at: b.resets_at.clone(),
    }
}

/// Parse the raw quota response into a snapshot carrying all four buckets. The headline pair
/// (`five_hour`/`weekly_primary`) is always the Gemini group: history records one fixed metric
/// per service, and letting the headline follow `WidgetConfig.headline_group` would mix two
/// differently-scoped series into the same history file depending on when the user toggled
/// it. The 3p group - and every bucket - stays fully visible via `buckets` (the Stats window
/// already renders every bucket generically) and via the widget's own primary/secondary
/// override (see `WidgetStyle.svelte`), which is a pure presentation-layer choice.
fn parse_snapshot(raw: &Value) -> Result<UsageSnapshot, AppError> {
    let mut buckets: Vec<Bucket> = Vec::new();
    for group in groups_of(raw) {
        let Some(items) = group.get("buckets").and_then(Value::as_array) else {
            continue;
        };
        for b in items {
            let Some(key) = b.get("bucketId").and_then(Value::as_str) else {
                continue;
            };
            let Some(frac) = remaining_fraction(b) else {
                continue;
            };
            let remaining = (frac.clamp(0.0, 1.0) * 100.0).round() as u8;
            let utilization = 100u8.saturating_sub(remaining);
            let label = b
                .get("displayName")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| humanize(key));
            buckets.push(Bucket {
                key: key.to_string(),
                label,
                remaining,
                utilization,
                resets_at: parse_reset_time(b),
            });
        }
    }
    if buckets.is_empty() {
        return Err(AppError::Parse("no usage buckets in response".to_string()));
    }
    buckets.sort_by_key(|b| bucket_sort_key(&b.key));

    let five_hour = buckets.iter().find(|b| b.key == "gemini-5h").map(to_window);
    let weekly_primary = buckets
        .iter()
        .find(|b| b.key == "gemini-weekly")
        .map(to_window);

    Ok(UsageSnapshot {
        service_id: crate::service::ANTIGRAVITY_IDE.to_string(),
        five_hour,
        weekly_primary,
        primary_key: Some("gemini-5h".to_string()),
        secondary_key: Some("gemini-weekly".to_string()),
        buckets,
        organization_name: String::new(),
        account_email: String::new(),
        subscription: String::new(),
        fetched_at: now_iso(),
        status: "ok".to_string(),
    })
}

/// Best-effort account identity from `GetUserStatus`. Every field access is lenient (see
/// `USER_STATUS_PATH`'s doc comment) - a schema miss just yields empty strings, never an error.
fn parse_user_status(raw: &Value) -> (String, String) {
    let email = raw
        .pointer("/userStatus/email")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let plan = raw
        .pointer("/planStatus/planInfo/planName")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    (email, plan)
}

// --- entry points ---

async fn discover_and_fetch() -> Result<UsageSnapshot, AppError> {
    let candidates = tauri::async_runtime::spawn_blocking(discover_candidates)
        .await
        .map_err(|e| AppError::Other(e.to_string()))??;
    if candidates.is_empty() {
        return Err(AppError::NotRunning);
    }
    let (raw, scheme, port, token) = probe_and_fetch(&candidates).await?;
    let mut snapshot = parse_snapshot(&raw)?;
    // The quota numbers are the point of this service; a signed-in account is a nice-to-have
    // for the Settings row, so a failure here (wrong guess at the endpoint/schema, or this IDE
    // version just not exposing it) is silently swallowed rather than failing the whole poll.
    if let Ok(status_raw) = try_user_status_request(scheme, port, &token).await {
        let (email, plan) = parse_user_status(&status_raw);
        snapshot.account_email = email;
        snapshot.subscription = plan;
    }
    Ok(snapshot)
}

/// `service::fetch`'s dispatch target. No opt-in gate: every poll tries the IDE fresh, and
/// "not running right now" is a normal, expected outcome (`AppError::NotRunning`), not an error
/// state that needs a stored credential to distinguish from "never signed in".
pub async fn fetch(_app: &AppHandle, _client: &reqwest::Client) -> Result<UsageSnapshot, AppError> {
    discover_and_fetch().await
}
