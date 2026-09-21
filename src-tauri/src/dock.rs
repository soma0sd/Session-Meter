//! Widget docking: the docked services are drawn as cells of ONE frameless window
//! (`windows::DOCK_LABEL`, rendered by `Widget.svelte` in its group mode) instead of each
//! keeping a window of its own. The window is an ordinary widget window as far as the OS is
//! concerned - one native drag moves the whole group, the cells line up because they share a
//! CSS grid, and there is no cross-window layout to keep in step.
//!
//! `Settings.dock` (config.rs) holds the on/off toggle, column count, placement order and the
//! group window's own opacity / always-on-top / move-lock. The window's screen position lives
//! in window.json under the dock anchor key (`config::save_dock_anchor`), like every other
//! widget position, so a drag never rewrites settings.json.
//!
//! This module only decides *who* is in the group; showing, placing and hiding the window is
//! `windows.rs`'s job, same as for the per-service widgets.

use tauri::{AppHandle, Emitter, Manager};

use crate::config;
use crate::state::AppState;

/// True if docking is on and `service` is one of the docked members (regardless of whether
/// it is logged in or currently shown).
pub fn is_docked(app: &AppHandle, service: &str) -> bool {
    let Some(state) = app.try_state::<AppState>() else {
        return false;
    };
    let s = state.settings.lock().unwrap();
    s.dock.enabled && s.dock.order.iter().any(|id| id == service)
}

/// The services the docked window should show right now, in placement order: docking is on,
/// the service is in `order`, it is one of `candidates` (the services that have a widget at
/// all - logged in, or Claude), and the user has not hidden its widget. An empty list means
/// the docked window has nothing to show and stays hidden.
pub fn members(app: &AppHandle, candidates: &[String]) -> Vec<String> {
    let Some(state) = app.try_state::<AppState>() else {
        return Vec::new();
    };
    let s = state.settings.lock().unwrap();
    if !s.dock.enabled {
        return Vec::new();
    }
    s.dock
        .order
        .iter()
        .filter(|id| candidates.iter().any(|c| c == *id) && s.widget(id).visible)
        .cloned()
        .collect()
}

/// Recompute the member list, remember it, and broadcast `dock://members` when it changed.
/// The docked window pulls the list once on mount (`get_dock_members`) and follows this event
/// afterwards. Returns the current list either way.
pub fn sync(app: &AppHandle, candidates: &[String]) -> Vec<String> {
    let next = members(app, candidates);
    let Some(state) = app.try_state::<AppState>() else {
        return next;
    };
    let changed = {
        let mut cur = state.dock_members.lock().unwrap();
        if *cur == next {
            false
        } else {
            *cur = next.clone();
            true
        }
    };
    if changed {
        let _ = app.emit("dock://members", &next);
    }
    next
}

/// The member list as last computed by `sync`.
pub fn current_members(app: &AppHandle) -> Vec<String> {
    app.try_state::<AppState>()
        .map(|s| s.dock_members.lock().unwrap().clone())
        .unwrap_or_default()
}

/// Called whenever a service becomes a widget candidate (startup, a login while the app is
/// running). If docking is on and the service is not yet in `order`, append it (a one-off;
/// the user can reorder afterwards from the Widget Style window's Placement tab).
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
        // Broadcast so the Placement tab (and the docked window's column/order view) picks the
        // new member up right away instead of on the next unrelated settings change.
        let _ = app.emit("settings://changed", &snap);
    }
}
