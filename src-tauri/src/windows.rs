//! Window helpers: show/hide the local windows, place the frameless widget, and
//! position the custom themed context-menu window near the tray click.

use std::sync::atomic::Ordering;
use std::time::Duration;

use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize};
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

pub fn open_news(app: &AppHandle) {
    show_and_focus(app, "news");
}

/// Restore the widget to its saved position, or bottom-right on first use.
fn place_widget(app: &AppHandle, win: &tauri::WebviewWindow) {
    if let Some((x, y)) = config::load_widget_pos(app) {
        let _ = win.set_position(PhysicalPosition::new(x, y));
    } else {
        let _ = win.move_window(Position::BottomRight);
    }
}

/// Persist the widget's current on-screen position.
pub fn save_widget_pos(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("widget") {
        if matches!(win.is_visible(), Ok(true)) {
            if let Ok(pos) = win.outer_position() {
                config::save_widget_pos(app, pos.x, pos.y);
            }
        }
    }
}

/// Show the widget (placing it if it is not already visible). Used on startup.
pub fn show_widget(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("widget") {
        if !matches!(win.is_visible(), Ok(true)) {
            place_widget(app, &win);
        }
        let _ = win.show();
    }
}

/// Show or hide the widget, persisting/restoring its position across toggles.
pub fn toggle_widget(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("widget") {
        if matches!(win.is_visible(), Ok(true)) {
            save_widget_pos(app);
            let _ = win.hide();
        } else {
            place_widget(app, &win);
            let _ = win.show();
            let _ = win.set_focus();
        }
    }
}

/// Position and show the custom context menu near the tray click point.
/// The tray usually sits bottom-right, so the menu is placed above-left of the click,
/// then clamped to the primary monitor.
pub fn show_menu_at(app: &AppHandle, x: f64, y: f64) {
    if let Some(win) = app.get_webview_window("menu") {
        let size = win
            .outer_size()
            .unwrap_or(PhysicalSize::new(196, 168));
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
