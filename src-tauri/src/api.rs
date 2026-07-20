//! claude.ai private web API client.
//!
//! Data comes from claude.ai (the same numbers its usage page shows), authenticated
//! with the browser session cookie. There is no official/public API for subscription
//! session limits, so this mirrors what the web app itself calls:
//!   GET /api/organizations            -> take orgs[0].uuid
//!   GET /api/organizations/{uuid}/usage
//! The usage response returns per-window `utilization` (percent used) + `resets_at`.
//! We parse every `{utilization, resets_at}` bucket dynamically so extra limits
//! (e.g. a model-group weekly cap) show up automatically.

use serde::Serialize;
use serde_json::Value;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::error::AppError;

const BASE: &str = "https://claude.ai/api";
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36";

#[derive(Serialize, Clone, Debug)]
pub struct WindowUsage {
    pub remaining: u8,
    pub utilization: u8,
    pub resets_at: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct Bucket {
    pub key: String,
    pub label: String,
    pub remaining: u8,
    pub utilization: u8,
    pub resets_at: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct UsageSnapshot {
    /// Which service this snapshot describes ("claude" | "antigravity").
    pub service_id: String,
    pub five_hour: Option<WindowUsage>,
    pub weekly_primary: Option<WindowUsage>,
    /// Bucket keys of the two headline windows the widget renders as primary/secondary.
    /// (For Claude: "five_hour" / "seven_day".) Every service fills `five_hour`/
    /// `weekly_primary` above with these two windows so the shared UI stays uniform.
    pub primary_key: Option<String>,
    pub secondary_key: Option<String>,
    pub buckets: Vec<Bucket>,
    pub organization_name: String,
    pub account_email: String,
    /// Friendly subscription/plan label (e.g. "Claude Max 20x"), derived from the org's
    /// rate-limit tier. Empty when unknown.
    pub subscription: String,
    pub fetched_at: String,
    pub status: String,
}

pub fn now_iso() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_default()
}

pub fn build_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(UA)
        .gzip(true)
        // Bound every request so a stalled, hung, or hostile peer cannot freeze the
        // poll loop indefinitely (reqwest has no default timeout).
        .timeout(std::time::Duration::from_secs(30))
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

/// GET a JSON endpoint under `/api`, mapping auth failures to `Unauthorized`.
pub async fn get_json(
    client: &reqwest::Client,
    cookie: &str,
    path: &str,
) -> Result<Value, AppError> {
    let resp = client
        .get(format!("{BASE}{path}"))
        .header(reqwest::header::COOKIE, cookie)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await?;
    let code = resp.status().as_u16();
    if code == 401 || code == 403 {
        return Err(AppError::Unauthorized);
    }
    if !resp.status().is_success() {
        return Err(AppError::Http(format!("HTTP {code}")));
    }
    resp.json::<Value>()
        .await
        .map_err(|e| AppError::Parse(e.to_string()))
}

fn humanize(key: &str) -> String {
    match key {
        "five_hour" => "Current session".to_string(),
        "seven_day" => "Weekly session".to_string(),
        other => {
            let mut s = other.replace('_', " ");
            if let Some(c) = s.get_mut(0..1) {
                c.make_ascii_uppercase();
            }
            s
        }
    }
}

fn bucket_order(key: &str) -> u8 {
    match key {
        "five_hour" => 0,
        "seven_day" => 1,
        _ => 2,
    }
}

fn bucket_from(key: &str, val: &Value) -> Option<Bucket> {
    let obj = val.as_object()?;
    let util = obj.get("utilization")?.as_f64()?;
    let resets_at = obj
        .get("resets_at")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let utilization = util.round().clamp(0.0, 100.0) as u8;
    let remaining = 100u8.saturating_sub(utilization);
    Some(Bucket {
        key: key.to_string(),
        label: humanize(key),
        remaining,
        utilization,
        resets_at,
    })
}

fn to_window(b: &Bucket) -> WindowUsage {
    WindowUsage {
        remaining: b.remaining,
        utilization: b.utilization,
        resets_at: b.resets_at.clone(),
    }
}

pub fn parse_usage(raw: &Value, org_fallback: &str) -> UsageSnapshot {
    let mut buckets: Vec<Bucket> = Vec::new();
    if let Some(obj) = raw.as_object() {
        for (key, val) in obj {
            if let Some(b) = bucket_from(key, val) {
                buckets.push(b);
            }
        }
    }
    buckets.sort_by_key(|b| bucket_order(&b.key));

    let five_hour = buckets.iter().find(|b| b.key == "five_hour").map(to_window);
    let weekly_primary = buckets
        .iter()
        .find(|b| b.key == "seven_day")
        .or_else(|| {
            buckets
                .iter()
                .find(|b| b.key.contains("seven") || b.key.contains("week"))
        })
        .map(to_window);

    let organization_name = raw
        .get("organization_name")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| org_fallback.to_string());

