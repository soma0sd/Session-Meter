mod api;
mod auth;
mod commands;
mod config;
mod error;
mod history;
mod i18n;
mod icon;
mod notify;
mod poller;
mod state;
mod theme;
mod tray;
mod update;
mod usage;
mod windows;

use std::sync::atomic::Ordering;

use tauri::{Manager, WindowEvent};

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Register single-instance FIRST so a second launch focuses the existing app.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(win) = app.get_webview_window("settings") {
                let _ = win.show();
                let _ = win.set_focus();
            }
        }))
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_positioner::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let handle = app.handle().clone();
            // Debug builds start from default settings each run for clean testing.
            #[cfg(debug_assertions)]
            config::clear_settings(&handle);
            let settings = config::load(&handle);
            app.manage(AppState::new(settings));
            tray::build_tray(&handle)?;

            // Always show the desktop widget on startup (including first run, before
            // sign-in), so it is visible immediately. always_on_top defaults to true.
            windows::show_widget(&handle);

            // First-run onboarding: if there's no saved session, also open Settings and the
            // claude.ai login window so the user can sign in.
            if config::load_cookie(&handle).is_none() {
                if let Some(win) = handle.get_webview_window("settings") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
                // Defer login-window creation to the event loop: building an
                // external-URL webview during setup() fails silently on Windows, so we
                // queue it (same path as the open_login_window command, which works).
                let h = handle.clone();
                let _ = handle.run_on_main_thread(move || windows::create_login_window(&h));
            }

            // Poll immediately on start, then every refresh_interval_min.
            poller::start(&handle);

            // Background: check GitHub for a newer signed release; the UI surfaces it via
            // the `update://available` event (silent if offline / no release yet).
            {
                let h = handle.clone();
                tauri::async_runtime::spawn(async move { update::check_and_emit(&h).await; });
            }
            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::Focused(false) if window.label() == "menu" => {
                let _ = window.hide();
            }
            // Persist the widget position as the user drags it, so a reboot restores the
            // last placement instead of the default corner.
            WindowEvent::Moved(pos) if window.label() == "widget" => {
                if matches!(window.is_visible(), Ok(true)) {
                    config::save_widget_pos(window.app_handle(), pos.x, pos.y);
                }
            }
            WindowEvent::CloseRequested { api, .. }
                if matches!(window.label(), "settings" | "stats" | "news") =>
            {
                api.prevent_close();
                let _ = window.hide();
            }
            // Login: hide instead of destroy so an in-flight cookies_for_url poll on the
            // UI thread cannot deadlock with webview teardown; also stop the watcher.
            WindowEvent::CloseRequested { api, .. } if window.label() == "login" => {
                api.prevent_close();
                if let Some(state) = window.app_handle().try_state::<AppState>() {
                    state.login_watching.store(false, Ordering::SeqCst);
                }
                let _ = window.hide();
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_usage,
            commands::refresh_now,
            commands::get_history,
            commands::get_settings,
            commands::set_settings,
            commands::set_autostart,
            commands::get_autostart,
            commands::check_for_update,
            commands::install_update,
            commands::get_update_state,
            commands::open_news_window,
            commands::get_changelog,
            commands::get_session_status,
            commands::open_login_window,
            commands::capture_session,
            commands::clear_session,
            commands::open_settings_window,
            commands::open_stats_window,
            commands::toggle_widget,
            commands::quit_app,
            commands::set_theme,
            commands::get_effective_locale,
            commands::set_always_on_top,
            commands::set_move_lock,
            commands::set_widget_opacity,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
