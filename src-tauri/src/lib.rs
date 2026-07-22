mod api;
mod auth;
mod commands;
mod config;
mod error;
mod gemini;
mod gemini_helper;
mod history;
mod i18n;
mod icon;
mod notify;
mod poller;
mod service;
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
    // The WebView2 loader reads this env var and applies the same browser args to EVERY WebView2
    // environment in the process, so there is no second-environment conflict (unlike per-window
    // additional_browser_args). We keep WRY's own default disabled features, additionally disable
    // User-Agent Client Hints (so the Firefox UA spoof on the Gemini login window stays coherent:
    // WebView2 otherwise leaks Edge branding via Sec-CH-UA and Google flags the mismatch), and
    // drop the AutomationControlled blink flag. Must be set before the first webview is created.
    #[cfg(windows)]
    std::env::set_var(
        "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
        "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,UserAgentClientHint \
         --disable-blink-features=AutomationControlled",
    );

    // Gemini helper mode: the main app relaunched this binary to host the Google login/scrape
    // webview in a SEPARATE process (its own UI thread), so a wedged Google page cannot freeze the
    // main app. Run the bare tao+wry helper and exit without building the full app.
    if let Ok(mode) = std::env::var("SM_GEMINI_MODE") {
        gemini_helper::run(&mode);
        return;
    }

    tauri::Builder::default()
        // Register single-instance FIRST so a second launch focuses the existing app.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(win) = app.get_webview_window("settings") {
                let _ = win.show();
                let _ = win.set_focus();
            }
        }))
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
            // One-time: migrate the renamed Antigravity->Gemini service's persisted files
            // (session, history, window position) before anything reads them.
            config::migrate_service_rename(&handle);
            let settings = config::load(&handle);
            app.manage(AppState::new(settings));

            // Self-heal a stale autostart path. `tauri-plugin-autostart` records the launch
            // command from `current_exe()` when autostart is first enabled and never revalidates
            // it. If the exe later moves or is reinstalled elsewhere, the Run-key entry keeps
            // launching the OLD binary on every boot even after an update. (Observed: autostart
            // was enabled while an earlier build ran under a virtualized/sandboxed exe path, so a
            // reboot kept relaunching that stale old version while the real install updated fine.)
            // On startup, if autostart is enabled, re-register so the stored path is rewritten to
            // THIS process's exe (captured by the plugin at init = the real install path).
            heal_autostart(&handle);

            tray::build_tray(&handle)?;

            // Always show the desktop widget on startup (including first run, before
            // sign-in), so it is visible immediately. always_on_top defaults to true.
            windows::show_widget(&handle);

            // First-run onboarding: if there's no saved Claude session, also open Settings and
            // the claude.ai login window so the user can sign in.
            if config::load_cookie(&handle, service::CLAUDE).is_none() {
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

            // Check GitHub for a newer signed release on startup and every 10 minutes; the UI
            // surfaces it via the `update://available` event (silent if offline / no release yet).
            update::start_periodic(&handle);
            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::Focused(false) if window.label() == "menu" => {
                let _ = window.hide();
            }
            // Persist a service widget's position as the user drags it, so a reboot restores
            // the last placement instead of the default corner.
            WindowEvent::Moved(pos)
                if windows::service_from_widget_label(window.label()).is_some() =>
            {
                // Windows parks a minimizing/hiding window at (-32000,-32000) and still
                // reports is_visible()==true, so also reject the sentinel and the minimized
                // state before persisting; otherwise a hide/minimize (e.g. during an update
                // restart) saves that bogus position and the widget returns off-screen.
                if pos.x > -32000
                    && pos.y > -32000
                    && matches!(window.is_visible(), Ok(true))
                    && !matches!(window.is_minimized(), Ok(true))
                {
                    if let Some(service) = windows::service_from_widget_label(window.label()) {
                        config::save_widget_pos(window.app_handle(), &service, pos.x, pos.y);
                    }
                }
            }
            WindowEvent::CloseRequested { api, .. }
                if matches!(window.label(), "settings" | "stats" | "news" | "style") =>
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
            commands::get_services_status,
            commands::open_login_window,
            commands::capture_session,
            commands::clear_session,
            commands::open_settings_window,
            commands::open_stats_window,
            commands::open_style_window,
            commands::toggle_widget,
            commands::quit_app,
            commands::set_theme,
            commands::get_effective_locale,
            commands::set_always_on_top,
            commands::set_move_lock,
            commands::set_widget_opacity,
            commands::set_widget_visible,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Rewrite the autostart Run-key path to the current executable when autostart is enabled, so a
/// stale path (from a moved/reinstalled/sandboxed exe) can't keep launching an old version on
/// boot. Only acts when already enabled, which respects a Task-Manager disable (reflected by
/// `is_enabled()`); `enable()` overwrites the stored value with the plugin's init-time
/// `current_exe()`. Release-only: in dev the current exe is a target/debug path we must not persist.
#[cfg(not(debug_assertions))]
fn heal_autostart(app: &tauri::AppHandle) {
    use tauri_plugin_autostart::ManagerExt;
    let al = app.autolaunch();
    if al.is_enabled().unwrap_or(false) {
        let _ = al.enable();
    }
}

#[cfg(debug_assertions)]
fn heal_autostart(_app: &tauri::AppHandle) {}
