//! Settings persistence (tauri-plugin-store) and the session secret.
//! The claude.ai cookie is a credential, kept in a dedicated per-user file
//! (`session.dat`, see SESSION_FILE below), separate from the settings store.
//! On Windows the file is encrypted at rest with DPAPI (per-user scope); on other
//! platforms it is a user-scoped plaintext file. See the SESSION_FILE comment.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tauri_plugin_store::StoreExt;

use crate::error::AppError;

const STORE_FILE: &str = "settings.json";
const STORE_KEY: &str = "settings";
const POS_KEY: &str = "widget_pos";

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
    pub thresholds: Vec<u8>,
    pub on_reset: bool,
}

impl Default for NotifySettings {
    fn default() -> Self {
        Self {
            enabled: true,
            thresholds: vec![80, 95],
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
    pub tray_display: String, // remaining | used
    pub tray_bucket: String,  // five_hour | weekly
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
            tray_bucket: "five_hour".to_string(),
            notify: NotifySettings::default(),
            history_retention_days: 30,
            org_name: String::new(),
            account_email: String::new(),
        }
    }
}

pub fn load(app: &AppHandle) -> Settings {
    match app.store(STORE_FILE) {
        Ok(store) => store
            .get(STORE_KEY)
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default(),
        Err(_) => Settings::default(),
    }
}

/// Reset persisted settings to defaults (used in debug builds so each dev run starts clean).
pub fn clear_settings(app: &AppHandle) {
    if let Ok(store) = app.store(STORE_FILE) {
        store.delete(STORE_KEY);
        store.delete(POS_KEY);
        let _ = store.save();
    }
}

pub fn save(app: &AppHandle, settings: &Settings) -> Result<(), AppError> {
    let store = app
        .store(STORE_FILE)
        .map_err(|e| AppError::Other(e.to_string()))?;
    let value = serde_json::to_value(settings).map_err(|e| AppError::Other(e.to_string()))?;
    store.set(STORE_KEY, value);
    store.save().map_err(|e| AppError::Other(e.to_string()))?;
    Ok(())
}

// --- widget position (persisted separately so it survives independently of settings) ---

pub fn load_widget_pos(app: &AppHandle) -> Option<(i32, i32)> {
    let store = app.store(STORE_FILE).ok()?;
    let v = store.get(POS_KEY)?;
    let x = v.get("x")?.as_i64()? as i32;
    let y = v.get("y")?.as_i64()? as i32;
    Some((x, y))
}

pub fn save_widget_pos(app: &AppHandle, x: i32, y: i32) {
    if let Ok(store) = app.store(STORE_FILE) {
        store.set(POS_KEY, serde_json::json!({ "x": x, "y": y }));
        let _ = store.save();
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
