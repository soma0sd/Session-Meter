//! Local usage history: one JSONL line per poll, in the app data dir. Used by the stats
//! window for trend charts and the depletion forecast. Retention-bounded to avoid growth.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use time::{format_description::well_known::Rfc3339, Duration, OffsetDateTime};

use crate::api::{self, UsageSnapshot};
use crate::state::AppState;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HistoryPoint {
    pub at: String,
    /// Remaining% of the service's primary window. The legacy name is not a duration claim.
    pub five_hour: Option<u8>,
    /// Remaining% of the service's secondary window when available.
    pub weekly: Option<u8>,
}

// Claude keeps the legacy `history.jsonl` filename so existing history is preserved across
// the multi-service upgrade; other services get their own per-service file.
fn history_file(service: &str) -> String {
    if service == "claude" {
        "history.jsonl".to_string()
    } else {
        format!("history.{service}.jsonl")
    }
}

fn history_path(app: &AppHandle, service: &str) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|d| d.join(history_file(service)))
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
    let Some(path) = history_path(app, &snapshot.service_id) else {
        return;
    };
    // Codex reported only its weekly quota until the plan-dependent 5-hour session window
    // came back, so past weekly points were recorded in the primary slot. Move them to the
    // secondary slot once the session window shows up, keeping the weekly trend one series.
    if snapshot.service_id == crate::service::CODEX && snapshot.weekly_primary.is_some() {
        move_primary_series_to_secondary(&path);
    }
    let point = HistoryPoint {
        at: api::now_iso(),
        five_hour: snapshot.five_hour.as_ref().map(|w| w.remaining),
        weekly: snapshot.weekly_primary.as_ref().map(|w| w.remaining),
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

/// Rewrite legacy points that carry their only value in the primary slot so it sits in the
/// secondary slot instead. Used when a service gains a shorter headline window: without it the
/// same quota would appear as two disconnected series in the stats charts. A no-op once every
/// point has been moved, so calling it on each poll costs one read of an already-migrated file.
fn move_primary_series_to_secondary(path: &Path) {
    let Ok(content) = std::fs::read_to_string(path) else {
        return;
    };
    if !content.contains("\"weekly\":null") {
        return;
    }
    let mut changed = false;
    let mut lines: Vec<String> = Vec::new();
    for line in content.lines() {
        let moved = match serde_json::from_str::<HistoryPoint>(line) {
            Ok(point) if point.weekly.is_none() && point.five_hour.is_some() => HistoryPoint {
                at: point.at,
                five_hour: None,
                weekly: point.five_hour,
            },
            // Keep unparsable or already-migrated lines exactly as they are.
            _ => {
                lines.push(line.to_string());
                continue;
            }
        };
        match serde_json::to_string(&moved) {
            Ok(json) => {
                changed = true;
                lines.push(json);
            }
            Err(_) => lines.push(line.to_string()),
        }
    }
    if changed {
        let _ = std::fs::write(path, format!("{}\n", lines.join("\n")));
    }
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

/// Load a service's history within the retention window.
pub fn load(app: &AppHandle, service: &str) -> Vec<HistoryPoint> {
    let Some(path) = history_path(app, service) else {
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

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    #[test]
    fn history_files_stay_service_scoped() {
        assert_eq!(super::history_file(crate::service::CLAUDE), "history.jsonl");
        assert_eq!(
            super::history_file(crate::service::CODEX),
            "history.codex.jsonl"
        );
    }

    #[test]
    fn migration_moves_legacy_primary_points_into_the_secondary_slot() {
        let path = std::env::temp_dir().join("sessionmeter-history-migration-test.jsonl");
        let mut file = std::fs::File::create(&path).expect("temp history file");
        writeln!(file, "{{\"at\":\"2026-08-20T00:00:00Z\",\"five_hour\":80,\"weekly\":null}}").unwrap();
        writeln!(file, "not json").unwrap();
        writeln!(file, "{{\"at\":\"2026-08-26T00:00:00Z\",\"five_hour\":40,\"weekly\":70}}").unwrap();
        drop(file);

        super::move_primary_series_to_secondary(&path);
        let migrated = std::fs::read_to_string(&path).expect("migrated history");
        let points: Vec<super::HistoryPoint> = migrated
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();

        assert_eq!(points.len(), 2);
        assert_eq!(points[0].five_hour, None);
        assert_eq!(points[0].weekly, Some(80));
        // An already-migrated point keeps both of its slots untouched.
        assert_eq!(points[1].five_hour, Some(40));
        assert_eq!(points[1].weekly, Some(70));
        assert!(migrated.contains("not json"));

        // Running again changes nothing.
        super::move_primary_series_to_secondary(&path);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), migrated);
        let _ = std::fs::remove_file(&path);
    }
}
