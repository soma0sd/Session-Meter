//! Theme resolution. The setting is light | dark | system; "system" follows the OS,
//! which we read from an existing window's reported theme.

use tauri::{AppHandle, Manager, Theme};

/// Resolve whether the UI should render dark, given the theme setting.
/// Currently unused (the tray shows the fixed app icon); kept for a possible
/// "dynamic tray icon" option that re-themes with the OS.
#[allow(dead_code)]
pub fn resolve_dark(app: &AppHandle, theme_setting: &str) -> bool {
    match theme_setting {
        "dark" => true,
        "light" => false,
        _ => app
            .get_webview_window("settings")
            .or_else(|| app.get_webview_window("widget"))
            .or_else(|| app.get_webview_window("stats"))
            .and_then(|w| w.theme().ok())
            .map(|t| matches!(t, Theme::Dark))
            .unwrap_or(true),
    }
}
