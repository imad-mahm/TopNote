//! Secrets live in the OS credential store (Windows Credential Manager, or the Secret Service on
//! Linux: GNOME Keyring / KWallet), never in settings.json or the webview.
//!
//! Linux desktops without a Secret Service (a bare window manager, a container) fall back to a
//! private file (mode 0600) in the config folder, so TopNote still works there.
use crate::error::{msg, Result};

const SERVICE: &str = "topnote";
/// Service name used before the app was renamed; entries are moved over on first read.
const LEGACY_SERVICE: &str = "spotify-miniplayer";

pub fn get(key: &str) -> Option<String> {
    if let Some(v) = keyring::Entry::new(SERVICE, key).ok().and_then(|e| e.get_password().ok()) {
        return Some(v);
    }
    if let Some(v) = fallback::get(key) {
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
    let stored = keyring::Entry::new(SERVICE, key).and_then(|e| e.set_password(value));
    match stored {
        Ok(()) => {
            fallback::delete(key); // never leave a stale copy in the file
            Ok(())
        }
        Err(e) => {
            log::warn!("credential store unavailable ({e}); using a private file instead");
            fallback::set(key, value).map_err(|fe| msg(format!("credential store: {e}; file fallback: {fe}")))
        }
    }
}

pub fn delete(key: &str) {
    for svc in [SERVICE, LEGACY_SERVICE] {
        if let Ok(e) = keyring::Entry::new(svc, key) {
            let _ = e.delete_credential();
        }
    }
    fallback::delete(key);
}

#[cfg(target_os = "linux")]
mod fallback {
    use serde_json::{Map, Value};
    use std::os::unix::fs::OpenOptionsExt;
    use std::path::PathBuf;
    use std::sync::OnceLock;

    static DIR: OnceLock<PathBuf> = OnceLock::new();

    /// Where the fallback file lives; set once at startup.
    pub fn init(dir: &std::path::Path) {
        let _ = DIR.set(dir.to_path_buf());
    }

    fn path() -> Option<PathBuf> {
        DIR.get().map(|d| d.join("secrets.json"))
    }

    fn read() -> Map<String, Value> {
        path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn write(m: &Map<String, Value>) -> std::io::Result<()> {
        let Some(p) = path() else { return Err(std::io::Error::other("config folder not ready")) };
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(p)?;
        f.write_all(serde_json::to_string(m)?.as_bytes())
    }

    pub fn get(key: &str) -> Option<String> {
        read().get(key).and_then(|v| v.as_str()).map(String::from)
    }

    pub fn set(key: &str, value: &str) -> std::io::Result<()> {
        let mut m = read();
        m.insert(key.into(), Value::String(value.into()));
        write(&m)
    }

    pub fn delete(key: &str) {
        let mut m = read();
        if m.remove(key).is_some() {
            let _ = write(&m);
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod fallback {
    pub fn get(_: &str) -> Option<String> {
        None
    }
    pub fn set(_: &str, _: &str) -> std::io::Result<()> {
        Err(std::io::Error::other("no fallback on this platform"))
    }
    pub fn delete(_: &str) {}
}

/// Tell the file fallback where the config folder is.
pub fn init(_dir: &std::path::Path) {
    #[cfg(target_os = "linux")]
    fallback::init(_dir);
}
