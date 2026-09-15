//! Threshold + reset desktop notifications, with duplicate suppression. Evaluated on
//! every applied snapshot.

use std::collections::HashMap;

use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::api::UsageSnapshot;
use crate::i18n;
use crate::state::AppState;

/// Per-bucket notification bookkeeping (in memory, reset on restart).
#[derive(Default)]
pub struct NotifyState {
    /// Highest used% threshold already notified for the current window.
    notified: HashMap<String, u8>,
    /// Epoch-seconds of the future `resets_at` we are counting down to, per bucket.
    /// Absent = idle (no active countdown), so reset notifications are held until one appears.
    countdown_target: HashMap<String, i64>,
}

fn send(app: &AppHandle, title: &str, body: &str) {
    let _ = app.notification().builder().title(title).body(body).show();
}

/// Which alert threshold a bucket is judged against.
///
/// A weekly key always uses the weekly threshold, whichever slot it occupies: on a plan without
/// a 5-hour session window, Codex's weekly quota is also its headline window.
///
/// A 5-hour window uses the session threshold even when it is *not* the headline window. This
/// is what `primary_key` alone cannot express: Antigravity reports two independent 5-hour
/// windows (`gemini-5h` and `3p-5h`) but its `primary_key` is pinned to the Gemini group by
/// design (see `antigravity::parse_snapshot`), so `3p-5h` used to fall through to the weekly
/// threshold - a 5-hour window judged by a weekly number.
///
/// Anything else keeps the original rule (headline window gets the session threshold, every
/// other bucket the weekly one), which is what Gemini's per-model buckets rely on.
fn threshold_for(
    key: &str,
    primary_key: Option<&str>,
    notify: &crate::config::NotifySettings,
) -> u8 {
    if key.contains("week") || key.contains("seven_day") {
        return notify.weekly_threshold;
    }
    // `ends_with` rather than `contains`, so a future key like "25h" is not mistaken for one.
    let is_five_hour = key == "five_hour" || key.ends_with("-5h");
    if is_five_hour || primary_key == Some(key) {
        return notify.session_threshold;
    }
    notify.weekly_threshold
}

pub fn evaluate(app: &AppHandle, snapshot: &UsageSnapshot) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let settings = state.settings.lock().unwrap().clone();
    let loc = i18n::effective_locale(&settings.language);
    let now_ts = time::OffsetDateTime::now_utc().unix_timestamp();

    let mut ns = state.notify_state.lock().unwrap();

    for b in &snapshot.buckets {
        // Bucket names are only service-local. Prefix the in-memory notification key so
        // a similarly named quota window from another provider cannot suppress an alert.
        let key = format!("{}:{}", snapshot.service_id, b.key);
        let label = i18n::bucket_label(loc, &b.key, &b.label);

        // Reset detection is countdown-based: notify when the session countdown we were
        // tracking has elapsed. While idle (no future resets_at), hold until one appears,
        // and only start tracking a countdown that is genuinely in the future.
        let expired = ns
            .countdown_target
            .get(&key)
            .map(|&target| now_ts >= target)
            .unwrap_or(false);
        if expired {
            ns.countdown_target.remove(&key);
            ns.notified.remove(&key);
            if settings.notify.enabled && settings.notify.on_reset {
                send(app, i18n::notify_reset_title(loc), &i18n::notify_reset_body(loc, &label));
            }
        }
        if let Some(ts) = crate::history::parse_iso(&b.resets_at).map(|t| t.unix_timestamp()) {
            if ts > now_ts {
                ns.countdown_target.insert(key.clone(), ts);
            }
        }

        let threshold = threshold_for(&b.key, snapshot.primary_key.as_deref(), &settings.notify);
        let used = b.utilization;
        let already = ns.notified.get(&key).copied().unwrap_or(0);
        if threshold > 0 && used >= threshold && already < threshold {
            ns.notified.insert(key.clone(), threshold);
            if settings.notify.enabled {
                send(
                    app,
                    i18n::notify_approaching_title(loc),
                    &i18n::notify_approaching_body(loc, &label, used),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::threshold_for;
    use crate::config::NotifySettings;

    fn settings() -> NotifySettings {
        NotifySettings {
            enabled: true,
            session_threshold: 70,
            weekly_threshold: 90,
            on_reset: true,
        }
    }

    #[test]
    fn antigravity_judges_both_five_hour_windows_by_the_session_threshold() {
        let n = settings();
        // `primary_key` is pinned to the Gemini group, so `3p-5h` is never the headline window.
        assert_eq!(threshold_for("gemini-5h", Some("gemini-5h"), &n), 70);
        assert_eq!(threshold_for("3p-5h", Some("gemini-5h"), &n), 70);
        assert_eq!(threshold_for("gemini-weekly", Some("gemini-5h"), &n), 90);
        assert_eq!(threshold_for("3p-weekly", Some("gemini-5h"), &n), 90);
    }

    #[test]
    fn every_other_service_keeps_its_existing_mapping() {
        let n = settings();
        assert_eq!(threshold_for("five_hour", Some("five_hour"), &n), 70);
        assert_eq!(threshold_for("seven_day", Some("five_hour"), &n), 90);
        assert_eq!(threshold_for("seven_day_opus", Some("five_hour"), &n), 90);
        assert_eq!(threshold_for("codex-5h", Some("codex-5h"), &n), 70);
        assert_eq!(threshold_for("codex-weekly", Some("codex-5h"), &n), 90);
        // ChatGPT Pro: the weekly quota is the headline window and still uses the weekly value.
        assert_eq!(threshold_for("codex-weekly", Some("codex-weekly"), &n), 90);
        // Gemini's per-model buckets: headline gets the session threshold, the rest weekly.
        assert_eq!(threshold_for("current", Some("current"), &n), 70);
        assert_eq!(threshold_for("weekly", Some("current"), &n), 90);
    }
}
