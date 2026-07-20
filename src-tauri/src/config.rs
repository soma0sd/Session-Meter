//! Settings persistence and the session secret.
//! Settings persist via direct atomic file writes (NOT tauri-plugin-store): see
//! `write_json_atomic`. The claude.ai cookie is a credential, kept in a dedicated per-user
//! file (`session.dat`, see SESSION_FILE below), separate from the settings files.
//! On Windows the file is encrypted at rest with DPAPI (per-user scope); on other
//! platforms it is a user-scoped plaintext file. See the SESSION_FILE comment.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::error::AppError;

// Persisted with direct atomic file writes (write a temp file, then rename over the target)
// so a hard process exit during an update install can never leave a half-written or
// truncated file. The widget position lives in its OWN file so the frequently-written
// position never shares a file with (and so can never clobber) the settings.
const SETTINGS_FILE: &str = "settings.json";
const WINDOW_FILE: &str = "window.json";

// Each service's session credential is stored as a file in the per-user app data dir. The
// OS keyring (Windows Credential Manager) caps a credential blob at ~2560 bytes, which the
// full claude.ai cookie string (Cloudflare + session cookies) exceeds, so keyring is
// unusable. On Windows the bytes are encrypted at rest with DPAPI (per-user scope); on
// other platforms the file is user-scoped plaintext (best-effort). Claude keeps the legacy
// `session.dat` filename so existing sessions survive the multi-service upgrade (no re-login).
fn session_file(service: &str) -> String {
    if service == "claude" {
        "session.dat".to_string()
    } else {
        format!("session.{service}.dat")
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct NotifySettings {
    pub enabled: bool,
    /// Alert when the 5-hour session's used% reaches this.
    pub session_threshold: u8,
    /// Alert when the weekly window's used% reaches this.
    pub weekly_threshold: u8,
    pub on_reset: bool,
}

impl Default for NotifySettings {
    fn default() -> Self {
        Self {
            enabled: true,
            session_threshold: 80,
            weekly_threshold: 80,
            on_reset: true,
        }
    }
}

/// Per-service widget appearance + behavior. Stored per service id in `Settings::widgets`,
/// configured from the "Widget style" window.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct WidgetConfig {
    pub style: String,        // style catalog id, e.g. "focus-slim-detailed"
    pub display_mode: String, // remaining | used (the widget's number display mode)
    pub opacity: f64,
    pub always_on_top: bool,
    pub move_lock: bool,
    pub visible: bool, // desired widget visibility (a watchdog keeps the window in sync)
}

impl Default for WidgetConfig {
    fn default() -> Self {
        Self {
            style: "focus-slim-detailed".to_string(),
            display_mode: "remaining".to_string(),
            opacity: 0.9,
            always_on_top: true,
            move_lock: false,
            visible: true,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Settings {
    pub theme: String,    // light | dark | system
    pub language: String, // auto | ko | en
    pub refresh_interval_min: u64,
    /// Per-service widget config, keyed by service id ("claude", "gemini", ...).
    pub widgets: HashMap<String, WidgetConfig>,
    pub notify: NotifySettings,
    pub history_retention_days: u32,
    pub org_name: String,
    pub account_email: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "system".to_string(),
            language: "auto".to_string(),
            refresh_interval_min: 5,
            widgets: HashMap::new(),
            notify: NotifySettings::default(),
            history_retention_days: 30,
            org_name: String::new(),
            account_email: String::new(),
        }
    }
}

impl Settings {
    /// The widget config for a service, falling back to defaults when unset.
    pub fn widget(&self, service: &str) -> WidgetConfig {
        self.widgets.get(service).cloned().unwrap_or_default()
    }
}

fn data_dir(app: &AppHandle) -> Option<PathBuf> {
    // Settings/window state live in the app data dir alongside the session and history.
    // Debug and release builds share it (there is no longer a per-run reset), so a dev run
    // sees and keeps the same settings as the installed app instead of appearing to reset.
    app.path().app_data_dir().ok()
}

/// Atomically write JSON to `path`: serialize to a sibling temp file, then rename over the
/// target. `fs::rename` replaces the destination in a single step (MoveFileEx on Windows),
/// so a crash or hard process exit leaves either the old or the new complete file, never a
/// half-written one.
fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), AppError> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| AppError::Other(e.to_string()))?;
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    // A unique temp name per write so concurrent savers (the UI thread and the poller
    // thread) never share a temp file: each writes its own complete file, then the atomic
    // rename publishes it (last writer wins, but the target is always a whole valid file).
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp = path.with_extension(format!("tmp{seq}"));
    fs::write(&tmp, &bytes).map_err(|e| AppError::Other(e.to_string()))?;
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        AppError::Other(e.to_string())
    })
}

