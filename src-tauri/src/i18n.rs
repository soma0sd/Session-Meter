//! Minimal Rust-side i18n for strings that Rust emits natively (tray tooltip,
//! notifications). The frontend has its own richer dictionary; this only covers the
//! handful of strings produced outside the webview.

/// Map the language setting to an effective locale (auto -> OS locale -> ko/en).
pub fn effective_locale(language: &str) -> &'static str {
    match language {
        "ko" => "ko",
        "en" => "en",
        _ => {
            let l = tauri_plugin_os::locale().unwrap_or_default().to_lowercase();
            if l.starts_with("ko") {
                "ko"
            } else {
                "en"
            }
        }
    }
}

pub fn bucket_label(loc: &str, key: &str, fallback: &str) -> String {
    match (loc, key) {
        ("ko", "five_hour") => "이번 세션".to_string(),
        ("ko", "seven_day") => "주간 세션".to_string(),
        ("en", "five_hour") => "Current session".to_string(),
        ("en", "seven_day") => "Weekly session".to_string(),
        _ => fallback.to_string(),
    }
}

/// Short "time until reset" from an ISO-8601 timestamp (e.g. "4d 12h" / "2시간 45분").
/// Empty when the timestamp is missing/unparseable.
pub fn fmt_countdown(loc: &str, resets_at: &str) -> String {
    let Some(target) = crate::history::parse_iso(resets_at) else {
        return String::new();
    };
    let secs = (target - time::OffsetDateTime::now_utc())
        .whole_seconds()
        .max(0);
    let days = secs / 86_400;
    let hours = (secs % 86_400) / 3600;
    let mins = (secs % 3600) / 60;
    if loc == "ko" {
        if days > 0 {
            format!("{days}일 {hours}시간")
        } else if hours > 0 {
            format!("{hours}시간 {mins}분")
        } else {
            format!("{mins}분")
        }
    } else if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {mins}m")
    } else {
        format!("{mins}m")
    }
}

/// One tray-tooltip bucket line: "  Current session: 62% left · resets in 1h 59m".
pub fn tooltip_line(loc: &str, label: &str, remaining: u8, countdown: &str) -> String {
    let base = if loc == "ko" {
        format!("  {label}: {remaining}% 남음")
    } else {
        format!("  {label}: {remaining}% left")
    };
    if countdown.is_empty() {
        base
    } else if loc == "ko" {
        format!("{base} · {countdown} 후 초기화")
    } else {
        format!("{base} · resets in {countdown}")
    }
}

pub fn tooltip_signed_out(loc: &str) -> &'static str {
    if loc == "ko" {
        "SessionMeter - 로그인 필요"
    } else {
        "SessionMeter - not signed in"
    }
}

pub fn notify_approaching_title(loc: &str) -> &'static str {
    if loc == "ko" {
        "SessionMeter · 사용량 경고"
    } else {
        "SessionMeter · Usage warning"
    }
}

pub fn notify_approaching_body(loc: &str, label: &str, used: u8) -> String {
    if loc == "ko" {
        format!("{label}: {used}% 사용됨")
    } else {
        format!("{label}: {used}% used")
    }
}

pub fn notify_reset_title(loc: &str) -> &'static str {
    if loc == "ko" {
        "SessionMeter · 사용량 초기화"
    } else {
        "SessionMeter · Usage reset"
    }
}

pub fn notify_reset_body(loc: &str, label: &str) -> String {
    if loc == "ko" {
        format!("{label} 사용량이 초기화되었습니다")
    } else {
        format!("{label} has reset")
    }
}
