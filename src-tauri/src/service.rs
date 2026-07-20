//! Service registry: the usage providers this app monitors (Claude, Antigravity). Each
//! service has a stable id used to key session credentials, cached snapshots, history, and
//! (later) its own widget window. New call sites pass a service id; older single-service
//! callers (and frontend calls that omit the argument) default to Claude via `normalize`.

use tauri::AppHandle;

use crate::api::UsageSnapshot;
use crate::error::AppError;

pub const CLAUDE: &str = "claude";
pub const ANTIGRAVITY: &str = "antigravity";

/// All known service ids, in display order.
pub fn all() -> &'static [&'static str] {
    &[CLAUDE, ANTIGRAVITY]
}

/// Human-facing service name (brand names, not localized).
pub fn display_name(id: &str) -> &'static str {
    match id {
        CLAUDE => "Claude",
        ANTIGRAVITY => "Antigravity",
        _ => "Unknown",
    }
}

/// Normalize an optional/foreign service id to a known one. Defaults to Claude so existing
/// single-service call sites (and frontend calls that omit the argument) keep working.
pub fn normalize(id: Option<&str>) -> String {
    match id {
        Some(ANTIGRAVITY) => ANTIGRAVITY.to_string(),
        _ => CLAUDE.to_string(),
    }
}

/// True if the service currently has a stored credential (is "logged in").
pub fn has_session(app: &AppHandle, service: &str) -> bool {
    match crate::config::load_cookie(app, service) {
        // Antigravity now stores a Google cookie header, not the old OAuth-token JSON; treat
        // a leftover OAuth blob (starts with '{') as signed-out so a fresh cookie login runs.
        Some(v) => !(service == ANTIGRAVITY && v.trim_start().starts_with('{')),
        None => false,
    }
}

/// The services that currently have a stored credential, in display order.
pub fn logged_in(app: &AppHandle) -> Vec<String> {
    all()
        .iter()
        .filter(|id| has_session(app, id))
        .map(|s| s.to_string())
        .collect()
}

/// Fetch a fresh usage snapshot for one service using its stored credential. The returned
/// snapshot carries its own `service_id` so downstream state/history/events stay keyed.
pub async fn fetch(
    app: &AppHandle,
    service: &str,
    client: &reqwest::Client,
) -> Result<UsageSnapshot, AppError> {
    match service {
        CLAUDE => {
            let cookie = crate::config::load_cookie(app, CLAUDE).ok_or(AppError::NoSession)?;
            // `parse_usage` stamps service_id/primary_key/secondary_key for Claude.
            crate::api::fetch_usage(client, &cookie).await
        }
        ANTIGRAVITY => crate::antigravity::fetch(app, client).await,
        other => Err(AppError::Other(format!("unknown service: {other}"))),
    }
}
