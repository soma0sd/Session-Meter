//! Window helpers: show/hide the local windows, place the frameless widget, and
//! position the custom themed context-menu window near the tray click.

use std::sync::atomic::Ordering;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize};
use tauri_plugin_positioner::{Position, WindowExt};

use crate::config;
use crate::state::AppState;

/// Safety net: if the login page has not left `about:blank` within 2s (webview stuck /
/// blank / unresponsive), cancel the capture watcher and close the window so the user is
/// never left staring at a blank login window that cannot be dismissed.
fn spawn_login_blank_guard(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let Some(win) = app.get_webview_window("login") else {
            return;
        };
        let blank = win
            .url()
            .map(|u| u.scheme() == "about" || u.as_str() == "about:blank")
            .unwrap_or(true);
        if blank {
            eprintln!("[cg] login page blank after 2s; auto-closing");
            if let Some(st) = app.try_state::<AppState>() {
                st.login_watching.store(false, Ordering::SeqCst);
            }
            let _ = win.hide();
        }
    });
}

pub fn show_and_focus(app: &AppHandle, label: &str) {
    if let Some(win) = app.get_webview_window(label) {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

pub fn open_settings(app: &AppHandle) {
    show_and_focus(app, "settings");
}

/// Create (or focus) the claude.ai login window. Must run on the main thread.
/// Spawns the Rust-driven capture watcher once the window exists.
pub fn create_login_window(app: &AppHandle) {
    let url: tauri::Url = match "https://claude.ai/login".parse() {
        Ok(u) => u,
        Err(e) => {
            eprintln!("[cg] login url parse error: {e}");
            return;
        }
    };
    // Reuse an existing (possibly hidden-on-close) login window: re-navigate so it
    // shows the live login page, then show + re-arm the capture watcher.
    if let Some(win) = app.get_webview_window("login") {
        let _ = win.navigate(url);
        let _ = win.show();
        let _ = win.set_focus();
        crate::auth::spawn_capture_watch(app.clone()); // self-guards against duplicates
        spawn_login_blank_guard(app);
        return;
    }
    match tauri::WebviewWindowBuilder::new(app, "login", tauri::WebviewUrl::External(url.clone()))
        .title("Sign in to Claude")
        .inner_size(480.0, 760.0)
        .center()
        .build()
    {
        Ok(win) => {
            eprintln!("[cg] login window created");
            // Packaged builds can leave an External window at about:blank; force the
            // navigation explicitly so claude.ai actually loads (otherwise it is blank).
            let _ = win.navigate(url);
            crate::auth::spawn_capture_watch(app.clone());
            spawn_login_blank_guard(app);
        }
        Err(e) => eprintln!("[cg] login window build error: {e}"),
    }
}

pub fn open_stats(app: &AppHandle) {
    show_and_focus(app, "stats");
}

pub fn open_style(app: &AppHandle) {
    show_and_focus(app, "style");
}

pub fn open_news(app: &AppHandle) {
    show_and_focus(app, "news");
}

/// The OS window label for a service's widget. Claude reuses the static `widget` window
/// declared in tauri.conf.json (so existing behavior is unchanged); other services get a
/// runtime `widget-{service}` window.
pub fn widget_label(service: &str) -> String {
    if service == "claude" {
        "widget".to_string()
    } else {
        format!("widget-{service}")
    }
}

/// Reverse of `widget_label`: the service id a widget window label belongs to.
pub fn service_from_widget_label(label: &str) -> Option<String> {
    if label == "widget" {
        Some("claude".to_string())
    } else {
        label.strip_prefix("widget-").map(|s| s.to_string())
    }
}

/// Services that should have a widget window: Claude always (shown even before sign-in), plus
/// any other logged-in service.
fn widget_services(app: &AppHandle) -> Vec<String> {
    let mut v = vec!["claude".to_string()];
    for s in crate::service::logged_in(app) {
        if s != "claude" && !v.contains(&s) {
            v.push(s);
        }
    }
    v
}

/// Create a runtime widget window for a non-Claude service (Claude uses the static one).
pub fn create_widget_window(app: &AppHandle, service: &str) {
    let label = widget_label(service);
    if app.get_webview_window(&label).is_some() {
        return;
    }
    let aot = app
        .try_state::<AppState>()
        .map(|s| s.settings.lock().unwrap().widget(service).always_on_top)
        .unwrap_or(true);
    match tauri::WebviewWindowBuilder::new(app, &label, tauri::WebviewUrl::App("widget.html".into()))
        .title("SessionMeter Widget")
        .inner_size(252.0, 150.0)
        .decorations(false)
        .transparent(true)
        .always_on_top(aot)
        .skip_taskbar(true)
        .resizable(false)
        .shadow(false)
        .visible(false)
        .build()
    {
        Ok(_) => eprintln!("[cg] widget window created ({service})"),
        Err(e) => eprintln!("[cg] widget window build error ({service}): {e}"),
    }
}

/// Restore a service's widget to its saved position, or bottom-right on first use (or whenever
/// the saved position is off-screen, self-healing a stale/sentinel value so the widget never
/// comes back invisible). A docked widget is never placed individually - `dock::apply_layout`
/// owns its position - so this returns immediately for one.
fn place_widget(app: &AppHandle, win: &tauri::WebviewWindow, service: &str) {
    if crate::dock::is_docked(app, service) {
        return;
    }
    match config::load_widget_pos(app, service).filter(|&(x, y)| pos_on_screen(win, x, y)) {
        Some((x, y)) => {
            let _ = win.set_position(PhysicalPosition::new(x, y));
        }
        None => {
            let _ = win.move_window(Position::BottomRight);
        }
    }
}

/// True if (x, y) lands inside a connected monitor. Rejects the Win32 minimized sentinel
/// (-32000) and any position on a since-disconnected display.
fn pos_on_screen(win: &tauri::WebviewWindow, x: i32, y: i32) -> bool {
    if x <= -32000 || y <= -32000 {
        return false;
    }
    let Ok(mons) = win.available_monitors() else {
        return false;
    };
    mons.iter().any(|m| {
        let p = m.position();
        let s = m.size();
        x >= p.x && y >= p.y && x < p.x + s.width as i32 && y < p.y + s.height as i32
    })
}

/// Persist a service widget's current on-screen position. Windows parks a minimizing/hiding
/// window at the (-32000,-32000) sentinel while still reporting is_visible()==true, so also
/// require the window not be minimized and reject the sentinel - otherwise that bogus position
/// gets saved and the widget returns off-screen (invisible) on the next launch.
pub fn save_widget_pos(app: &AppHandle, service: &str) {
    if let Some(win) = app.get_webview_window(&widget_label(service)) {
        if matches!(win.is_visible(), Ok(true)) && !matches!(win.is_minimized(), Ok(true)) {
            if let Ok(pos) = win.outer_position() {
                if pos.x > -32000 && pos.y > -32000 {
                    config::save_widget_pos(app, service, pos.x, pos.y);
                }
            }
        }
    }
}

/// Flush every service widget's position to disk before the process exits or restarts. Called
/// by both `quit_app` and the updater's install-then-restart path so an update never drops a
/// widget's on-screen position: the updater hides the window during teardown, which would
/// otherwise leave the last position unsaved and bring the widget back at its default corner.
pub fn persist_widgets_before_exit(app: &AppHandle) {
    for svc in widget_services(app) {
        save_widget_pos(app, &svc);
    }
}

/// Desired visibility of a service's widget (defaults to shown).
fn widget_should_show(app: &AppHandle, service: &str) -> bool {
    app.try_state::<AppState>()
        .map(|s| s.settings.lock().unwrap().widget(service).visible)
        .unwrap_or(true)
}

/// Persist a service widget's desired visibility (so a restart and the watchdog honor it).
fn set_widget_visible(app: &AppHandle, service: &str, visible: bool) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let snap = {
        let mut s = state.settings.lock().unwrap();
        let mut wc = s.widget(service);
        if wc.visible == visible {
            return;
        }
        wc.visible = visible;
        s.widgets.insert(service.to_string(), wc);
        s.clone()
    };
    let _ = config::save(app, &snap);
    // Broadcast like the other widget-control commands so an open Settings/Style window keeps a
    // current value; otherwise its next save would round-trip a stale value and revert this
    // show/hide (which reconcile would then re-enforce).
    let _ = app.emit("settings://changed", &snap);
}

/// Set a service widget's visibility from the UI (Widget Style window): persist the choice and
/// show/hide the existing window immediately. Window creation is left to `show_widget` / the
/// watchdog (avoids off-main-thread window builds); a logged-in service already has its window.
pub fn apply_widget_visible(app: &AppHandle, service: &str, visible: bool) {
    set_widget_visible(app, service, visible);
    if let Some(win) = app.get_webview_window(&widget_label(service)) {
        if visible {
            if !matches!(win.is_visible(), Ok(true)) {
                place_widget(app, &win, service);
            }
            let _ = win.show();
        } else {
            save_widget_pos(app, service);
            let _ = win.hide();
        }
    }
    // Hiding/showing a docked member changes who `pack()` sees, so the rest of the group
    // needs to re-flow around the gap (or make room again).
    crate::dock::apply_layout(app);
}

/// Show each service's widget on startup, unless the user had it hidden.
pub fn show_widget(app: &AppHandle) {
    for svc in widget_services(app) {
        if svc != "claude" {
            create_widget_window(app, &svc);
        }
        crate::dock::on_membership_changed(app, &svc);
        if !widget_should_show(app, &svc) {
            continue;
        }
        if let Some(win) = app.get_webview_window(&widget_label(&svc)) {
            if !matches!(win.is_visible(), Ok(true)) {
                place_widget(app, &win, &svc);
            }
            let _ = win.show();
        }
    }
    crate::dock::apply_layout(app);
    // Re-push settings right after showing, in case a widget's own startup `getSettings()`
    // call raced ahead of `AppState` being managed (the static "widget" window's webview can
    // begin executing JS before `setup()` finishes) and so applied stale/default values. This
    // costs nothing when there was no race - `applySettings` is idempotent - but guarantees
    // every widget converges on the real persisted style shortly after launch either way.
    if let Some(state) = app.try_state::<AppState>() {
        let settings = state.settings.lock().unwrap().clone();
        let _ = app.emit("settings://changed", &settings);
    }
}

/// Show or hide all service widgets together (tray left-click): show all if any is hidden,
/// otherwise hide all. Persists each widget's choice and position.
pub fn toggle_widget(app: &AppHandle) {
    let services = widget_services(app);
    let any_hidden = services.iter().any(|svc| {
        app.get_webview_window(&widget_label(svc))
            .map(|w| !matches!(w.is_visible(), Ok(true)))
            .unwrap_or(true)
    });
    for svc in &services {
        if svc != "claude" {
            create_widget_window(app, svc);
        }
        crate::dock::on_membership_changed(app, svc);
        if let Some(win) = app.get_webview_window(&widget_label(svc)) {
            if any_hidden {
                place_widget(app, &win, svc);
                let _ = win.show();
                let _ = win.set_focus();
            } else {
                save_widget_pos(app, svc);
                let _ = win.hide();
            }
            set_widget_visible(app, svc, any_hidden);
        }
    }
    crate::dock::apply_layout(app);
}

/// Keep each service widget's actual state in sync with its desired visibility, recovering if
/// it drifted (hidden when it should show, or pushed off-screen). Called each poll cycle.
pub fn reconcile_widget_visibility(app: &AppHandle) {
    for svc in widget_services(app) {
        if svc != "claude" {
            create_widget_window(app, &svc);
        }
        let Some(win) = app.get_webview_window(&widget_label(&svc)) else {
            continue;
        };
        if widget_should_show(app, &svc) {
            let on_screen = win
                .outer_position()
                .map(|p| pos_on_screen(&win, p.x, p.y))
                .unwrap_or(false);
            if !matches!(win.is_visible(), Ok(true)) || !on_screen {
                place_widget(app, &win, &svc);
                let _ = win.show();
            }
        } else if matches!(win.is_visible(), Ok(true)) {
            let _ = win.hide();
        }
    }
    crate::dock::apply_layout(app);
}

/// Position and show the custom context menu near the tray click point.
/// The tray usually sits bottom-right, so the menu is placed above-left of the click,
/// then clamped to the primary monitor.
pub fn show_menu_at(app: &AppHandle, x: f64, y: f64) {
    if let Some(win) = app.get_webview_window("menu") {
        let size = win
            .outer_size()
            .unwrap_or(PhysicalSize::new(196, 300));
        let w = size.width as i32;
        let h = size.height as i32;
        let mut tx = x as i32 - w;
        let mut ty = y as i32 - h;
        if let Ok(Some(mon)) = win.primary_monitor() {
            let mp = mon.position();
            let ms = mon.size();
            let min_x = mp.x;
            let min_y = mp.y;
            let max_x = (mp.x + ms.width as i32 - w).max(min_x);
            let max_y = (mp.y + ms.height as i32 - h).max(min_y);
            tx = tx.clamp(min_x, max_x);
            ty = ty.clamp(min_y, max_y);
        }
        let _ = win.set_position(PhysicalPosition::new(tx, ty));
        let _ = win.show();
        let _ = win.set_focus();
        // A tray right-click leaves the shell as the foreground process, so Windows
        // blocks the menu from taking focus; without this it shows but ignores clicks.
        force_foreground(&win);
    }
}

/// Force a window to the foreground even when another process (the shell, on a tray
/// click) holds it. Windows blocks a background `SetForegroundWindow`; briefly attaching
/// to the foreground thread's input queue lifts that restriction.
#[cfg(windows)]
fn force_foreground(win: &tauri::WebviewWindow) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId, SetForegroundWindow,
    };
    // tauri's `hwnd()` returns an HWND from its own (newer) `windows` crate version, so
    // rebuild ours from the raw handle to match this crate's Win32 signatures.
    let Ok(raw) = win.hwnd() else {
        return;
    };
    let hwnd = HWND(raw.0 as _);
    unsafe {
        let fg = GetForegroundWindow();
        let fg_thread = GetWindowThreadProcessId(fg, None);
        let cur = GetCurrentThreadId();
        if fg_thread != 0 && fg_thread != cur {
            let _ = AttachThreadInput(fg_thread, cur, true);
            let _ = SetForegroundWindow(hwnd);
            let _ = AttachThreadInput(fg_thread, cur, false);
        } else {
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

#[cfg(not(windows))]
fn force_foreground(_win: &tauri::WebviewWindow) {}
