//! Localized tray and notification messages use the same dictionaries as the UI.
use std::collections::HashMap;
use std::sync::OnceLock;

type Dictionary = HashMap<String, String>;

fn dictionaries() -> &'static [Dictionary; 6] {
    static DICTIONARIES: OnceLock<[Dictionary; 6]> = OnceLock::new();
    DICTIONARIES.get_or_init(|| {
        [
            include_str!("../../src/lib/locales/en.json"),
            include_str!("../../src/lib/locales/ko.json"),
            include_str!("../../src/lib/locales/ja.json"),
            include_str!("../../src/lib/locales/zh-CN.json"),
            include_str!("../../src/lib/locales/de.json"),
            include_str!("../../src/lib/locales/es.json"),
        ]
        .map(|source| serde_json::from_str(source).expect("invalid bundled locale dictionary"))
    })
}

fn lookup(loc: &str, key: &str) -> Option<&'static str> {
    let index = match loc {
        "ko" => 1,
        "ja" => 2,
        "zh-CN" => 3,
        "de" => 4,
        "es" => 5,
        _ => 0,
    };
    let values = dictionaries();
    values[index]
        .get(key)
        .or_else(|| values[0].get(key))
        .map(String::as_str)
}

fn text(loc: &str, key: &str) -> &'static str {
    lookup(loc, key).expect("missing bundled native translation")
}

fn match_locale(language: &str) -> Option<&'static str> {
    let normalized = language.trim().to_ascii_lowercase();
    let mut parts = normalized.split(['-', '_']);
    match parts.next()? {
        "ko" => Some("ko"),
        "en" => Some("en"),
        "ja" => Some("ja"),
        "de" => Some("de"),
        "es" => Some("es"),
        "zh" if !parts.any(|tag| matches!(tag, "hant" | "tw" | "hk" | "mo")) => Some("zh-CN"),
        _ => None,
    }
}

/// Resolve the saved choice or use the OS language when set to automatic.
pub fn effective_locale(language: &str) -> &'static str {
    match_locale(language)
        .or_else(|| tauri_plugin_os::locale().as_deref().and_then(match_locale))
        .unwrap_or("en")
}

#[allow(dead_code)]
pub fn bucket_label(loc: &str, key: &str, fallback: &str) -> String {
    lookup(loc, &format!("bucket.{key}"))
        .unwrap_or(fallback)
        .to_string()
}

/// Format the time until reset, or return an empty string for an invalid timestamp.
#[allow(dead_code)]
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
    if days > 0 {
        text(loc, "time.daysHours")
            .replace("{d}", &days.to_string())
            .replace("{h}", &hours.to_string())
    } else if hours > 0 {
        text(loc, "time.hoursMinutes")
            .replace("{h}", &hours.to_string())
            .replace("{m}", &mins.to_string())
    } else {
        text(loc, "time.minutes").replace("{m}", &mins.to_string())
    }
}

#[allow(dead_code)]
pub fn tooltip_line(loc: &str, label: &str, remaining: u8, countdown: &str) -> String {
    let value = text(loc, "native.tooltip.remaining").replace("{value}", &remaining.to_string());
    if countdown.is_empty() {
        format!("  {label}: {value}")
    } else {
        let reset = text(loc, "common.resetsIn").replace("{time}", countdown);
        format!("  {label}: {value} · {reset}")
    }
}

#[allow(dead_code)]
pub fn tooltip_signed_out(loc: &str) -> &'static str {
    text(loc, "native.tooltip.signedOut")
}

pub fn notify_approaching_title(loc: &str) -> &'static str {
    text(loc, "native.notify.warning")
}

pub fn notify_approaching_body(loc: &str, label: &str, used: u8) -> String {
    text(loc, "native.notify.used")
        .replace("{value}", &used.to_string())
        .replace("{label}", label)
}

pub fn notify_reset_title(loc: &str) -> &'static str {
    text(loc, "native.notify.reset")
}

pub fn notify_reset_body(loc: &str, label: &str) -> String {
    text(loc, "native.notify.resetBody").replace("{label}", label)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_system_languages_and_rejects_traditional_chinese() {
        for (raw, expected) in [
            ("ko-KR", Some("ko")),
            ("EN_us", Some("en")),
            ("ja-JP", Some("ja")),
            ("zh-Hans-SG", Some("zh-CN")),
            ("de-AT", Some("de")),
            ("es-MX", Some("es")),
            ("zh-Hant-TW", None),
            ("fr-FR", None),
        ] {
            assert_eq!(match_locale(raw), expected);
        }
    }

    #[test]
    fn native_notification_uses_the_selected_language() {
        assert_eq!(
            notify_approaching_body("es", &bucket_label("es", "codex-weekly", ""), 82),
            "Codex semanal: 82 % usado"
        );
        assert_eq!(
            notify_reset_body("zh-CN", &bucket_label("zh-CN", "five_hour", "")),
            "当前会话用量已重置"
        );
    }
}
