//! Widget grid docking: several service widgets snapped into a grid that move together
//! when any one of them is dragged. `Settings.dock` (config.rs) holds the on/off toggle,
//! column count, and placement order; the group's on-screen anchor lives in window.json
//! (see `config::save_dock_anchor`/`load_dock_anchor`) since it churns every drag frame and
//! must never share a file with (or trigger a rewrite of) settings.json.
//!
//! Layout: `order` is packed row-major into `columns` columns. Each column's width is the
//! live `outer_size()` max width of the widgets it holds; each row's height is the max
//! height of the widgets in that row. Tiles sit flush at their cell's top-left corner (no
//! reflow/centering for uneven sizes - kept simple, matches the confirmed design).

use std::sync::atomic::Ordering;

use tauri::{AppHandle, Manager, PhysicalPosition};

use crate::config::{self, DockConfig};
use crate::state::AppState;
use crate::windows::widget_label;

/// Coordinate convergence tolerance (px) for telling a relayout echo apart from a real
/// external move (DPI rounding can be off by a pixel or two).
const DOCK_EPS: i32 = 2;
/// Used when a widget window's `outer_size()` cannot be read yet (e.g. just created).
const FALLBACK_SIZE: (i32, i32) = (252, 150);

/// True if docking is on and `service` is one of the docked members (regardless of whether
/// its window is currently visible).
pub fn is_docked(app: &AppHandle, service: &str) -> bool {
    let Some(state) = app.try_state::<AppState>() else {
        return false;
    };
    let s = state.settings.lock().unwrap();
    s.dock.enabled && s.dock.order.iter().any(|id| id == service)
}

/// The docked members that currently have a visible widget window, in `order`. Hidden or
/// unknown ids are filtered out here so the row-major pack always re-flows around gaps.
fn active_members(app: &AppHandle, cfg: &DockConfig) -> Vec<String> {
    cfg.order
        .iter()
        .filter(|&id| {
            app.get_webview_window(&widget_label(id))
                .map(|w| matches!(w.is_visible(), Ok(true)))
                .unwrap_or(false)
        })
        .cloned()
        .collect()
}

/// A service widget's live outer size, or the fallback if the window is missing/unreadable.
fn live_size(app: &AppHandle, service: &str) -> (i32, i32) {
    app.get_webview_window(&widget_label(service))
        .and_then(|w| w.outer_size().ok())
        .map(|s| (s.width as i32, s.height as i32))
        .unwrap_or(FALLBACK_SIZE)
}

/// Row-major pack of `members` into `cfg.columns` columns: column width = that column's
/// widest member, row height = that row's tallest member, tiles flush at the cell's
/// top-left. Returns each member's absolute physical position (anchor + its offset).
fn pack(app: &AppHandle, cfg: &DockConfig, members: &[String]) -> Vec<(String, i32, i32)> {
    if members.is_empty() {
        return Vec::new();
    }
    let columns = (cfg.columns.max(1) as usize).min(members.len().max(1));
    let sizes: Vec<(i32, i32)> = members.iter().map(|id| live_size(app, id)).collect();
    let rows = (members.len() + columns - 1) / columns;

    let mut col_widths = vec![0i32; columns];
    let mut row_heights = vec![0i32; rows];
    for (i, &(w, h)) in sizes.iter().enumerate() {
        let (row, col) = (i / columns, i % columns);
        col_widths[col] = col_widths[col].max(w);
        row_heights[row] = row_heights[row].max(h);
    }

    let mut col_x = vec![0i32; columns];
    for c in 1..columns {
        col_x[c] = col_x[c - 1] + col_widths[c - 1];
    }
    let mut row_y = vec![0i32; rows];
    for r in 1..rows {
        row_y[r] = row_y[r - 1] + row_heights[r - 1];
    }

    members
        .iter()
        .enumerate()
        .map(|(i, id)| {
            let (row, col) = (i / columns, i % columns);
            (id.clone(), cfg.anchor_x + col_x[col], cfg.anchor_y + row_y[row])
        })
        .collect()
}

/// The position `service` should currently occupy under the live config (None if it is not
/// an active docked member right now).
pub fn expected_position(app: &AppHandle, service: &str) -> Option<(i32, i32)> {
    let state = app.try_state::<AppState>()?;
    let cfg = state.settings.lock().unwrap().dock.clone();
    if !cfg.enabled {
        return None;
    }
    let members = active_members(app, &cfg);
    pack(app, &cfg, &members)
        .into_iter()
        .find(|(id, ..)| id == service)
        .map(|(_, x, y)| (x, y))
}

