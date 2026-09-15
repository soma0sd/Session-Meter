//! Widget size unification: while grid docking is on, every docked widget window is sized to
//! the largest natural panel among them, so the tiles line up instead of each hugging its own
//! content. Antigravity is normally the widest (its "Claude/GPT weekly" bucket labels are
//! `white-space: nowrap`), which is what made the grid look ragged.
//!
//! Rust owns the maximum because a webview only knows its own size, and only Rust knows which
//! widgets are visible and docked right now. The unit is **logical (CSS) px**: the value ends
//! up as a CSS `min-width`/`min-height`, so keeping it in CSS px avoids a physical/logical
//! round trip through `devicePixelRatio`. Note the contrast with `AppState::widget_base_sizes`,
//! which is physical px because the docking grid stacks physical rectangles.
//!
//! The unified size is derived only from *natural* sizes (`set_widget_natural_size`), never
//! from the applied ones. If the applied size fed back in, the maximum could never come back
//! down: hiding the widest widget would leave every other one stuck at its width forever.

use tauri::{AppHandle, Emitter, Manager};

use crate::state::AppState;

/// Same value as `MAX_W` in Widget.svelte. A wider uniform width cannot be honoured anyway -
/// the panel's own `max-width` and `fitWindow`'s window clamp both stop at 360 - and would
/// only clip the panel's right border.
const MAX_UNIFORM_W: i32 = 360;

/// Panel size every docked widget grows to, in logical px. `(0, 0)` means "no minimum":
/// docking is off, or nothing has reported a size yet.
#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct UniformSize {
    pub width: i32,
    pub height: i32,
}

impl UniformSize {
    const NONE: Self = Self {
        width: 0,
        height: 0,
    };
}

/// Largest natural size among the widgets that are docked *and* visible right now.
///
/// Membership comes from `dock::active_members` so the unified set is exactly the set the grid
/// packs. Filtering on "visible now" rather than "has ever reported a size" is what lets the
/// maximum shrink again: a stale entry for a logged-out service would otherwise pin every
/// remaining widget to a size nothing on screen needs.
fn compute(app: &AppHandle, state: &AppState) -> UniformSize {
    let cfg = state.settings.lock().unwrap().dock.clone();
    if !cfg.enabled {
        return UniformSize::NONE;
    }
    let members = crate::dock::active_members(app, &cfg);
    if members.is_empty() {
        return UniformSize::NONE;
    }
    let natural = state.widget_natural_sizes.lock().unwrap();
    let mut out = UniformSize::NONE;
    for id in &members {
        let Some(&(w, h)) = natural.get(id) else {
            continue;
        };
        out.width = out.width.max(w.min(MAX_UNIFORM_W));
        out.height = out.height.max(h);
    }
    out
}

/// The unified size as last broadcast. A widget pulls this once on mount: the broadcast below
/// only fires on change, so a window created later (a service logged in while the app runs)
/// would otherwise never learn the size the others are already using.
pub fn current(app: &AppHandle) -> UniformSize {
    let Some(state) = app.try_state::<AppState>() else {
        return UniformSize::NONE;
    };
    let (width, height) = *state.widget_uniform_size.lock().unwrap();
    UniformSize { width, height }
}

/// Recompute and broadcast only when the value actually changed. Cheap and idempotent, so it
/// is safe to call from every path that changes which widgets are visible or docked - the same
/// places that already call `dock::apply_layout`.
pub fn recompute(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let next = compute(app, &state);
    {
        let mut cur = state.widget_uniform_size.lock().unwrap();
        if *cur == (next.width, next.height) {
            return;
        }
        *cur = (next.width, next.height);
    }
    let _ = app.emit("widget://uniform-size", next);
}
