//! Window helpers: show/hide the local windows, place the frameless widgets (one per
//! service, plus the docked group window), and position the custom themed context-menu
//! window near the tray click.

use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize};
use tauri_plugin_positioner::{Position, WindowExt};

use crate::config;
use crate::state::AppState;

/// The one frameless window that hosts every docked service as a grid cell (see `dock.rs`).
/// Created at runtime like the non-Claude service widgets; `Widget.svelte` switches to its
/// group mode when it finds itself running under this label.
pub const DOCK_LABEL: &str = "widget-dock";

/// Used when a widget window's `outer_size()` cannot be read yet (e.g. just created).
const FALLBACK_SIZE: (i32, i32) = (252, 150);

/// Safety net: if the login page has not left `about:blank` within 2s (webview stuck /
/// blank / unresponsive), cancel the capture watcher and close the window so the user is
/// never left staring at a blank login window that cannot be dismissed.
fn spawn_login_blank_guard(app: &AppHandle, generation: u64) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(2)).await;
        if !crate::auth::capture_is_current(&app, generation) {
            return;
        }
        let Some(win) = app.get_webview_window("login") else {
            return;
        };
        let blank = win
            .url()
            .map(|u| u.scheme() == "about" || u.as_str() == "about:blank")
            .unwrap_or(true);
        if blank {
            eprintln!("[cg] login page blank after 2s; auto-closing");
            crate::auth::cancel_capture_watch(&app);
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

/// Create (or focus) Claude's browser-login window. Must run on the main thread.
/// Codex and Gemini use isolated helper processes instead of this shared webview.
pub fn create_login_window(app: &AppHandle, service: &str) {
    let (login_url, title) = match service {
        crate::service::CLAUDE => ("https://claude.ai/login", "Sign in to Claude"),
        other => {
            eprintln!("[cg] unsupported browser login service: {other}");
            return;
        }
    };
    let url: tauri::Url = match login_url.parse() {
        Ok(u) => u,
        Err(e) => {
            eprintln!("[cg] login url parse error: {e}");
            return;
        }
    };
    // Reuse an existing (possibly hidden-on-close) login window: re-navigate so it
    // shows the live login page, then show + re-arm the capture watcher.
    if let Some(win) = app.get_webview_window("login") {
        crate::auth::cancel_capture_watch(app);
        let _ = win.navigate(url);
        let _ = win.set_title(title);
        let _ = win.show();
        let _ = win.set_focus();
        if let Some(generation) = crate::auth::spawn_capture_watch(app.clone(), service.to_string()) {
            spawn_login_blank_guard(app, generation);
        }
        return;
    }
    match tauri::WebviewWindowBuilder::new(app, "login", tauri::WebviewUrl::External(url.clone()))
        .title(title)
        .inner_size(480.0, 760.0)
        .center()
        .build()
    {
        Ok(win) => {
            eprintln!("[cg] login window created");
            // Packaged builds can leave an External window at about:blank; force the
            // navigation explicitly so the external login page actually loads (otherwise blank).
            let _ = win.navigate(url);
            if let Some(generation) = crate::auth::spawn_capture_watch(app.clone(), service.to_string()) {
                spawn_login_blank_guard(app, generation);
            }
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

/// The OS window label for a service's own widget. Claude reuses the static `widget` window
/// declared in tauri.conf.json (so existing behavior is unchanged); other services get a
/// runtime `widget-{service}` window.
pub fn widget_label(service: &str) -> String {
    if service == "claude" {
        "widget".to_string()
    } else {
        format!("widget-{service}")
    }
}

/// Reverse of `widget_label`: the service id a widget window label belongs to. The docked
/// group window (`DOCK_LABEL`) belongs to no single service and yields `None`.
pub fn service_from_widget_label(label: &str) -> Option<String> {
    if label == "widget" {
        Some("claude".to_string())
    } else if label == DOCK_LABEL {
        None
    } else {
        label.strip_prefix("widget-").map(|s| s.to_string())
    }
}

/// True for every frameless usage widget window: the per-service ones and the docked group.
pub fn is_widget_window(label: &str) -> bool {
    label == DOCK_LABEL || service_from_widget_label(label).is_some()
}

/// Services that should have a widget at all: Claude always (shown even before sign-in), plus
/// any other logged-in service. Whether a given one gets its own window or a cell in the
/// docked window is decided per service in `reconcile_widget_visibility`.
fn widget_services(app: &AppHandle) -> Vec<String> {
    let mut v = vec!["claude".to_string()];
    for s in crate::service::logged_in(app) {
        if s != "claude" && !v.contains(&s) {
            v.push(s);
        }
    }
    v
}

/// Apply a widget window's layering: always-on-top, and taskbar presence as its inverse. A
/// widget that no longer floats above everything is an ordinary window the user may need to
/// find again once something covers it, so it gets a taskbar button (and an Alt+Tab entry);
/// a floating one stays out of the taskbar as before.
pub fn apply_layering(win: &tauri::WebviewWindow, always_on_top: bool) {
    let _ = win.set_always_on_top(always_on_top);
    let _ = win.set_skip_taskbar(always_on_top);
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
        .skip_taskbar(aot)
        .resizable(false)
        .shadow(false)
        .visible(false)
        .build()
    {
        Ok(_) => eprintln!("[cg] widget window created ({service})"),
        Err(e) => eprintln!("[cg] widget window build error ({service}): {e}"),
    }
}

/// Create the docked group window (hidden). Same shell as a service widget; the page inside
/// tells the two apart by label.
pub fn create_dock_window(app: &AppHandle) {
    if app.get_webview_window(DOCK_LABEL).is_some() {
        return;
    }
    let aot = app
        .try_state::<AppState>()
        .map(|s| s.settings.lock().unwrap().dock.always_on_top)
        .unwrap_or(true);
    match tauri::WebviewWindowBuilder::new(app, DOCK_LABEL, tauri::WebviewUrl::App("widget.html".into()))
        .title("SessionMeter Widget")
        .inner_size(252.0, 150.0)
        .decorations(false)
        .transparent(true)
        .always_on_top(aot)
        .skip_taskbar(aot)
        .resizable(false)
        .shadow(false)
        .visible(false)
        .build()
    {
        Ok(_) => eprintln!("[cg] docked widget window created"),
        Err(e) => eprintln!("[cg] docked widget window build error: {e}"),
    }
}

/// Restore a window to `saved`, or bottom-right on first use (or whenever the saved position
/// is off-screen, self-healing a stale/sentinel value so the window never comes back
/// invisible). Returns the position it ended up at when that differs from `saved`, so the
/// caller can persist the healed value.
fn place_window(win: &tauri::WebviewWindow, saved: Option<(i32, i32)>) -> Option<(i32, i32)> {
    let (w, h) = win
        .outer_size()
        .map(|s| (s.width as i32, s.height as i32))
        .unwrap_or(FALLBACK_SIZE);
    match saved {
        Some((x, y)) => {
            let (nx, ny, _moved) = clamp_rect_to_screen(win, x, y, w, h);
            let _ = win.set_position(PhysicalPosition::new(nx, ny));
            ((nx, ny) != (x, y)).then_some((nx, ny))
        }
        None => {
            let _ = win.move_window(Position::BottomRight);
            let pos = win.outer_position().ok()?;
            let (nx, ny, _moved) = clamp_rect_to_screen(win, pos.x, pos.y, w, h);
            let _ = win.set_position(PhysicalPosition::new(nx, ny));
            Some((nx, ny))
        }
    }
}

fn place_widget(app: &AppHandle, win: &tauri::WebviewWindow, service: &str) {
    if let Some((x, y)) = place_window(win, config::load_widget_pos(app, service)) {
        config::save_widget_pos(app, service, x, y);
    }
}

fn place_dock_window(app: &AppHandle, win: &tauri::WebviewWindow) {
    if let Some((x, y)) = place_window(win, config::load_dock_anchor(app)) {
        config::save_dock_anchor(app, x, y);
    }
}

/// A monitor's work area (its bounds minus the taskbar and any other docked shell bars) as
/// `(x, y, w, h)`. Widgets are kept inside this rather than the raw bounds: a widget that is
/// not always-on-top would otherwise end up with its bottom rows under the taskbar.
fn work_rect(mon: &tauri::Monitor) -> (i32, i32, i32, i32) {
    let wa = mon.work_area();
    (wa.position.x, wa.position.y, wa.size.width as i32, wa.size.height as i32)
}

/// Clamps a rectangle (x, y, w, h) so that it is fully contained inside the best-matching
/// monitor's work area. Returns `(clamped_x, clamped_y, moved)` where `moved` is true if the
/// rectangle was partially or fully outside and had to be adjusted.
pub fn clamp_rect_to_screen(win: &tauri::WebviewWindow, x: i32, y: i32, w: i32, h: i32) -> (i32, i32, bool) {
    if x <= -32000 || y <= -32000 {
        if let Ok(Some(mon)) = win.primary_monitor() {
            let (mx, my, mw, mh) = work_rect(&mon);
            let target_x = mx + (mw - w).max(0);
            let target_y = my + (mh - h).max(0);
            return (target_x, target_y, true);
        }
    }

    let Ok(mons) = win.available_monitors() else {
        return (x, y, false);
    };
    if mons.is_empty() {
        return (x, y, false);
    }

    let primary_mon = win.primary_monitor().ok().flatten();
    let cx = x + w / 2;
    let cy = y + h / 2;
    let mon = mons
        .iter()
        .find(|m| {
            let (mx, my, mw, mh) = work_rect(m);
            cx >= mx && cy >= my && cx < mx + mw && cy < my + mh
        })
        .or_else(|| {
            mons.iter().max_by_key(|m| {
                let (mx, my, mw, mh) = work_rect(m);
                let overlap_w = (x + w).min(mx + mw) - x.max(mx);
                let overlap_h = (y + h).min(my + mh) - y.max(my);
                if overlap_w > 0 && overlap_h > 0 {
                    overlap_w * overlap_h
                } else {
                    0
                }
            })
        })
        .or(primary_mon.as_ref())
        .or(mons.first());

    let Some(mon) = mon else {
        return (x, y, false);
    };

    let (mx, my, mw, mh) = work_rect(mon);
    let is_fully_contained = x >= mx && y >= my && (x + w) <= (mx + mw) && (y + h) <= (my + mh);

    if is_fully_contained {
        (x, y, false)
    } else {
        let clamped_x = if w >= mw { mx } else { x.clamp(mx, mx + mw - w) };
        let clamped_y = if h >= mh { my } else { y.clamp(my, my + mh - h) };
        (clamped_x, clamped_y, true)
    }
}

/// Move a visible, non-minimized window fully back inside screen bounds if any part of it is
/// outside. Returns the corrected position when it had to move.
fn ensure_on_screen(win: &tauri::WebviewWindow) -> Option<(i32, i32)> {
    if matches!(win.is_minimized(), Ok(true)) {
        return None;
    }
    let pos = win.outer_position().ok()?;
    let (w, h) = win
        .outer_size()
        .map(|s| (s.width as i32, s.height as i32))
        .unwrap_or(FALLBACK_SIZE);
    let (nx, ny, moved) = clamp_rect_to_screen(win, pos.x, pos.y, w, h);
    if moved {
        let _ = win.set_position(PhysicalPosition::new(nx, ny));
        Some((nx, ny))
    } else {
        None
    }
}

/// Ensure a service's own widget is fully inside screen bounds.
pub fn ensure_widget_on_screen(app: &AppHandle, service: &str) {
    if let Some(win) = app.get_webview_window(&widget_label(service)) {
        if let Some((x, y)) = ensure_on_screen(&win) {
            config::save_widget_pos(app, service, x, y);
        }
    }
}

/// Ensure the docked group window is fully inside screen bounds.
pub fn ensure_dock_on_screen(app: &AppHandle) {
    if let Some(win) = app.get_webview_window(DOCK_LABEL) {
        if let Some((x, y)) = ensure_on_screen(&win) {
            config::save_dock_anchor(app, x, y);
        }
    }
}

/// A window's current on-screen position, if it is really on screen. Windows parks a
/// minimizing/hiding window at the (-32000,-32000) sentinel while still reporting
/// is_visible()==true, so also require the window not be minimized and reject the sentinel -
/// otherwise that bogus position gets saved and the window returns off-screen (invisible) on
/// the next launch.
fn on_screen_position(win: &tauri::WebviewWindow) -> Option<(i32, i32)> {
    if !matches!(win.is_visible(), Ok(true)) || matches!(win.is_minimized(), Ok(true)) {
        return None;
    }
    let pos = win.outer_position().ok()?;
    (pos.x > -32000 && pos.y > -32000).then_some((pos.x, pos.y))
}

/// Persist a service widget's current on-screen position.
pub fn save_widget_pos(app: &AppHandle, service: &str) {
    if let Some(win) = app.get_webview_window(&widget_label(service)) {
        if let Some((x, y)) = on_screen_position(&win) {
            config::save_widget_pos(app, service, x, y);
        }
    }
}

/// Persist the docked group window's current on-screen position.
pub fn save_dock_pos(app: &AppHandle) {
    if let Some(win) = app.get_webview_window(DOCK_LABEL) {
        if let Some((x, y)) = on_screen_position(&win) {
            config::save_dock_anchor(app, x, y);
        }
    }
}

/// Flush every widget window's position to disk before the process exits or restarts. Called
/// by both `quit_app` and the updater's install-then-restart path so an update never drops a
/// widget's on-screen position: the updater hides the window during teardown, which would
/// otherwise leave the last position unsaved and bring the widget back at its default corner.
pub fn persist_widgets_before_exit(app: &AppHandle) {
    for svc in widget_services(app) {
        save_widget_pos(app, &svc);
    }
    save_dock_pos(app);
}

/// Desired visibility of a service's widget (defaults to shown).
fn widget_should_show(app: &AppHandle, service: &str) -> bool {
    app.try_state::<AppState>()
        .map(|s| s.settings.lock().unwrap().widget(service).visible)
        .unwrap_or(true)
}

/// Persist the desired visibility of one or more service widgets (so a restart and the
/// watchdog honor it) and broadcast the change once. Broadcasting matters: an open
/// Settings/Style window otherwise keeps a stale value that its next save would round-trip
/// back, reverting this show/hide (which reconcile would then re-enforce).
fn set_widgets_visible(app: &AppHandle, services: &[String], visible: bool) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let snap = {
        let mut s = state.settings.lock().unwrap();
        let mut changed = false;
        for svc in services {
            let mut wc = s.widget(svc);
            if wc.visible != visible {
                wc.visible = visible;
                s.widgets.insert(svc.clone(), wc);
                changed = true;
            }
        }
        if !changed {
            return;
        }
        s.clone()
    };
    let _ = config::save(app, &snap);
    let _ = app.emit("settings://changed", &snap);
}

/// Show a service's own widget window (placing it and applying its layering when it was
/// hidden) or hide it (remembering where it was), per `show`. Window creation is left to the
/// callers, which all run on the main thread.
fn set_service_window_shown(app: &AppHandle, service: &str, show: bool) {
    let Some(win) = app.get_webview_window(&widget_label(service)) else {
        return;
    };
    let visible = matches!(win.is_visible(), Ok(true));
    if show {
        if !visible {
            place_widget(app, &win, service);
            let aot = app
                .try_state::<AppState>()
                .map(|s| s.settings.lock().unwrap().widget(service).always_on_top)
                .unwrap_or(true);
            apply_layering(&win, aot);
            let _ = win.show();
        }
    } else if visible {
        save_widget_pos(app, service);
        let _ = win.hide();
    }
}

/// Same for the docked group window. Creates it on first show.
fn set_dock_window_shown(app: &AppHandle, show: bool) {
    if show {
        create_dock_window(app);
    }
    let Some(win) = app.get_webview_window(DOCK_LABEL) else {
        return;
    };
    let visible = matches!(win.is_visible(), Ok(true));
    if show {
        if !visible {
            place_dock_window(app, &win);
            let aot = app
                .try_state::<AppState>()
                .map(|s| s.settings.lock().unwrap().dock.always_on_top)
                .unwrap_or(true);
            apply_layering(&win, aot);
            let _ = win.show();
        }
    } else if visible {
        save_dock_pos(app);
        let _ = win.hide();
    }
}

/// Set a service widget's visibility from the UI (Widget Style window): persist the choice
/// and bring the windows in line right away. For a docked member this changes which cells
/// the group window shows (and hides the group when nothing is left).
pub fn apply_widget_visible(app: &AppHandle, service: &str, visible: bool) {
    set_widgets_visible(app, &[service.to_string()], visible);
    reconcile_widget_visibility(app);
}

/// Hide a dynamically-created service widget after its session is removed or expires. The
/// persisted visibility preference remains unchanged so the widget returns automatically after
/// the next successful login. The docked window drops the service on the next reconcile.
pub fn hide_runtime_widget(app: &AppHandle, service: &str) {
    if service == crate::service::CLAUDE {
        return;
    }
    set_service_window_shown(app, service, false);
}

/// Show each service's widget on startup, unless the user had it hidden.
pub fn show_widget(app: &AppHandle) {
    reconcile_widget_visibility(app);
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

/// The window a service's widget lives in: its own, or the docked group's.
fn widget_window_for(app: &AppHandle, service: &str) -> Option<tauri::WebviewWindow> {
    if crate::dock::is_docked(app, service) {
        app.get_webview_window(DOCK_LABEL)
    } else {
        app.get_webview_window(&widget_label(service))
    }
}

/// Show or hide all service widgets together (tray left-click): show all if any is hidden or
/// minimized, otherwise hide all. Persists each widget's choice and position.
pub fn toggle_widget(app: &AppHandle) {
    let services = widget_services(app);
    for svc in &services {
        if svc != "claude" {
            create_widget_window(app, svc);
        }
        crate::dock::on_membership_changed(app, svc);
    }
    let any_hidden = services.iter().any(|svc| {
        if !widget_should_show(app, svc) {
            return true;
        }
        widget_window_for(app, svc)
            .map(|w| !matches!(w.is_visible(), Ok(true)) || matches!(w.is_minimized(), Ok(true)))
            .unwrap_or(true)
    });
    set_widgets_visible(app, &services, any_hidden);
    reconcile_widget_visibility(app);
    if any_hidden {
        // Bring the (re)shown windows forward. A widget shown in the taskbar can have been
        // minimized from there, which reconcile deliberately leaves alone; an explicit tray
        // click is the user asking for it back.
        let mut raised: Vec<String> = Vec::new();
        for svc in &services {
            let Some(win) = widget_window_for(app, svc) else {
                continue;
            };
            if raised.iter().any(|l| l == win.label()) {
                continue;
            }
            raised.push(win.label().to_string());
            let _ = win.unminimize();
            let _ = win.set_focus();
        }
    }
}

/// Bring every widget window into line with what the settings say: each service either has
/// its own window shown, is a cell in the docked group window, or is hidden. Also recovers a
/// window that drifted (off-screen, or hidden when it should show), and hides the runtime
/// widget of a service that signed out. Called on startup, after every visibility/docking
/// change, and each poll cycle as a watchdog. Must run on the main thread (creates windows).
pub fn reconcile_widget_visibility(app: &AppHandle) {
    let active = widget_services(app);
    for &svc in crate::service::all() {
        if svc != crate::service::CLAUDE && !active.iter().any(|id| id == svc) {
            hide_runtime_widget(app, svc);
        }
    }
    for svc in &active {
        if svc != "claude" {
            create_widget_window(app, svc);
        }
        // A service signed into while the app is already running (Codex, say) first shows up
        // here, so this is where it has to be offered to the dock.
        crate::dock::on_membership_changed(app, svc);
    }
    for svc in &active {
        let own_window = !crate::dock::is_docked(app, svc) && widget_should_show(app, svc);
        set_service_window_shown(app, svc, own_window);
        if own_window {
            ensure_widget_on_screen(app, svc);
        }
    }
    let members = crate::dock::sync(app, &active);
    set_dock_window_shown(app, !members.is_empty());
    if !members.is_empty() {
        ensure_dock_on_screen(app);
    }
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
