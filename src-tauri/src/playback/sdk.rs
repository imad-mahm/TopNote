//! TopNote's own Spotify player: the Web Playback SDK running in a hidden webview
//! (`index.html#player`). It shows up in Spotify Connect as "TopNote" and needs Premium.
use crate::error::{msg, Result};
use crate::state::AppState;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// The Web Playback SDK needs Widevine DRM in the webview. WebView2 has it; WebKitGTK (Linux) doesn't,
/// so there TopNote plays through Spotify desktop instead.
pub const SUPPORTED: bool = cfg!(windows);

pub const LABEL: &str = "player";
pub const DEVICE_NAME: &str = "TopNote";

#[derive(Default, Debug, Clone)]
pub struct SdkState {
    /// Spotify Connect device id once the SDK is ready.
    pub device_id: Option<String>,
    /// Last error the SDK reported.
    pub error: Option<String>,
    /// The SDK failed (no Premium, DRM refused, script blocked…): don't retry until `broken_at` is old.
    pub broken: bool,
    pub broken_at: Option<std::time::Instant>,
}

/// After a failure, Play goes straight to Spotify for this long instead of retrying the player.
const RETRY_AFTER: Duration = Duration::from_secs(5 * 60);

/// Start the player (if needed) and wait until Spotify has registered it as a device.
pub async fn ensure(app: &AppHandle, st: &Arc<AppState>) -> Result<String> {
    if !SUPPORTED {
        return Err(msg("TopNote's own player isn't available on this system"));
    }
    {
        let mut s = st.sdk.lock().unwrap();
        if s.broken {
            if s.broken_at.is_some_and(|t| t.elapsed() < RETRY_AFTER) {
                return Err(msg(s.error.clone().unwrap_or_else(|| "TopNote's player isn't available".into())));
            }
            *s = SdkState::default(); // give it another go
        }
        if let Some(id) = &s.device_id {
            if app.get_webview_window(LABEL).is_some() {
                return Ok(id.clone());
            }
        }
    }
    if app.get_webview_window(LABEL).is_none() {
        *st.sdk.lock().unwrap() = SdkState::default();
        WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html#player".into()))
            .title("TopNote Player")
            .visible(false)
            .skip_taskbar(true)
            .focused(false)
            .inner_size(320.0, 200.0)
            .additional_browser_args(crate::window::BROWSER_ARGS)
            .build()?;
    }
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        {
            let s = st.sdk.lock().unwrap();
            if let Some(id) = &s.device_id {
                return Ok(id.clone());
            }
            if s.broken {
                return Err(msg(s.error.clone().unwrap_or_else(|| "TopNote's player failed to start".into())));
            }
        }
        if tokio::time::Instant::now() >= deadline {
            mark_broken(app, st, "TopNote's player took too long to start");
            return Err(msg("TopNote's player took too long to start"));
        }
        // the notification only speeds things up; the short sleep covers a missed wake-up
        let _ = tokio::time::timeout(Duration::from_millis(250), st.sdk_changed.notified()).await;
    }
}

/// Events from the player page.
pub fn on_event(app: &AppHandle, st: &AppState, kind: &str, device_id: Option<String>, message: Option<String>) {
    if kind == "state" {
        let v = message.and_then(|m| serde_json::from_str(&m).ok());
        let st = app.state::<Arc<AppState>>().inner().clone();
        crate::playback::apply_sdk_state(app, &st, v);
        return;
    }
    let text = message.unwrap_or_default();
    log::info!("in-app player: {kind} {text}");
    match kind {
        "ready" => {
            let mut s = st.sdk.lock().unwrap();
            s.device_id = device_id;
            s.error = None;
        }
        "notReady" => st.sdk.lock().unwrap().device_id = None,
        "accountError" => {
            *st.premium.lock().unwrap() = Some(false);
            mark_broken(app, st, "Playing in TopNote needs Spotify Premium");
        }
        "authenticationError" => mark_broken(app, st, &format!("Spotify refused TopNote's player ({text}). Try logging in again")),
        "initializationError" | "loadFailed" | "connectFailed" => {
            mark_broken(app, st, &format!("TopNote's player couldn't start: {text}"))
        }
        "playbackError" => st.sdk.lock().unwrap().error = Some(text),
        _ => {}
    }
    st.sdk_changed.notify_waiters();
}

/// Send a playback command straight to the player page. False = not something it handles
/// locally (or the page is gone); the caller then uses the Web API.
pub fn command(app: &AppHandle, action: &str, value: Option<i64>) -> bool {
    if !matches!(action, "play" | "pause" | "toggle" | "next" | "previous" | "seek" | "volume") {
        return false;
    }
    if app.get_webview_window(LABEL).is_none() {
        return false;
    }
    use tauri::Emitter;
    app.emit_to(LABEL, "player://command", serde_json::json!({ "action": action, "value": value })).is_ok()
}

/// Give up on the in-app player for this session and close its webview.
pub fn mark_broken(app: &AppHandle, st: &AppState, why: &str) {
    log::warn!("in-app player disabled: {why}");
    {
        let mut s = st.sdk.lock().unwrap();
        s.broken = true;
        s.broken_at = Some(std::time::Instant::now());
        s.device_id = None;
        s.error = Some(why.to_string());
    }
    st.sdk_changed.notify_waiters();
    close(app);
}

/// Close the player webview (music playing in it stops).
pub fn close(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.destroy();
    }
}

/// Turned off in Settings: stop the player and allow a fresh try when it is turned back on.
pub fn reset(app: &AppHandle, st: &AppState) {
    close(app);
    *st.sdk.lock().unwrap() = SdkState::default();
}
