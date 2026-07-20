use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::Mutex;

use tauri::tray::TrayIcon;

use crate::api::UsageSnapshot;
use crate::config::Settings;
use crate::notify::NotifyState;

/// Shared application state managed by Tauri.
pub struct AppState {
    pub client: reqwest::Client,
    pub settings: Mutex<Settings>,
    /// Latest usage snapshot per service id (keyed by `UsageSnapshot::service_id`).
    pub last_snapshot: Mutex<HashMap<String, UsageSnapshot>>,
    pub tray: Mutex<Option<TrayIcon>>,
    /// Generation counter to disambiguate a single click from a double click.
    pub click_gen: AtomicU64,
    /// Epoch millis of the last tray double-click. On Windows a double-click emits
    /// Click, DoubleClick, Click; this lets a scheduled single-click suppress itself when
    /// it resolves just after a double-click (otherwise the widget toggles on stats open).
    pub last_double_ms: AtomicU64,
    pub notify_state: Mutex<NotifyState>,
    /// True while a login-capture watcher is running (prevents duplicates).
    pub login_watching: AtomicBool,
    /// Latest available update (version + notes), set by the startup update check.
    pub update_available: Mutex<Option<crate::update::UpdateInfo>>,
}

impl AppState {
    pub fn new(settings: Settings) -> Self {
        Self {
            client: crate::api::build_client(),
            settings: Mutex::new(settings),
            last_snapshot: Mutex::new(HashMap::new()),
            tray: Mutex::new(None),
            click_gen: AtomicU64::new(0),
            last_double_ms: AtomicU64::new(0),
            notify_state: Mutex::new(NotifyState::default()),
            login_watching: AtomicBool::new(false),
            update_available: Mutex::new(None),
        }
    }
}
