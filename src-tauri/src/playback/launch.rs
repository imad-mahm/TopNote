//! "Play" with nothing playing anywhere: find or start a player, reporting progress to the UI
//! (`launch://status`) so a slow Spotify start never looks like a frozen button.
//!
//! Order: TopNote's own player (Premium, opt-in) → Spotify desktop on this PC, already open or
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
        Ok(note) => report(app, "done", note.clone().unwrap_or_default()),
        Err(e) => {
            log::warn!("play: {e}");
            report(app, "error", e.to_string())
        }
    }
    st.poll_now.notify_one();
    r.map(|_| ())
}

/// Find or start a player on this PC. Ok(Some(note)) = playing, but not the way the user asked
/// for (the note says why, e.g. TopNote's player failed and Spotify took over).
async fn run(app: &AppHandle, st: &Arc<AppState>) -> Result<Option<String>> {
    report(app, "working", "Looking for a player…");
    let mut note = None;

    // 1. TopNote's own player, when the user asked for it.
    if st.settings().in_app_playback && !free(st) {
        if auth::has_scope(st, "streaming") == Some(false) {
            log::info!("play: login has no streaming scope, skipping TopNote's player");
            note = Some("Using Spotify: log in again (Settings → Account) to play inside TopNote".to_string());
        } else {
            report(app, "working", "Starting TopNote's player…");
            match play_in_app(app, st).await {
                Ok(()) => {
                    log::info!("play: TopNote's player is playing");
                    return Ok(None);
                }
                Err(AppError::PremiumRequired) => {}
                Err(e) => {
                    log::warn!("play: TopNote's player failed, using Spotify instead: {e}");
                    note = Some(format!("Using Spotify: {e}"));
                }
            }
        }
    }

    // 2. Spotify on this PC: already open (maybe idle), or started in the background.
    if free(st) && st.smtc.lock().unwrap().is_some() {
        smtc::command(smtc::Command::Play).await?;
        return Ok(note);
    }
    let launched = !desktop::running();
    if launched {
        report(app, "working", "Starting Spotify…");
        desktop::launch()?;
    } else {
        report(app, "working", "Waiting for Spotify…");
    }
    let started = Instant::now();
    let mut connecting = false;
    let mut first = true;
    loop {
        if !first {
            tokio::time::sleep(Duration::from_millis(600)).await;
        }
        first = false;
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
                    Ok(()) => return finish(launched, note),
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
                return finish(launched, note);
            }
        }
        if started.elapsed() > Duration::from_secs(45) {
            return Err(msg("Spotify didn't start in time. Open it, then press play."));
        }
    }
}

/// Spotify can re-show its window while it finishes loading, so keep hiding it for a while.
fn finish(launched: bool, note: Option<String>) -> Result<Option<String>> {
    if launched {
        std::thread::spawn(|| {
            for _ in 0..30 {
                desktop::hide_windows();
                std::thread::sleep(Duration::from_millis(500));
            }
        });
    }
    Ok(note)
}

async fn transfer(st: &AppState, device: &str) -> Result<()> {
    webapi::call(st, Method::PUT, "/me/player", Some(json!({ "device_ids": [device], "play": true })))
        .await
        .map(|_| ())
}

fn host_name() -> String {
    let name = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .filter(|n| !n.trim().is_empty())
        .or_else(|| std::fs::read_to_string("/proc/sys/kernel/hostname").ok())
        .or_else(|| std::fs::read_to_string("/etc/hostname").ok())
        .unwrap_or_default();
    name.trim().to_lowercase()
}

/// Spotify desktop on *this* PC. Phones, speakers and other computers are never picked: pressing
/// Play in TopNote shouldn't start music somewhere else.
async fn pick_device(st: &AppState) -> Result<Option<String>> {
    let v = webapi::call(st, Method::GET, "/me/player/devices", None).await?;
    let list = v.as_ref().and_then(|d| d["devices"].as_array().cloned()).unwrap_or_default();
    let computers: Vec<&Value> = list
        .iter()
        .filter(|d| {
            d["id"].as_str().is_some()
                && !d["is_restricted"].as_bool().unwrap_or(false)
                && d["type"] == "Computer"
                && d["name"] != sdk::DEVICE_NAME
        })
        .collect();
    let host = host_name();
    // Spotify names the device after the PC; Windows' COMPUTERNAME is cut to 15 characters
    let is_this_pc = |d: &&Value| {
        let n = d["name"].as_str().unwrap_or_default().to_lowercase();
        !host.is_empty() && (n == host || (host.len() == 15 && n.starts_with(&host)))
    };
    let pick = computers
        .iter()
        .copied()
        .find(|d| is_this_pc(d))
        // name not recognised, but Spotify is running here and it's the only computer online
        .or_else(|| (computers.len() == 1 && desktop::running()).then(|| computers[0]));
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