/// RAII guard for the reentrancy flag: guarantees the flag clears on every return path out
/// of `apply_layout`, however it exits.
struct RelayoutGuard<'a>(&'a std::sync::atomic::AtomicBool);
impl<'a> Drop for RelayoutGuard<'a> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// Recompute the pack and move every active docked widget to its slot. Safe to call from
/// the watchdog, a resize notification, a visibility change, or a group drag tick - the
/// reentrancy flag makes overlapping calls (and the `WindowEvent::Moved` echoes they cause)
/// a no-op instead of a feedback loop.
pub fn apply_layout(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    if state.dock_relayout_in_progress.swap(true, Ordering::SeqCst) {
        return; // already relaying out (watchdog/drag overlap) - skip, not queue
    }
    let _guard = RelayoutGuard(&state.dock_relayout_in_progress);

    let mut cfg = state.settings.lock().unwrap().dock.clone();
    if !cfg.enabled {
        return;
    }
    let members = active_members(app, &cfg);
    if members.is_empty() {
        return;
    }

    if let Some(ref_win) = app.get_webview_window(&widget_label(&members[0])) {
        let columns = (cfg.columns.max(1) as usize).min(members.len().max(1));
        let rows = (members.len() + columns - 1) / columns;

        let sizes: Vec<(i32, i32)> = members.iter().map(|id| live_size(app, id)).collect();
        let mut col_widths = vec![0i32; columns];
        let mut row_heights = vec![0i32; rows];
        for (i, &(w, h)) in sizes.iter().enumerate() {
            let (row, col) = (i / columns, i % columns);
            col_widths[col] = col_widths[col].max(w);
            row_heights[row] = row_heights[row].max(h);
        }
        let total_w: i32 = col_widths.iter().sum();
        let total_h: i32 = row_heights.iter().sum();

        let (clamped_anchor_x, clamped_anchor_y, moved) =
            crate::windows::clamp_rect_to_screen(&ref_win, cfg.anchor_x, cfg.anchor_y, total_w, total_h);

        if moved {
            cfg.anchor_x = clamped_anchor_x;
            cfg.anchor_y = clamped_anchor_y;
            let mut settings = state.settings.lock().unwrap();
            settings.dock.anchor_x = clamped_anchor_x;
            settings.dock.anchor_y = clamped_anchor_y;
            config::save_dock_anchor(app, clamped_anchor_x, clamped_anchor_y);
        }
    }

    for (id, x, y) in pack(app, &cfg, &members) {
        if let Some(win) = app.get_webview_window(&widget_label(&id)) {
            if matches!(win.is_minimized(), Ok(true)) {
                let _ = win.unminimize();
            }
            let _ = win.set_position(PhysicalPosition::new(x, y));
        }
    }
}

/// Update the docked group's anchor in memory and re-pack. `service`/`x`/`y` describe where
/// the dragged widget wants to be *now* (its live physical position); the anchor is
/// recovered by subtracting that widget's offset within the current pack.
///
/// Deliberately does NOT touch disk: this runs on every rAF-throttled pointer-move frame
/// while a group drag is in progress (so up to ~60 times/sec), and a full file read+write+
/// rename on every frame (`config::save_dock_anchor` used to be called right here) was
/// visible as drag stutter/lag. `move_group_end` persists the final anchor once the drag
/// actually finishes; if the app exits mid-drag without ever calling it, the in-memory anchor
/// (and thus this frame's position) is simply lost, same as any other unsaved-at-crash state.
pub fn move_group_to(app: &AppHandle, service: &str, x: i32, y: i32) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let cfg = state.settings.lock().unwrap().dock.clone();
    if !cfg.enabled {
        return;
    }
    let members = active_members(app, &cfg);
    let Some((_, off_x, off_y)) = pack(app, &cfg, &members)
        .into_iter()
        .find(|(id, ..)| id == service)
    else {
        return;
    };
    let new_anchor_x = x - (off_x - cfg.anchor_x);
    let new_anchor_y = y - (off_y - cfg.anchor_y);
    {
        let mut settings = state.settings.lock().unwrap();
        settings.dock.anchor_x = new_anchor_x;
        settings.dock.anchor_y = new_anchor_y;
    }
    apply_layout(app);
}

/// Persist the current anchor once a group drag ends (see `move_group_to`'s doc comment for
/// why the anchor isn't written to disk on every frame during the drag itself).
pub fn move_group_end(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let (x, y) = {
        let settings = state.settings.lock().unwrap();
        (settings.dock.anchor_x, settings.dock.anchor_y)
    };
    config::save_dock_anchor(app, x, y);
}

/// `WindowEvent::Moved` handler entry point for a docked widget (only called when the
/// reentrancy flag was already observed clear, i.e. this did not originate from our own
/// `apply_layout`). Tells a late-arriving echo of our own relayout apart from a real
/// external move (OS snap, another tool, etc.): within `DOCK_EPS` of where the widget is
/// supposed to be, it is an echo and ignored; otherwise it is drift and gets corrected by
/// re-running the layout (the anchor itself is never touched by drift correction).
pub fn on_widget_moved(app: &AppHandle, service: &str, x: i32, y: i32) {
    match expected_position(app, service) {
        Some((ex, ey)) if (x - ex).abs() <= DOCK_EPS && (y - ey).abs() <= DOCK_EPS => {
            // Echo of our own relayout arriving late - nothing to do.
        }
        Some(_) => apply_layout(app),
        None => {}
    }
}

/// Called when a service's widget window is (re)created / becomes a candidate member. If
/// docking is on and the service is not yet in `order`, append it (a one-off; the user can
/// reorder afterwards from the Widget Style window's Placement tab).
pub fn on_membership_changed(app: &AppHandle, service: &str) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let changed = {
        let mut settings = state.settings.lock().unwrap();
        if !settings.dock.enabled || settings.dock.order.iter().any(|id| id == service) {
            false
        } else {
            settings.dock.order.push(service.to_string());
            true
        }
    };
    if changed {
        let snap = state.settings.lock().unwrap().clone();
        let _ = config::save(app, &snap);
    }
}

/// Poller watchdog tick (every `CHECK_STEP_SECS`, independent of the refresh interval):
/// re-run the layout so drift gets corrected quickly even when the user set a long poll
/// interval. No-op (cheap) when docking is off.
pub fn watchdog_tick(app: &AppHandle) {
    let enabled = app
        .try_state::<AppState>()
        .map(|s| s.settings.lock().unwrap().dock.enabled)
        .unwrap_or(false);
    if enabled {
        apply_layout(app);
    }
}