fn read_json_file<T: DeserializeOwned>(path: &Path) -> Option<T> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

pub fn load(app: &AppHandle) -> Settings {
    let Some(dir) = data_dir(app) else {
        return Settings::default();
    };
    let path = dir.join(SETTINGS_FILE);
    let Ok(bytes) = fs::read(&path) else {
        return Settings::default(); // no file yet -> first run
    };
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Settings::default();
    }
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        // Not valid JSON (e.g. a truncated write): keep a copy for recovery instead of
        // silently replacing it with defaults, which the next save would overwrite for good.
        backup_corrupt(&path, &dir);
        return Settings::default();
    };
    // Current format is the flat Settings object; also accept (and migrate the widget
    // position out of) the legacy shape { "settings": {..}, "widget_pos": {..} }.
    let legacy = v.get("settings").filter(|x| x.is_object()).cloned();
    let was_legacy = legacy.is_some();
    if was_legacy {
        migrate_widget_pos(app, &dir, &v);
    }
    let source = legacy.unwrap_or(v);
    match serde_json::from_value::<Settings>(source.clone()) {
        Ok(mut s) => {
            let seeded = migrate_widgets(&mut s, &source);
            // Rewrite the file once when it was a pre-0.4 shape (nested settings, or flat widget
            // fields with no `widgets` map): otherwise the widget config is re-derived from the
            // legacy fields on every launch/update instead of being saved, so a user's later
            // style/interval change (or a stale multi-window save) appears to reset. Persisting
            // the upgraded settings locks the current format in.
            if was_legacy || seeded {
                let _ = save(app, &s);
            }
            s
        }
        Err(_) => {
            // Valid JSON but the wrong shape (e.g. a hand-edit with a mistyped field): back
            // it up rather than silently discarding the user's settings.
            backup_corrupt(&path, &dir);
            Settings::default()
        }
    }
}

/// One-time upgrade: seed the Claude widget config from the pre-0.4 flat widget fields (or
/// defaults) so the widget keeps its style/opacity/visibility across the multi-service update.
fn migrate_widgets(settings: &mut Settings, source: &serde_json::Value) -> bool {
    if !settings.widgets.is_empty() {
        return false;
    }
    let mut wc = WidgetConfig::default();
    if let Some(x) = source.get("widget_style").and_then(|x| x.as_str()) {
        wc.style = x.to_string();
    } else if let Some(x) = source.get("widget_layout").and_then(|x| x.as_str()) {
        // Even older builds stored a detailed/compact layout; map it onto a Focus & Slim style.
        wc.style = if x == "compact" {
            "focus-slim-compact".to_string()
        } else {
            "focus-slim-detailed".to_string()
        };
    }
    if let Some(x) = source.get("tray_display").and_then(|x| x.as_str()) {
        wc.display_mode = x.to_string();
    }
    if let Some(x) = source.get("widget_opacity").and_then(|x| x.as_f64()) {
        wc.opacity = x;
    }
    if let Some(x) = source.get("always_on_top").and_then(|x| x.as_bool()) {
        wc.always_on_top = x;
    }
    if let Some(x) = source.get("move_lock").and_then(|x| x.as_bool()) {
        wc.move_lock = x;
    }
    if let Some(x) = source.get("widget_visible").and_then(|x| x.as_bool()) {
        wc.visible = x;
    }
    settings.widgets.insert("claude".to_string(), wc);
    true
}

