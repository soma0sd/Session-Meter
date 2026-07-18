//! Settings persistence (tauri-plugin-store) and the session secret.
//! The claude.ai cookie is a credential, kept in a dedicated per-user file
//! (`session.dat`, see SESSION_FILE below), separate from the settings store.
//! On Windows the file is encrypted at rest with DPAPI (per-user scope); on other
//! platforms it is a user-scoped plaintext file. See the SESSION_FILE comment.

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

// The session cookie is stored as a file in the per-user app data dir. The OS keyring
// (Windows Credential Manager) caps a credential blob at ~2560 bytes, which the full
// claude.ai cookie string (Cloudflare + session cookies) exceeds, so keyring is unusable.
// On Windows the bytes are encrypted at rest with DPAPI (per-user scope); on other
// platforms the file is user-scoped plaintext (best-effort).
const SESSION_FILE: &str = "session.dat";

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

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Settings {
    pub theme: String,    // light | dark | system
    pub language: String, // auto | ko | en
    pub widget_opacity: f64,
    pub refresh_interval_min: u64,
    pub always_on_top: bool,
    pub move_lock: bool,
    pub tray_display: String,  // remaining | used
    pub widget_layout: String, // detailed | compact
    pub widget_visible: bool,  // desired widget visibility (watchdog keeps the window in sync)
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
            widget_opacity: 0.9,
            refresh_interval_min: 5,
            always_on_top: true,
            move_lock: false,
            tray_display: "remaining".to_string(),
            widget_layout: "detailed".to_string(),
            widget_visible: true,
            notify: NotifySettings::default(),
            history_retention_days: 30,
            org_name: String::new(),
            account_email: String::new(),
        }
    }
}

fn data_dir(app: &AppHandle) -> Option<PathBuf> {
    let base = app.path().app_data_dir().ok()?;
    // Debug builds keep their settings/window state in a subfolder so running `tauri dev`
    // never clears or overwrites the installed release app's data in the same dir. (The
    // session cookie is intentionally left shared so a dev run stays signed in.)
    #[cfg(debug_assertions)]
    let base = base.join("dev");
    Some(base)
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
    if legacy.is_some() {
        migrate_widget_pos(app, &dir, &v);
    }
    match serde_json::from_value::<Settings>(legacy.unwrap_or(v)) {
        Ok(s) => s,
        Err(_) => {
            // Valid JSON but the wrong shape (e.g. a hand-edit with a mistyped field): back
            // it up rather than silently discarding the user's settings.
            backup_corrupt(&path, &dir);
            Settings::default()
        }
    }
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
        save_widget_pos(app, x as i32, y as i32);
    }
}

pub fn save(app: &AppHandle, settings: &Settings) -> Result<(), AppError> {
    let dir = data_dir(app).ok_or_else(|| AppError::Other("no app data dir".to_string()))?;
    write_json_atomic(&dir.join(SETTINGS_FILE), settings)
}

// --- widget position (its own file, so position churn never rewrites settings.json) ---

pub fn load_widget_pos(app: &AppHandle) -> Option<(i32, i32)> {
    let v: serde_json::Value = read_json_file(&data_dir(app)?.join(WINDOW_FILE))?;
    let x = v.get("x")?.as_i64()? as i32;
    let y = v.get("y")?.as_i64()? as i32;
    Some((x, y))
}

pub fn save_widget_pos(app: &AppHandle, x: i32, y: i32) {
    if let Some(dir) = data_dir(app) {
        let _ = write_json_atomic(&dir.join(WINDOW_FILE), &serde_json::json!({ "x": x, "y": y }));
    }
}

// --- session cookie (user-scoped file; DPAPI-encrypted on Windows) ---

fn session_path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join(SESSION_FILE))
}

pub fn save_cookie(app: &AppHandle, cookie: &str) -> Result<(), AppError> {
    let path = session_path(app).ok_or_else(|| AppError::Other("no app data dir".to_string()))?;
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let bytes = encrypt_secret(cookie.as_bytes())?;
    std::fs::write(&path, bytes).map_err(|e| AppError::Other(e.to_string()))
}

pub fn load_cookie(app: &AppHandle) -> Option<String> {
    let raw = std::fs::read(session_path(app)?).ok()?;
    if raw.is_empty() {
        return None;
    }
    // A blob that will not decrypt (legacy plaintext, or copied from another user/machine)
    // is treated as "no session" so the app prompts a fresh login rather than trusting it.
    let plain = decrypt_secret(&raw)?;
    String::from_utf8(plain).ok().filter(|s| !s.trim().is_empty())
}

pub fn clear_cookie(app: &AppHandle) -> Result<(), AppError> {
    if let Some(path) = session_path(app) {
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
