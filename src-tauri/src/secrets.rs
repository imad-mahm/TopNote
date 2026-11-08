//! Secrets live in Windows Credential Manager, never in settings.json or the webview.
use crate::error::{msg, Result};

const SERVICE: &str = "spotify-miniplayer";

pub fn get(key: &str) -> Option<String> {
    keyring::Entry::new(SERVICE, key).ok()?.get_password().ok()
}

pub fn set(key: &str, value: &str) -> Result<()> {
    keyring::Entry::new(SERVICE, key)
        .and_then(|e| e.set_password(value))
        .map_err(|e| msg(format!("credential store: {e}")))
}

pub fn delete(key: &str) {
    if let Ok(e) = keyring::Entry::new(SERVICE, key) {
        let _ = e.delete_credential();
    }
}
