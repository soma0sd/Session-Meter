//! Local usage history: one JSONL line per poll, in the app data dir. Used by the stats
//! window for trend charts and the depletion forecast. Retention-bounded to avoid growth.

use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use time::{format_description::well_known::Rfc3339, Duration, OffsetDateTime};

use crate::api::{self, UsageSnapshot};
use crate::state::AppState;

const FILE: &str = "history.jsonl";

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HistoryPoint {
    pub at: String,
    pub five_hour: Option<u8>,
    pub weekly: Option<u8>,
}

fn history_path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join(FILE))
}

pub fn parse_iso(s: &str) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(s, &Rfc3339).ok()
}

fn retention_days(app: &AppHandle) -> i64 {
    app.try_state::<AppState>()
        .map(|s| s.settings.lock().unwrap().history_retention_days as i64)
        .unwrap_or(30)
}

/// Append the current snapshot as one history point, then prune anything older than the
/// retention window (rewriting only when something is actually dropped).
pub fn record(app: &AppHandle, snapshot: &UsageSnapshot) {
    let point = HistoryPoint {
        at: api::now_iso(),
        five_hour: snapshot.five_hour.as_ref().map(|w| w.remaining),
        weekly: snapshot.weekly_primary.as_ref().map(|w| w.remaining),
    };
    let Some(path) = history_path(app) else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(line) = serde_json::to_string(&point) {
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            let _ = writeln!(f, "{line}");
        }
    }
    prune(app, &path);
}

fn prune(app: &AppHandle, path: &PathBuf) {
    let cutoff = OffsetDateTime::now_utc() - Duration::days(retention_days(app));
    let Ok(content) = std::fs::read_to_string(path) else {
        return;
    };
    let total = content.lines().count();
    let kept: Vec<&str> = content
        .lines()
        .filter(|l| {
            serde_json::from_str::<HistoryPoint>(l)
                .ok()
                .and_then(|p| parse_iso(&p.at))
                .map(|t| t >= cutoff)
                .unwrap_or(false)
        })
        .collect();
    if kept.len() != total {
        let _ = std::fs::write(path, format!("{}\n", kept.join("\n")));
    }
}

/// Load history within the retention window.
pub fn load(app: &AppHandle) -> Vec<HistoryPoint> {
    let Some(path) = history_path(app) else {
        return Vec::new();
    };
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let cutoff = OffsetDateTime::now_utc() - Duration::days(retention_days(app));
    content
        .lines()
        .filter_map(|l| serde_json::from_str::<HistoryPoint>(l).ok())
        .filter(|p| parse_iso(&p.at).map(|t| t >= cutoff).unwrap_or(false))
        .collect()
}
