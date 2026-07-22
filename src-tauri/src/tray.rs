//! System tray: a fixed app icon, a usage tooltip, and click routing.
//! Left single-click toggles the widget, double-click opens stats, right-click shows
//! the custom themed menu. No native menu is attached (we render our own, themed).

use std::sync::atomic::Ordering;
use std::time::Duration;

use tauri::image::Image;
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

use crate::api::UsageSnapshot;
use crate::state::AppState;
use crate::{i18n, windows};

/// A left Click arriving this soon after a DoubleClick is the trailing Click that Windows
/// emits for a double-click (order: Click, DoubleClick, Click); it must not schedule a toggle.
const DOUBLE_TRAILING_MS: u64 = 250;
/// Margin added to the system double-click time so a pending single-click always resolves
/// after the DoubleClick would have fired (otherwise the widget toggles when opening stats).
const SINGLE_CLICK_MARGIN_MS: u64 = 40;

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// The system double-click interval. The single-click debounce waits at least this long so a
/// single click is not mistaken for the first click of a double (which would toggle the widget
/// before the double-click opens stats).
fn double_click_time_ms() -> u64 {
    #[cfg(windows)]
    {
        #[link(name = "user32")]
        extern "system" {
            fn GetDoubleClickTime() -> u32;
        }
        unsafe { GetDoubleClickTime() as u64 }
    }
    #[cfg(not(windows))]
    {
        500
    }
}

/// The tray icon is fixed to the app icon (the gauge logo).
fn app_icon() -> Image<'static> {
    Image::from_bytes(include_bytes!("../icons/32x32.png")).expect("valid app icon PNG")
}

pub fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let tray = TrayIconBuilder::with_id("main")
        .icon(app_icon())
        .tooltip("SessionMeter")
        .on_tray_icon_event(handle_tray_event)
        .build(app)?;
    if let Some(state) = app.try_state::<AppState>() {
        *state.tray.lock().unwrap() = Some(tray);
    }
    Ok(())
}

fn handle_tray_event(tray: &TrayIcon, event: TrayIconEvent) {
    let app = tray.app_handle();
    tauri_plugin_positioner::on_tray_event(app, &event);
    match event {
        TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } => schedule_single_click(app),
        TrayIconEvent::DoubleClick {
            button: MouseButton::Left,
            ..
        } => {
            if let Some(state) = app.try_state::<AppState>() {
                // Invalidate the pending single-click and record the time so the trailing
                // Click of the double-click cannot toggle the widget.
                state.click_gen.fetch_add(1, Ordering::SeqCst);
                state.last_double_ms.store(now_ms(), Ordering::SeqCst);
            }
            windows::open_stats(app);
        }
        TrayIconEvent::Click {
            button: MouseButton::Right,
            button_state: MouseButtonState::Up,
            position,
            ..
        } => windows::show_menu_at(app, position.x, position.y),
        _ => {}
    }
}

/// Debounce a left click: wait briefly and, unless a double-click bumped the counter,
/// toggle the widget.
fn schedule_single_click(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    // Ignore the trailing Click of a double-click so it cannot schedule a widget toggle.
    if now_ms().saturating_sub(state.last_double_ms.load(Ordering::SeqCst)) < DOUBLE_TRAILING_MS {
        return;
    }
    let g = state.click_gen.fetch_add(1, Ordering::SeqCst) + 1;
    let delay = double_click_time_ms() + SINGLE_CLICK_MARGIN_MS;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(delay)).await;
        if let Some(state) = app.try_state::<AppState>() {
            // A DoubleClick (open stats) bumps click_gen, cancelling this pending toggle.
            if state.click_gen.load(Ordering::SeqCst) == g {
                windows::toggle_widget(&app);
            }
        }
    });
}

/// Build the tray tooltip across all services: a header per service, then up to two headline
/// buckets each with remaining% and the time until reset.
fn build_tooltip(
    map: &std::collections::HashMap<String, UsageSnapshot>,
    loc: &str,
    settings: &crate::config::Settings,
) -> String {
    let mut lines = Vec::new();
    for id in crate::service::all() {
        let Some(s) = map.get(*id) else { continue };
        if s.status != "ok" {
            continue;
        }
        lines.push(crate::service::display_name(id).to_string());
        // Antigravity's snapshot always carries all four buckets (two model groups, fixed
        // Gemini-first sort order - see antigravity.rs), so a plain "first two" would ignore
        // the widget's own Gemini/3p headline toggle. Pick the buckets for whichever group
        // the widget is set to show, so the tray tooltip and the widget agree.
        let headline: Vec<&crate::api::Bucket> = if *id == crate::service::ANTIGRAVITY_IDE {
            let prefix = if settings.widget(id).headline_group == "3p" {
                "3p-"
            } else {
                "gemini-"
            };
            let picked: Vec<&crate::api::Bucket> =
                s.buckets.iter().filter(|b| b.key.starts_with(prefix)).collect();
            if picked.is_empty() {
                s.buckets.iter().take(2).collect()
            } else {
                picked
            }
        } else {
            s.buckets.iter().take(2).collect()
        };
        for b in headline {
            let label = i18n::bucket_label(loc, &b.key, &b.label);
            let cd = i18n::fmt_countdown(loc, &b.resets_at);
            lines.push(i18n::tooltip_line(loc, &label, b.remaining, &cd));
        }
    }
    if lines.is_empty() {
        i18n::tooltip_signed_out(loc).to_string()
    } else {
        lines.join("\n")
    }
}

/// Refresh the tray tooltip from the latest snapshots (the icon stays the fixed app icon).
pub fn update_tray(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let settings = state.settings.lock().unwrap().clone();
    let loc = i18n::effective_locale(&settings.language);
    let map = state.last_snapshot.lock().unwrap().clone();
    let tooltip = build_tooltip(&map, loc, &settings);
    let guard = state.tray.lock().unwrap();
    if let Some(t) = guard.as_ref() {
        let _ = t.set_tooltip(Some(&tooltip));
    }
}