fn backup_corrupt(path: &Path, dir: &Path) {
    let _ = fs::rename(path, dir.join("settings.corrupt.json"));
}

/// One-time upgrade: carry the widget position from the pre-0.3.0 combined settings.json
/// into the new window.json when it has not been written yet, so the widget keeps its place
/// instead of jumping back to the default corner after the update.
fn migrate_widget_pos(app: &AppHandle, dir: &Path, v: &serde_json::Value) {
    if dir.join(WINDOW_FILE).exists() {
        return;
    }
    if let (Some(x), Some(y)) = (
        v.pointer("/widget_pos/x").and_then(serde_json::Value::as_i64),
        v.pointer("/widget_pos/y").and_then(serde_json::Value::as_i64),
    ) {
        save_widget_pos(app, "claude", x as i32, y as i32);
    }
}

pub fn save(app: &AppHandle, settings: &Settings) -> Result<(), AppError> {
    let dir = data_dir(app).ok_or_else(|| AppError::Other("no app data dir".to_string()))?;
    write_json_atomic(&dir.join(SETTINGS_FILE), settings)
}

// --- widget position, per service (its own file, so churn never rewrites settings.json) ---

pub fn load_widget_pos(app: &AppHandle, service: &str) -> Option<(i32, i32)> {
    let v: serde_json::Value = read_json_file(&data_dir(app)?.join(WINDOW_FILE))?;
    // New shape: { "claude": {x,y}, ... }. Legacy shape (pre-0.4): a bare { x, y } == Claude.
    let legacy = if service == "claude" && v.get("x").is_some() {
        Some(&v)
    } else {
        None
    };
    let node = v.get(service).or(legacy)?;
    let x = node.get("x")?.as_i64()? as i32;
    let y = node.get("y")?.as_i64()? as i32;
    Some((x, y))
}

pub fn save_widget_pos(app: &AppHandle, service: &str, x: i32, y: i32) {
    let Some(dir) = data_dir(app) else {
        return;
    };
    let path = dir.join(WINDOW_FILE);
    let mut v = read_json_file::<serde_json::Value>(&path).unwrap_or_else(|| serde_json::json!({}));
    if !v.is_object() {
        v = serde_json::json!({});
    }
    v[service] = serde_json::json!({ "x": x, "y": y });
    let _ = write_json_atomic(&path, &v);
}

// --- one-time service rename migration (Antigravity -> Gemini, 0.4.1) ---

/// Carry an upgrading user's Antigravity data over to the new `gemini` service id so they keep
/// their session, history, widget position, and widget settings after the rename. Each step is
/// idempotent (acts only when the old artifact exists and the new one does not / is absent), so
/// running it on every startup is safe. Call before settings/window state are loaded.
pub fn migrate_service_rename(app: &AppHandle) {
    let Some(dir) = data_dir(app) else {
        return;
    };
    // Session credential + usage history: plain per-service file renames.
    rename_if_needed(
        &dir.join("session.antigravity.dat"),
        &dir.join("session.gemini.dat"),
    );
    rename_if_needed(
        &dir.join("history.antigravity.jsonl"),
        &dir.join("history.gemini.jsonl"),
    );
    // Widget position: top-level service key inside window.json.
    let wpath = dir.join(WINDOW_FILE);
    if let Some(mut v) = read_json_file::<serde_json::Value>(&wpath) {
        let changed = v
            .as_object_mut()
            .map(|o| move_map_key(o, "antigravity", "gemini"))
            .unwrap_or(false);
        if changed {
            let _ = write_json_atomic(&wpath, &v);
        }
    }
    // Widget settings: widgets.antigravity -> widgets.gemini inside settings.json.
    let spath = dir.join(SETTINGS_FILE);
    if let Some(mut v) = read_json_file::<serde_json::Value>(&spath) {
        let changed = v
            .get_mut("widgets")
            .and_then(|w| w.as_object_mut())
            .map(|w| move_map_key(w, "antigravity", "gemini"))
            .unwrap_or(false);
        if changed {
            let _ = write_json_atomic(&spath, &v);
        }
    }
}