    UsageSnapshot {
        service_id: crate::service::CLAUDE.to_string(),
        five_hour,
        weekly_primary,
        primary_key: Some("five_hour".to_string()),
        secondary_key: Some("seven_day".to_string()),
        buckets,
        organization_name,
        account_email: String::new(),
        subscription: String::new(),
        fetched_at: now_iso(),
        status: "ok".to_string(),
    }
}

/// Derive a friendly subscription label from the organization object. claude.ai exposes
/// `rate_limit_tier` like "default_claude_max_20x" / "default_claude_pro"; fall back to the
/// `capabilities` array ("claude_max" / "claude_pro") when the tier is absent.
fn subscription_label(org: &Value) -> String {
    let tier = org
        .get("rate_limit_tier")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let t = tier.strip_prefix("default_").unwrap_or(tier);
    // A bare "default" (or empty) tier carries no plan info -> fall through to capabilities.
    if !t.is_empty() && !t.eq_ignore_ascii_case("default") {
        let mut out = String::new();
        for part in t.split('_') {
            if !out.is_empty() {
                out.push(' ');
            }
            if part.eq_ignore_ascii_case("claude") {
                out.push_str("Claude");
            } else if part.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                out.push_str(part); // "20x", "5x"
            } else {
                let mut ch = part.chars();
                if let Some(f) = ch.next() {
                    out.extend(f.to_uppercase());
                    out.push_str(ch.as_str());
                }
            }
        }
        return out;
    }
    let caps = org.get("capabilities").and_then(|v| v.as_array());
    let has = |name: &str| {
        caps.map(|a| a.iter().any(|c| c.as_str() == Some(name)))
            .unwrap_or(false)
    };
    if has("claude_max") {
        "Claude Max".to_string()
    } else if has("claude_pro") {
        "Claude Pro".to_string()
    } else if has("claude_team") {
        "Claude Team".to_string()
    } else {
        String::new()
    }
}

fn strip_org_suffix(name: &str) -> String {
    name.strip_suffix("'s Organization")
        .unwrap_or(name)
        .trim()
        .to_string()
}

/// Extract a human display name from an account-like JSON value, checking both the root
/// and a nested `account` object, then falling back to the email local-part.
fn extract_display_name(v: &Value) -> Option<String> {
    let null = Value::Null;
    let objs = [v, v.get("account").unwrap_or(&null)];
    for obj in objs {
        for key in ["full_name", "display_name", "name"] {
            if let Some(s) = obj.get(key).and_then(|x| x.as_str()) {
                let s = s.trim();
                if !s.is_empty() {
                    return Some(s.to_string());
                }
            }
        }
    }
    for obj in objs {
        if let Some(email) = obj
            .get("email_address")
            .or_else(|| obj.get("email"))
            .and_then(|x| x.as_str())
        {
            if let Some(local) = email.split('@').next() {
                if !local.is_empty() {
                    return Some(local.to_string());
                }
            }
        }
    }
    None
}

/// Extract the full account email from an account-like JSON value (root or nested `account`).
fn extract_email(v: &Value) -> Option<String> {
    let null = Value::Null;
    for obj in [v, v.get("account").unwrap_or(&null)] {
        if let Some(email) = obj
            .get("email_address")
            .or_else(|| obj.get("email"))
            .and_then(|x| x.as_str())
        {
            let email = email.trim();
            if !email.is_empty() {
                return Some(email.to_string());
            }
        }
    }
    None
}

/// Full fetch: resolve the first organization, then its usage, then a display name.
pub async fn fetch_usage(
    client: &reqwest::Client,
    cookie: &str,
) -> Result<UsageSnapshot, AppError> {
    let orgs = get_json(client, cookie, "/organizations").await?;
    let org = orgs
        .as_array()
        .and_then(|a| a.first())
        .ok_or_else(|| AppError::Other("no organizations".to_string()))?;
    let uuid = org
        .get("uuid")
        .and_then(|u| u.as_str())
        .ok_or_else(|| AppError::Parse("organization uuid missing".to_string()))?
        .to_string();
    let org_name = org
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or("")
        .to_string();
    let raw = get_json(client, cookie, &format!("/organizations/{uuid}/usage")).await?;
    let mut snapshot = parse_usage(&raw, &org_name);
    snapshot.subscription = subscription_label(org);

    // Prefer the account's real display name + email from /account; else fall back to the
    // org name without the "'s Organization" suffix. One fetch supplies both fields.
    match get_json(client, cookie, "/account").await {
        Ok(acc) => {
            snapshot.organization_name = extract_display_name(&acc)
                .unwrap_or_else(|| strip_org_suffix(&snapshot.organization_name));
            snapshot.account_email = extract_email(&acc).unwrap_or_default();
        }
        Err(_) => snapshot.organization_name = strip_org_suffix(&snapshot.organization_name),
    }
    Ok(snapshot)
}
