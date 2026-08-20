//! ChatGPT Codex subscription-quota provider.
//!
//! Codex subscription limits are not exposed through the public OpenAI API. The Codex
//! desktop client currently reads the authenticated ChatGPT web endpoint below. Keep this
//! adapter deliberately small and defensive: SessionMeter intentionally exposes only the
//! seven-day Codex subscription window. The endpoint's `primary_window` and
//! `secondary_window` names describe roles, not durations, so their
//! `limit_window_seconds` value determines which response window is eligible.

use serde_json::{Map, Value};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::api::{Bucket, UsageSnapshot, WindowUsage};
use crate::error::AppError;

const USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
const WEEKLY_WINDOW_SECONDS: u64 = 7 * 24 * 60 * 60;

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
            window
                .get("limit_window_seconds")
                .and_then(Value::as_u64)
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
}