fn rename_if_needed(from: &Path, to: &Path) {
    if from.exists() && !to.exists() {
        let _ = fs::rename(from, to);
    }
}

/// Move the value under `old` to `new` in a JSON object, only when `old` is present and `new`
/// is not. Returns whether anything changed.
fn move_map_key(obj: &mut serde_json::Map<String, serde_json::Value>, old: &str, new: &str) -> bool {
    if obj.contains_key(old) && !obj.contains_key(new) {
        if let Some(val) = obj.remove(old) {
            obj.insert(new.to_string(), val);
            return true;
        }
    }
    false
}

// --- session credential (per service; user-scoped file; DPAPI-encrypted on Windows) ---

fn session_path(app: &AppHandle, service: &str) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|d| d.join(session_file(service)))
}

pub fn save_cookie(app: &AppHandle, service: &str, cookie: &str) -> Result<(), AppError> {
    let path =
        session_path(app, service).ok_or_else(|| AppError::Other("no app data dir".to_string()))?;
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let bytes = encrypt_secret(cookie.as_bytes())?;
    std::fs::write(&path, bytes).map_err(|e| AppError::Other(e.to_string()))
}

pub fn load_cookie(app: &AppHandle, service: &str) -> Option<String> {
    let raw = std::fs::read(session_path(app, service)?).ok()?;
    if raw.is_empty() {
        return None;
    }
    // A blob that will not decrypt (legacy plaintext, or copied from another user/machine)
    // is treated as "no session" so the app prompts a fresh login rather than trusting it.
    let plain = decrypt_secret(&raw)?;
    String::from_utf8(plain).ok().filter(|s| !s.trim().is_empty())
}

pub fn clear_cookie(app: &AppHandle, service: &str) -> Result<(), AppError> {
    if let Some(path) = session_path(app, service) {
        let _ = std::fs::remove_file(path);
    }
    Ok(())
}

/// Encrypt the session secret for at-rest storage. Windows: DPAPI (per-user scope);
/// other platforms: passthrough (the file stays a user-scoped plaintext file).
#[cfg(windows)]
fn encrypt_secret(plain: &[u8]) -> Result<Vec<u8>, AppError> {
    dpapi::protect(plain).ok_or_else(|| AppError::Other("DPAPI encrypt failed".to_string()))
}
#[cfg(not(windows))]
fn encrypt_secret(plain: &[u8]) -> Result<Vec<u8>, AppError> {
    Ok(plain.to_vec())
}

/// Decrypt a stored session secret; None means the blob is not valid for this user.
#[cfg(windows)]
fn decrypt_secret(raw: &[u8]) -> Option<Vec<u8>> {
    dpapi::unprotect(raw)
}
#[cfg(not(windows))]
fn decrypt_secret(raw: &[u8]) -> Option<Vec<u8>> {
    Some(raw.to_vec())
}

/// Windows DPAPI (per-user data protection) for the session secret at rest.
/// `CryptProtectData`/`CryptUnprotectData` bind the ciphertext to the current user
/// account, so another user (or an offline disk / backup) cannot decrypt it.
#[cfg(windows)]
mod dpapi {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    fn in_blob(data: &[u8]) -> CRYPT_INTEGER_BLOB {
        CRYPT_INTEGER_BLOB {
            cbData: data.len() as u32,
            pbData: data.as_ptr() as *mut u8,
        }
    }

    /// Copy a DPAPI-allocated output blob into an owned Vec, then free it with LocalFree.
    ///
    /// # Safety
    /// `out` must be an output blob from a successful CryptProtectData/CryptUnprotectData call.
    unsafe fn take_out(out: CRYPT_INTEGER_BLOB) -> Vec<u8> {
        let bytes = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        let _ = LocalFree(HLOCAL(out.pbData as *mut core::ffi::c_void));
        bytes
    }

