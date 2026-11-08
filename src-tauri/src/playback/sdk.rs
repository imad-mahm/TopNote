//! TopNote's own Spotify player: the Web Playback SDK running in a hidden webview
//! (`index.html#player`). It shows up in Spotify Connect as "TopNote" and needs Premium.
use crate::error::{msg, Result};
use crate::state::AppState;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub const LABEL: &str = "player";
pub const DEVICE_NAME: &str = "TopNote";

#[derive(Default, Debug, Clone)]
pub struct SdkState {
    /// Spotify Connect device id once the SDK is ready.
    pub device_id: Option<String>,
    /// Last error the SDK reported.
    pub error: Option<String>,
    /// The SDK can't work here (no Premium, DRM refused, script blocked…): don't try again this session.
    pub broken: bool,
}

/// Start the player (if needed) and wait until Spotify has registered it as a device.
pub async fn ensure(app: &AppHandle, st: &Arc<AppState>) -> Result<String> {
    {
        let s = st.sdk.lock().unwrap();
        if s.broken {
            return Err(msg(s.error.clone().unwrap_or_else(|| "TopNote's player isn't available".into())));
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
        "authenticationError" => mark_broken(app, st, "Log in again to play music in TopNote"),
        "initializationError" | "loadFailed" | "connectFailed" => {
            mark_broken(app, st, &format!("TopNote's player couldn't start: {text}"))
        }
        "playbackError" => st.sdk.lock().unwrap().error = Some(text),
        _ => {}
    }
    st.sdk_changed.notify_waiters();
}

/// Give up on the in-app player for this session and close its webview.
pub fn mark_broken(app: &AppHandle, st: &AppState, why: &str) {
    log::warn!("in-app player disabled: {why}");
    {
        let mut s = st.sdk.lock().unwrap();
        s.broken = true;
        s.device_id = None;
        s.error = Some(why.to_string());
    }
    st.sdk_changed.notify_waiters();
    close(app);
}

pub fn device_id(st: &AppState) -> Option<String> {
    st.sdk.lock().unwrap().device_id.clone()
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
