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

        // A weekly key always uses the weekly threshold, including Codex whose weekly quota
        // occupies the shared primary serialization slot for compatibility.
        let is_weekly = b.key.contains("week") || b.key.contains("seven_day");
        let threshold = if is_weekly {
            settings.notify.weekly_threshold
        } else if snapshot.primary_key.as_deref() == Some(b.key.as_str()) {
            settings.notify.session_threshold
        } else {
            settings.notify.weekly_threshold
        };
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