    /// Encrypt `plain` with DPAPI, per-user scope (no CRYPTPROTECT_LOCAL_MACHINE).
    pub fn protect(plain: &[u8]) -> Option<Vec<u8>> {
        let input = in_blob(plain);
        let mut out = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };
        unsafe {
            CryptProtectData(&input, PCWSTR::null(), None, None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut out).ok()?;
            Some(take_out(out))
        }
    }

    /// Decrypt a DPAPI blob. Returns None if it was not encrypted by this user.
    pub fn unprotect(cipher: &[u8]) -> Option<Vec<u8>> {
        let input = in_blob(cipher);
        let mut out = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };
        unsafe {
            CryptUnprotectData(&input, None, None, None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut out).ok()?;
            Some(take_out(out))
        }
    }
}

#[cfg(test)]
mod settings_tests {
    use super::*;

    // A real pre-0.4 (0.3.x) settings.json: flat widget fields, NO `widgets` map.
    const LEGACY: &str = r#"{
      "theme":"system","language":"auto","widget_opacity":0.7,"refresh_interval_min":5,
      "always_on_top":true,"move_lock":false,"tray_display":"remaining","widget_layout":"compact",
      "widget_visible":true,
      "notify":{"enabled":true,"session_threshold":80,"weekly_threshold":80,"on_reset":true},
      "history_retention_days":30,"org_name":"n","account_email":"e"
    }"#;

    // Mimics config::load's parse + migrate (without the AppHandle-bound file IO).
    fn parse_and_migrate(s: &str) -> Settings {
        let v: serde_json::Value = serde_json::from_str(s).expect("valid json");
        let mut set: Settings = serde_json::from_value(v.clone()).expect("parses as Settings");
        migrate_widgets(&mut set, &v);
        set
    }

    #[test]
    fn legacy_seeds_widget_and_keeps_interval() {
        let s = parse_and_migrate(LEGACY);
        assert_eq!(s.refresh_interval_min, 5, "interval kept from legacy");
        let w = s.widgets.get("claude").expect("claude widget seeded");
        assert_eq!(w.style, "focus-slim-compact", "widget_layout=compact -> focus-slim-compact");
        assert!((w.opacity - 0.7).abs() < 1e-9, "opacity carried from legacy");
    }

    #[test]
    fn user_change_round_trips() {
        // Load legacy, apply a user change, save (serialize), reload.
        let mut s = parse_and_migrate(LEGACY);
        let mut w = s.widget("claude");
        w.style = "hex-rings-detailed".to_string();
        s.widgets.insert("claude".to_string(), w);
        s.refresh_interval_min = 1;
        let bytes = serde_json::to_vec_pretty(&s).unwrap();
        let s2 = parse_and_migrate(std::str::from_utf8(&bytes).unwrap());
        assert_eq!(s2.refresh_interval_min, 1, "interval must persist across save/load");
        assert_eq!(
            s2.widgets.get("claude").unwrap().style,
            "hex-rings-detailed",
            "style must persist across save/load"
        );
    }
}

#[cfg(all(test, windows))]
mod tests {
    /// DPAPI must actually round-trip at runtime (not just compile): encrypt then
    /// decrypt recovers the secret, ciphertext differs from plaintext, and a
    /// non-DPAPI blob fails gracefully (None) instead of panicking.
    #[test]
    fn dpapi_roundtrip() {
        let secret = b"sessionKey=sk-live-abc123; cf_clearance=xyz; activitySessionId=1";
        let enc = super::dpapi::protect(secret).expect("DPAPI protect should succeed");
        assert_ne!(enc.as_slice(), &secret[..], "ciphertext must differ from plaintext");
        let dec = super::dpapi::unprotect(&enc).expect("DPAPI unprotect should succeed");
        assert_eq!(dec.as_slice(), &secret[..], "round-trip must recover the secret");
        assert!(
            super::dpapi::unprotect(b"not a valid dpapi blob").is_none(),
            "a non-DPAPI blob must decrypt to None, not panic",
        );
    }
}
