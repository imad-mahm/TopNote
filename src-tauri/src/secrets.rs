//! Secrets live in Windows Credential Manager, never in settings.json or the webview.
use crate::error::{msg, Result};

const SERVICE: &str = "topnote";
/// Service name used before the app was renamed; entries are moved over on first read.
const LEGACY_SERVICE: &str = "spotify-miniplayer";

pub fn get(key: &str) -> Option<String> {
    if let Some(v) = keyring::Entry::new(SERVICE, key).ok().and_then(|e| e.get_password().ok()) {
        return Some(v);
    }
    let old = keyring::Entry::new(LEGACY_SERVICE, key).ok()?;
    let v = old.get_password().ok()?;
    if set(key, &v).is_ok() {
        let _ = old.delete_credential();
    }
    Some(v)
}

pub fn set(key: &str, value: &str) -> Result<()> {
    keyring::Entry::new(SERVICE, key)
        .and_then(|e| e.set_password(value))
        .map_err(|e| msg(format!("credential store: {e}")))
}

pub fn delete(key: &str) {
    for svc in [SERVICE, LEGACY_SERVICE] {
        if let Ok(e) = keyring::Entry::new(svc, key) {
            let _ = e.delete_credential();
        }
    }
}
