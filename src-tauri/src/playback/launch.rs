//! "Play" with nothing playing anywhere: find or start a player, reporting progress to the UI
//! (`launch://status`) so a slow Spotify start never looks like a frozen button.
//!
//! Order: an idle Spotify device → TopNote's own player (Premium, opt-in) → Spotify desktop
//! started in the background with its window hidden.
use super::{desktop, sdk, smtc, webapi};
use crate::error::{msg, AppError, Result};
use crate::{auth, state::AppState};
use reqwest::Method;
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

static BUSY: AtomicBool = AtomicBool::new(false);

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LaunchStatus {
    /// working | done | error
    pub phase: &'static str,
    pub message: String,
}

fn report(app: &AppHandle, phase: &'static str, message: impl Into<String>) {
    let _ = app.emit("launch://status", LaunchStatus { phase, message: message.into() });
}

fn free(st: &AppState) -> bool {
    *st.premium.lock().unwrap() == Some(false)
}

pub async fn start(app: &AppHandle, st: &Arc<AppState>) -> Result<()> {
    if BUSY.swap(true, Ordering::AcqRel) {
        return Ok(()); // already starting; the UI is showing progress
    }
    let r = run(app, st).await;
    BUSY.store(false, Ordering::Release);
    match &r {
        Ok(()) => report(app, "done", ""),
        Err(e) => report(app, "error", e.to_string()),
    }
    st.poll_now.notify_one();
    r
}

async fn run(app: &AppHandle, st: &Arc<AppState>) -> Result<()> {
    report(app, "working", "Looking for Spotify…");

    // 1. Spotify is open somewhere, just idle.
    if !free(st) {
        if let Some(id) = pick_device(st).await? {
            match transfer(st, &id).await {
                Err(AppError::PremiumRequired) => {}
                r => return r,
            }
        }
    }
    if free(st) && st.smtc.lock().unwrap().is_some() {
        return smtc::command(smtc::Command::Play).await;
    }

    // 2. TopNote's own player.
    if st.settings().in_app_playback && !free(st) && auth::has_scope(st, "streaming") != Some(false) {
        report(app, "working", "Starting TopNote's player…");
        match play_in_app(app, st).await {
            Ok(()) => return Ok(()),
            Err(e) => log::warn!("in-app playback failed, starting Spotify instead: {e}"),
        }
    }

    // 3. Spotify desktop, in the background.
    let launched = !desktop::running();
    if launched {
        report(app, "working", "Starting Spotify…");
        desktop::launch()?;
    } else {
        report(app, "working", "Waiting for Spotify…");
    }
    let started = Instant::now();
    let mut connecting = false;
    loop {
        tokio::time::sleep(Duration::from_millis(600)).await;
        if launched {
            desktop::hide_windows();
        }
        if !connecting && started.elapsed() > Duration::from_secs(4) {
            report(app, "working", "Waiting for Spotify to connect…");
            connecting = true;
        }
        if !free(st) {
            match pick_device(st).await {
                Ok(Some(id)) => match transfer(st, &id).await {
                    Ok(()) => return finish(launched),
                    Err(AppError::PremiumRequired) => {} // Free account: use the media controls below
                    Err(e) => log::warn!("transfer to Spotify failed: {e}"),
                },
                Ok(None) | Err(AppError::PremiumRequired) => {}
                Err(e) => log::warn!("device list failed: {e}"),
            }
        }
        if free(st) && st.smtc.lock().unwrap().is_some() {
            // let Spotify finish restoring the last track before asking it to play
            tokio::time::sleep(Duration::from_millis(800)).await;
            if smtc::command(smtc::Command::Play).await.is_ok() {
                return finish(launched);
            }
        }
        if started.elapsed() > Duration::from_secs(45) {
            return Err(msg("Spotify didn't start in time. Open it, then press play."));
        }
    }
}

/// Spotify can re-show its window while it finishes loading, so keep hiding it for a while.
fn finish(launched: bool) -> Result<()> {
    if launched {
        std::thread::spawn(|| {
            for _ in 0..30 {
                desktop::hide_windows();
                std::thread::sleep(Duration::from_millis(500));
            }
        });
    }
    Ok(())
}

async fn transfer(st: &AppState, device: &str) -> Result<()> {
    webapi::call(st, Method::PUT, "/me/player", Some(json!({ "device_ids": [device], "play": true })))
        .await
        .map(|_| ())
}

/// Best idle device: a computer running Spotify, then TopNote's own (live) player, then anything.
async fn pick_device(st: &AppState) -> Result<Option<String>> {
    let v = webapi::call(st, Method::GET, "/me/player/devices", None).await?;
    let list = v.as_ref().and_then(|d| d["devices"].as_array().cloned()).unwrap_or_default();
    let ours = sdk::device_id(st);
    let usable = |d: &&Value| {
        let id = d["id"].as_str();
        let restricted = d["is_restricted"].as_bool().unwrap_or(false);
        // a dead TopNote player can linger in the list for a while
        let stale_ours = d["name"] == sdk::DEVICE_NAME && id != ours.as_deref();
        id.is_some() && !restricted && !stale_ours
    };
    let pick = list
        .iter()
        .filter(usable)
        .find(|d| d["type"] == "Computer" && d["name"] != sdk::DEVICE_NAME)
        .or_else(|| list.iter().filter(usable).find(|d| d["id"].as_str() == ours.as_deref()))
        .or_else(|| list.iter().find(|d| usable(d)));
    Ok(pick.and_then(|d| d["id"].as_str().map(String::from)))
}

/// Move playback into TopNote's player and make sure audio is really running: the SDK can
/// register as a device yet fail to decrypt audio (DRM) on some setups.
async fn play_in_app(app: &AppHandle, st: &Arc<AppState>) -> Result<()> {
    let id = sdk::ensure(app, st).await?;
    report(app, "working", "Connecting TopNote's player…");
    st.sdk.lock().unwrap().error = None;
    transfer(st, &id).await?;
    let started = Instant::now();
    let mut first: Option<i64> = None;
    while started.elapsed() < Duration::from_secs(12) {
        tokio::time::sleep(Duration::from_millis(900)).await;
        {
            let s = st.sdk.lock().unwrap();
            if s.broken {
                return Err(msg(s.error.clone().unwrap_or_default()));
            }
            if let Some(e) = &s.error {
                let e = e.clone();
                drop(s);
                sdk::mark_broken(app, st, &format!("TopNote's player couldn't play: {e}"));
                return Err(msg(e));
            }
        }
        let Some(v) = webapi::call(st, Method::GET, "/me/player", None).await? else { continue };
        if v["device"]["id"].as_str() != Some(id.as_str()) || !v["is_playing"].as_bool().unwrap_or(false) {
            continue;
        }
        let pos = v["progress_ms"].as_i64().unwrap_or(0);
        match first {
            None => first = Some(pos),
            Some(p0) if pos > p0 + 300 => return Ok(()),
            _ => {}
        }
    }
    sdk::mark_broken(app, st, "TopNote's player couldn't play audio on this PC");
    Err(msg("TopNote's player didn't start playing"))
}
