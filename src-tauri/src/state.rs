use std::collections::{HashMap, HashSet};
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
    /// Changes whenever a browser-login capture starts or is cancelled. A watcher only acts
    /// while its own generation remains current, preventing a hidden previous login from
    /// capturing cookies after the window has been re-used for another service.
    pub login_capture_generation: AtomicU64,
    /// Latest available update (version + notes), set by the startup update check.
    pub update_available: Mutex<Option<crate::update::UpdateInfo>>,
    /// True while `dock::apply_layout` is repositioning docked widget windows. Lets the
    /// `WindowEvent::Moved` handler (and `apply_layout` itself) recognize its own relayout
    /// echoes instead of mistaking them for a user drag or re-entering the relayout.
    pub dock_relayout_in_progress: AtomicBool,
    /// Physical panel size reported by each widget while no popover is open. Docking uses this
    /// instead of the temporarily enlarged webview size of an open kebab popover.
    pub widget_base_sizes: Mutex<HashMap<String, (i32, i32)>>,
    /// Widgets whose kebab popover is currently open. This is transient UI state only.
    pub widget_menus_open: Mutex<HashSet<String>>,
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
            login_capture_generation: AtomicU64::new(0),
            update_available: Mutex::new(None),
            dock_relayout_in_progress: AtomicBool::new(false),
            widget_base_sizes: Mutex::new(HashMap::new()),
            widget_menus_open: Mutex::new(HashSet::new()),
        }
    }
}
