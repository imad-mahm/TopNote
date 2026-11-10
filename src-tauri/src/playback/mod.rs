pub mod desktop;
pub mod launch;
pub mod sdk;
pub mod smtc;
pub mod webapi;

use crate::error::{msg, AppError, Result};
use crate::lyrics::TrackMeta;
use crate::{auth, state::AppState};
use reqwest::Method;
use serde::Serialize;
use serde_json::{json, Value};
use smtc::now_ms;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackState {
    /// ok | idle | noDevice | loggedOut | offline | rateLimited
    pub status: String,
    /// track | episode | ad | unknown
    pub kind: String,
    pub track_id: Option<String>,
    pub title: String,
    pub artists: Vec<String>,
    pub album: String,
    pub art_url: Option<String>,
    pub duration_ms: i64,
    pub progress_ms: i64,
    pub is_playing: bool,
    pub shuffle: bool,
    pub repeat: String,
    pub volume: Option<i64>,
    pub supports_volume: bool,
    pub liked: bool,
    pub isrc: Option<String>,
    pub device_name: Option<String>,
    pub description: Option<String>,
    /// webapi | smtc
    pub source: String,
    /// Spotify plan: true Premium, false Free (controls go through Windows), null unknown.
    pub premium: Option<bool>,
    /// unix ms when progress_ms was measured
    pub ts: i64,
}

impl Default for PlaybackState {
    fn default() -> Self {
        Self {
            status: "idle".into(),
            kind: "unknown".into(),
            track_id: None,
            title: String::new(),
            artists: vec![],
            album: String::new(),
            art_url: None,
            duration_ms: 0,
            progress_ms: 0,
            is_playing: false,
            shuffle: false,
            repeat: "off".into(),
            volume: None,
            supports_volume: true,
            liked: false,
            isrc: None,
            device_name: None,
            description: None,
            source: "webapi".into(),
            premium: None,
            ts: now_ms(),
        }
    }
}

impl PlaybackState {
    pub fn meta(&self) -> Option<TrackMeta> {
        Some(TrackMeta {
            id: self.track_id.clone()?,
            title: self.title.clone(),
            artists: self.artists.clone(),
            album: self.album.clone(),
            duration_ms: self.duration_ms,
            isrc: self.isrc.clone(),
        })
    }
}

fn s(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_string()
}

pub fn parse_player(v: &Value) -> PlaybackState {
    let mut p = PlaybackState {
        status: "ok".into(),
        is_playing: v["is_playing"].as_bool().unwrap_or(false),
        progress_ms: v["progress_ms"].as_i64().unwrap_or(0),
        shuffle: v["shuffle_state"].as_bool().unwrap_or(false),
        repeat: v["repeat_state"].as_str().unwrap_or("off").to_string(),
        volume: v["device"]["volume_percent"].as_i64(),
        supports_volume: v["device"]["supports_volume"].as_bool().unwrap_or(true),
        device_name: v["device"]["name"].as_str().map(String::from),
        kind: v["currently_playing_type"].as_str().unwrap_or("unknown").to_string(),
        ts: now_ms(),
        ..Default::default()
    };
    let item = &v["item"];
    if item.is_object() {
        p.track_id = item["id"].as_str().map(String::from);
        p.title = s(&item["name"]);
        p.duration_ms = item["duration_ms"].as_i64().unwrap_or(0);
        if item["type"] == "episode" {
            p.kind = "episode".into();
            p.artists = vec![s(&item["show"]["name"])];
            p.album = s(&item["show"]["publisher"]);
            p.art_url = item["images"][0]["url"].as_str().map(String::from);
            p.description = item["description"].as_str().map(String::from);
        } else {
            p.kind = "track".into();
            p.artists = item["artists"]
                .as_array()
                .map(|a| a.iter().map(|x| s(&x["name"])).collect())
                .unwrap_or_default();
            p.album = s(&item["album"]["name"]);
            p.art_url = item["album"]["images"][0]["url"].as_str().map(String::from);
            p.isrc = item["external_ids"]["isrc"].as_str().map(String::from);
        }
    }
    p
}

fn from_smtc(st: &AppState, status: &str) -> PlaybackState {
    let prev = st.playback.lock().unwrap().clone();
    match st.smtc.lock().unwrap().clone() {
        Some(sm) => {
            let same = prev.title == sm.title;
            PlaybackState {
                status: status.into(),
                kind: "track".into(),
                track_id: if sm.track_id.is_some() {
                    sm.track_id.clone()
                } else if same && prev.track_id.is_some() {
                    prev.track_id.clone()
                } else {
                    Some(format!("smtc:{}|{}", sm.artist, sm.title))
                },
                title: sm.title,
                artists: sm.artist.split(", ").map(String::from).collect(),
                album: sm.album,
                art_url: sm.art_url.clone().or(if same { prev.art_url.clone() } else { None }),
                duration_ms: sm.duration_ms,
                progress_ms: sm.position_ms,
                is_playing: sm.playing,
                liked: same && prev.liked,
                isrc: if same { prev.isrc.clone() } else { None },
                source: "smtc".into(),
                ts: sm.at_ms,
                ..prev
            }
        }
        None => PlaybackState { status: status.into(), ..Default::default() },
    }
}

fn publish(app: &AppHandle, st: &AppState, mut p: PlaybackState) {
    p.premium = *st.premium.lock().unwrap();
    if p.premium == Some(false) {
        p.supports_volume = false;
    }
    let _ = app.emit("playback://state", &p);
    let prev = std::mem::replace(&mut *st.playback.lock().unwrap(), p.clone());
    crate::stats::observe(st, &p);
    if prev.title != p.title || prev.artists != p.artists {
        if let Some(w) = crate::window::main_window(app) {
            let t = match (p.title.is_empty(), p.artists.first()) {
                (true, _) => "TopNote".to_string(),
                (false, Some(a)) if !a.is_empty() => format!("{} · {}", p.title, a),
                _ => p.title.clone(),
            };
            let _ = w.set_title(&t);
        }
    }
    #[cfg(windows)]
    crate::taskbar::update(app, p.is_playing, st.settings().taskbar_buttons);
    #[cfg(windows)]
    crate::media_session::update(app, &p);
}

pub fn spawn(app: AppHandle, st: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        loop {
            let wait = poll_once(&app, &st).await;
            tokio::select! {
                _ = tokio::time::sleep(wait) => {}
                _ = st.poll_now.notified() => {}
            }
        }
    });
}

async fn poll_once(app: &AppHandle, st: &Arc<AppState>) -> Duration {
    if !auth::logged_in() {
        publish(app, st, from_smtc(st, "loggedOut"));
        return Duration::from_secs(3);
    }
    let hidden = !st.window_visible.load(Ordering::Relaxed);
    if !PLAN_CHECKED.swap(true, Ordering::AcqRel) {
        let st2 = st.clone();
        tauri::async_runtime::spawn(async move { detect_plan(&st2).await });
    }
    match webapi::call(st, Method::GET, "/me/player?additional_types=episode", None).await {
        Ok(Some(v)) => {
            let mut p = parse_player(&v);
            let in_app = p.device_name.as_deref() == Some(sdk::DEVICE_NAME);
            let (prev_id, prev_playing) = {
                let cur = st.playback.lock().unwrap();
                (cur.track_id.clone(), cur.is_playing)
            };
            // TopNote's own player already told us what's playing; Spotify's servers can lag a moment
            // behind it, so don't let a stale answer flip the song back.
            if in_app
                && now_ms() - SDK_STATE_AT.load(Ordering::Relaxed) < 3000
                && (p.track_id != prev_id || p.is_playing != prev_playing)
            {
                return Duration::from_millis(1000);
            }
            let changed = p.track_id != prev_id;
            let new_track = p.track_id.clone().filter(|_| changed && p.kind == "track");
            if !changed {
                p.liked = st.playback.lock().unwrap().liked;
            }
            let playing = p.is_playing;
            let remaining = (p.duration_ms - p.progress_ms).max(0) as u64;
            // Publish first so the UI starts loading lyrics right away; extras follow in the background.
            publish(app, st, p);
            if let Some(id) = new_track {
                let (app2, st2) = (app.clone(), st.clone());
                tauri::async_runtime::spawn(async move {
                    if let Ok(liked) = check_liked(&st2, &id).await {
                        let mut cur = st2.playback.lock().unwrap().clone();
                        if cur.track_id.as_deref() == Some(id.as_str()) && cur.liked != liked {
                            cur.liked = liked;
                            publish(&app2, &st2, cur);
                        }
                    }
                });
            }
            let due = now_ms() - LAST_QUEUE_CHECK.load(Ordering::Relaxed) > QUEUE_RECHECK_MS;
            if changed || (playing && due) {
                let (app2, st2) = (app.clone(), st.clone());
                tauri::async_runtime::spawn(async move { prefetch_queue(app2, st2).await });
            }
            poll_interval(st, hidden, playing, in_app, remaining)
        }
        Ok(None) | Err(AppError::NoDevice) => {
            let mut p = from_smtc(st, "noDevice");
            if p.track_id.is_none() {
                p.status = "noDevice".into();
            }
            publish(app, st, p);
            Duration::from_secs(if hidden { 15 } else { 5 })
        }
        Err(AppError::RateLimited(secs)) => {
            publish(app, st, from_smtc(st, "rateLimited"));
            Duration::from_secs(secs.min(30))
        }
        Err(AppError::NotLoggedIn) => {
            let _ = app.emit("auth://changed", false);
            publish(app, st, from_smtc(st, "loggedOut"));
            Duration::from_secs(5)
        }
        Err(AppError::Http(e)) => {
            log::warn!("playback offline: {e}");
            publish(app, st, from_smtc(st, "offline"));
            Duration::from_secs(5)
        }
        Err(e) => {
            log::warn!("playback poll error: {e}");
            Duration::from_secs(5)
        }
    }
}

/// How long until the next look at Spotify's servers.
/// - TopNote's own player reports every change itself, so the servers are only a safety net
///   (playback moved to another device, a song liked elsewhere).
/// - Spotify desktop on this PC announces changes through Windows, which triggers an immediate poll.
/// - Otherwise (phone, speaker...) poll briskly.
/// While playing, always look again right after the song should have ended.
fn poll_interval(st: &AppState, hidden: bool, playing: bool, in_app: bool, remaining_ms: u64) -> Duration {
    let sdk_live = in_app && SDK_STATE_AT.load(Ordering::Relaxed) > 0;
    let base = if sdk_live || hidden {
        Duration::from_secs(15)
    } else if !playing {
        Duration::from_secs(5)
    } else if st.smtc.lock().unwrap().is_some() {
        Duration::from_secs(3)
    } else {
        Duration::from_millis(1000)
    };
    if playing && !hidden {
        base.min(Duration::from_millis(remaining_ms + 400).max(Duration::from_millis(300)))
    } else {
        base
    }
}

/// When TopNote's player last reported its state (ms); 0 = it isn't the one playing.
static SDK_STATE_AT: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);

/// State straight from TopNote's player (Web Playback SDK): the song, position and pause state
/// as they change, without waiting for Spotify's servers. `None` = playback moved elsewhere.
static SDK_STALLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn apply_sdk_state(app: &AppHandle, st: &Arc<AppState>, v: Option<Value>) {
    let Some(v) = v.filter(|v| v["trackId"].is_string()) else {
        SDK_STATE_AT.store(0, Ordering::Relaxed);
        st.poll_now.notify_one();
        return;
    };
    let mut p = st.playback.lock().unwrap().clone();
    let id = s(&v["trackId"]);
    let changed = p.track_id.as_deref() != Some(id.as_str());
    if changed {
        p.isrc = None;
        p.liked = false;
        p.description = None;
        p.album = String::new();
        p.art_url = None;
    }
    p.status = "ok".into();
    p.source = "webapi".into();
    p.device_name = Some(sdk::DEVICE_NAME.into());
    p.supports_volume = true;
    p.track_id = Some(id.clone());
    p.kind = match v["kind"].as_str() {
        Some("episode") => "episode",
        Some("ad") => "ad",
        _ => "track",
    }
    .into();
    p.title = s(&v["title"]);
    p.artists = v["artists"].as_array().map(|a| a.iter().map(s).collect()).unwrap_or_default();
    if let Some(a) = v["album"].as_str() {
        p.album = a.into();
    }
    if let Some(a) = v["artUrl"].as_str() {
        p.art_url = Some(a.into());
    }
    p.duration_ms = v["durationMs"].as_i64().unwrap_or(p.duration_ms);
    p.progress_ms = v["positionMs"].as_i64().unwrap_or(0);
    // A stalled (buffering) player counts as not playing, so the clock doesn't run ahead of the audio.
    let stalled = v["stalled"].as_bool().unwrap_or(false);
    if SDK_STALLED.swap(stalled, Ordering::Relaxed) != stalled {
        log::info!("in-app player: {} at {} ms", if stalled { "stalled (buffering)" } else { "playing again" }, v["positionMs"]);
    }
    p.is_playing = !v["paused"].as_bool().unwrap_or(false) && !stalled;
    p.shuffle = v["shuffle"].as_bool().unwrap_or(p.shuffle);
    p.repeat = match v["repeatMode"].as_i64() {
        Some(1) => "context",
        Some(2) => "track",
        _ => "off",
    }
    .into();
    p.ts = now_ms();
    SDK_STATE_AT.store(now_ms(), Ordering::Relaxed);
    let kind_track = p.kind == "track";
    publish(app, st, p);
    if let Some(next) = v["nextArt"].as_array() {
        let urls: Vec<&str> = next.iter().filter_map(|u| u.as_str()).collect();
        if !urls.is_empty() {
            let _ = app.emit("art://preload", urls);
        }
    }
    if changed {
        let (app2, st2) = (app.clone(), st.clone());
        tauri::async_runtime::spawn(async move {
            if kind_track {
                if let Ok(liked) = check_liked(&st2, &id).await {
                    let mut cur = st2.playback.lock().unwrap().clone();
                    if cur.track_id.as_deref() == Some(id.as_str()) && cur.liked != liked {
                        cur.liked = liked;
                        publish(&app2, &st2, cur);
                    }
                }
            }
            prefetch_queue(app2, st2).await;
        });
    }
}

static PLAN_CHECKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Look the plan up again (after a new login).
pub fn recheck_plan(st: &AppState) {
    *st.premium.lock().unwrap() = None;
    PLAN_CHECKED.store(false, Ordering::Release);
}

/// Premium or Free, from the profile. Needs the user-read-private scope; logins from before
/// 1.1 don't have it, and then the first refused player command (403 PREMIUM_REQUIRED) tells us.
async fn detect_plan(st: &AppState) {
    match webapi::call(st, Method::GET, "/me", None).await {
        Ok(Some(v)) => {
            if let Some(product) = v["product"].as_str() {
                log::info!("Spotify plan: {product}");
                *st.premium.lock().unwrap() = Some(product == "premium");
            }
        }
        Ok(None) => {}
        Err(e) => {
            log::warn!("plan check failed: {e}");
            PLAN_CHECKED.store(false, Ordering::Release);
        }
    }
}

async fn check_liked(st: &AppState, id: &str) -> Result<bool> {
    let v = webapi::call(st, Method::GET, &format!("/me/tracks/contains?ids={id}"), None).await?;
    Ok(v.and_then(|v| v[0].as_bool()).unwrap_or(false))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueItem {
    pub id: String,
    pub title: String,
    pub artists: Vec<String>,
    pub art_url: Option<String>,
    pub duration_ms: i64,
    pub kind: String,
    /// "ready" | "none" | "pending"
    pub lyrics: String,
}

/// The upcoming queue as Spotify reports it (positions matter for skipTo, so no de-duplication).
pub async fn queue(st: &Arc<AppState>) -> Result<Vec<QueueItem>> {
    let v = webapi::call(st, Method::GET, "/me/player/queue", None).await?.unwrap_or_default();
    Ok(queue_items(st, &v))
}

fn queue_items(st: &AppState, v: &Value) -> Vec<QueueItem> {
    let empty = Vec::new();
    let items = v["queue"].as_array().unwrap_or(&empty);
    items
        .iter()
        .take(QUEUE_DEPTH)
        .map(|t| {
            let episode = t["type"] == "episode";
            let images = if episode { &t["images"] } else { &t["album"]["images"] };
            let art = images.as_array().and_then(|a| a.last()).and_then(|i| i["url"].as_str()).map(String::from);
            let id = s(&t["id"]);
            let lyrics = if episode {
                "none"
            } else {
                match crate::lyrics::resolver::lyrics_ready(st, &id) {
                    Some(true) => "ready",
                    Some(false) => "none",
                    None => "pending",
                }
            };
            QueueItem {
                id,
                title: s(&t["name"]),
                artists: if episode {
                    vec![s(&t["show"]["name"])]
                } else {
                    t["artists"].as_array().map(|a| a.iter().map(|x| s(&x["name"])).collect()).unwrap_or_default()
                },
                art_url: art,
                duration_ms: t["duration_ms"].as_i64().unwrap_or(0),
                kind: if episode { "episode".into() } else { "track".into() },
                lyrics: lyrics.into(),
            }
        })
        .collect()
}

/// How many upcoming tracks to prepare lyrics for.
const QUEUE_DEPTH: usize = 20;
/// Re-read the queue this often while playing, to pick up newly queued songs.
const QUEUE_RECHECK_MS: i64 = 20_000;
/// Parallel lyric fetches during prefetch (kept low to avoid provider rate limits / captchas).
const PREFETCH_CONCURRENCY: usize = 2;

static LAST_QUEUE_CHECK: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);
static PREFETCH_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Resolve lyrics for the upcoming queue, nearest track first. Already-cached tracks are skipped,
/// so after the first pass only newly queued songs cost a network round-trip.
/// The one place the queue is read while playing: the queue panel gets the result as an event
/// (no polling of its own) and the next song's artwork is downloaded ahead of time.
async fn prefetch_queue(app: AppHandle, st: Arc<AppState>) {
    LAST_QUEUE_CHECK.store(now_ms(), Ordering::Relaxed);
    let res = webapi::call(&st, Method::GET, "/me/player/queue", None).await;
    if let Ok(Some(v)) = &res {
        let _ = app.emit("queue://changed", queue_items(&st, v));
        let next_art: Vec<&str> = v["queue"]
            .as_array()
            .map(|a| a.as_slice())
            .unwrap_or_default()
            .iter()
            .take(2)
            .filter_map(|t| {
                let images = if t["type"] == "episode" { &t["images"] } else { &t["album"]["images"] };
                images[0]["url"].as_str()
            })
            .collect();
        if !next_art.is_empty() {
            let _ = app.emit("art://preload", next_art);
        }
    }
    if PREFETCH_RUNNING.swap(true, Ordering::AcqRel) {
        return; // a lyrics pass is already running; it picks up new songs on the next check
    }
    let metas: Vec<TrackMeta> = match res {
        Ok(Some(v)) => {
            let mut seen = std::collections::HashSet::new();
            v["queue"]
                .as_array()
                .map(|a| a.as_slice())
                .unwrap_or_default()
                .iter()
                .filter(|t| t["type"] == "track")
                .filter_map(|t| parse_player(&json!({ "item": t })).meta())
                .filter(|m| seen.insert(m.id.clone())) // repeat/loop contexts list the same track many times
                .take(QUEUE_DEPTH)
                .filter(|m| !st.lyrics.prefetched.lock().unwrap().contains(&m.id))
                .filter(|m| !crate::lyrics::resolver::is_cached(&st, &m.id))
                .collect()
        }
        _ => Vec::new(),
    };
    if !metas.is_empty() {
        log::info!("prefetching lyrics for {} queued tracks", metas.len());
    }
    let sem = Arc::new(tokio::sync::Semaphore::new(PREFETCH_CONCURRENCY));
    let mut tasks = Vec::new();
    for meta in metas {
        let permit = sem.clone().acquire_owned().await.expect("semaphore open");
        let st2 = st.clone();
        tasks.push(tauri::async_runtime::spawn(async move {
            let id = meta.id.clone();
            let _ = crate::lyrics::resolver::resolve(st2.clone(), meta, false, None).await;
            st2.lyrics.prefetched.lock().unwrap().insert(id);
            drop(permit);
        }));
    }
    let fetched = !tasks.is_empty();
    for t in tasks {
        let _ = t.await;
    }
    PREFETCH_RUNNING.store(false, Ordering::Release);
    if fetched {
        // refresh the queue panel's "lyrics ready" marks
        if let Ok(Some(v)) = webapi::call(&st, Method::GET, "/me/player/queue", None).await {
            let _ = app.emit("queue://changed", queue_items(&st, &v));
        }
    }
}

fn is_free(st: &AppState) -> bool {
    *st.premium.lock().unwrap() == Some(false)
}

/// The same command through the OS media controls (Windows SMTC / Linux MPRIS) (works without Premium).
fn smtc_command(action: &str, cur: &PlaybackState, value: Option<i64>) -> Option<smtc::Command> {
    Some(match action {
        "play" => smtc::Command::Play,
        "pause" => smtc::Command::Pause,
        "toggle" => smtc::Command::Toggle,
        "next" => smtc::Command::Next,
        "previous" => smtc::Command::Previous,
        "seek" => smtc::Command::Seek(value?),
        "shuffle" => smtc::Command::Shuffle(!cur.shuffle),
        "repeat" => smtc::Command::Repeat(next_repeat(&cur.repeat).into()),
        _ => return None,
    })
}

fn next_repeat(cur: &str) -> &'static str {
    match cur {
        "off" => "context",
        "context" => "track",
        _ => "off",
    }
}

/// Playback controls. Returns after the command was accepted and schedules a fast re-poll.
/// With nothing playing anywhere, "play" starts a player first (see [`launch`]).
pub async fn control(app: &AppHandle, st: &Arc<AppState>, action: &str, value: Option<Value>) -> Result<()> {
    let num = value.as_ref().and_then(|v| v.as_f64()).map(|f| f.round() as i64);
    let cur = st.playback.lock().unwrap().clone();
    let wants_play = matches!(action, "play" | "toggle");
    // A Free account with Spotify open (seen through Windows) can just be told to play.
    let smtc_reachable = is_free(st) && cur.source == "smtc";
    if action == "transfer" || (wants_play && cur.status == "noDevice" && !smtc_reachable) {
        return launch::start(app, st).await;
    }
    // TopNote's own player is playing: tell it directly instead of going through Spotify's
    // servers. Its state event then updates the UI.
    if cur.device_name.as_deref() == Some(sdk::DEVICE_NAME)
        && SDK_STATE_AT.load(Ordering::Relaxed) > 0
        && sdk::command(app, action, num)
    {
        if action == "volume" {
            let mut p = st.playback.lock().unwrap().clone();
            p.volume = num.map(|v| v.clamp(0, 100));
            publish(app, st, p);
        }
        return Ok(());
    }
    let res = match smtc_command(action, &cur, num) {
        Some(cmd) if is_free(st) => smtc::command(cmd).await,
        _ => web_control(st, action, &cur, num).await,
    };
    let res = match res {
        // Found out just now that this is a Free account: retry through Windows.
        Err(AppError::PremiumRequired) if is_free(st) => match smtc_command(action, &cur, num) {
            Some(cmd) => {
                st.poll_now.notify_one(); // republish with premium=false so the UI adapts
                smtc::command(cmd).await
            }
            None => Err(AppError::PremiumRequired),
        },
        Err(AppError::NoDevice) if wants_play => return launch::start(app, st).await,
        r => r,
    };
    res?;
    let st2 = st.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(350)).await;
        st2.poll_now.notify_one();
    });
    Ok(())
}

async fn web_control(st: &Arc<AppState>, action: &str, cur: &PlaybackState, num: Option<i64>) -> Result<()> {
    let r = match action {
        "play" => webapi::call(st, Method::PUT, "/me/player/play", None).await,
        "pause" => webapi::call(st, Method::PUT, "/me/player/pause", None).await,
        "toggle" => {
            let path = if cur.is_playing { "/me/player/pause" } else { "/me/player/play" };
            webapi::call(st, Method::PUT, path, None).await
        }
        "next" => webapi::call(st, Method::POST, "/me/player/next", None).await,
        "previous" => webapi::call(st, Method::POST, "/me/player/previous", None).await,
        // Jump to the n-th upcoming queue item (the API can only skip one at a time).
        "skipTo" => {
            let n = num.ok_or(msg("skipTo needs a count"))?.clamp(1, 50);
            let mut r = Ok(None);
            for i in 0..n {
                r = webapi::call(st, Method::POST, "/me/player/next", None).await;
                if r.is_err() {
                    break;
                }
                if i + 1 < n {
                    tokio::time::sleep(Duration::from_millis(120)).await;
                }
            }
            r
        }
        "seek" => {
            let ms = num.ok_or(msg("seek needs a position"))?.max(0);
            webapi::call(st, Method::PUT, &format!("/me/player/seek?position_ms={ms}"), None).await
        }
        "shuffle" => {
            webapi::call(st, Method::PUT, &format!("/me/player/shuffle?state={}", !cur.shuffle), None).await
        }
        "repeat" => {
            let next = next_repeat(&cur.repeat);
            webapi::call(st, Method::PUT, &format!("/me/player/repeat?state={next}"), None).await
        }
        "volume" => {
            let v = num.ok_or(msg("volume needs a value"))?.clamp(0, 100);
            webapi::call(st, Method::PUT, &format!("/me/player/volume?volume_percent={v}"), None).await
        }
        "like" | "unlike" => {
            let id = cur.track_id.clone().filter(|i| !i.starts_with("smtc:")).ok_or(msg("nothing to like"))?;
            let m = if action == "like" { Method::PUT } else { Method::DELETE };
            let r = webapi::call(st, m, &format!("/me/tracks?ids={id}"), None).await;
            if r.is_ok() {
                st.playback.lock().unwrap().liked = action == "like";
            }
            r
        }
        other => return Err(msg(format!("unknown action {other}"))),
    };
    r.map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_track() {
        let v = json!({
            "is_playing": true, "progress_ms": 1234, "shuffle_state": false, "repeat_state": "track",
            "currently_playing_type": "track",
            "device": {"name": "PC", "volume_percent": 55, "supports_volume": true},
            "item": {"type": "track", "id": "abc", "name": "Song", "duration_ms": 200000,
                "artists": [{"name": "A"}, {"name": "B"}],
                "album": {"name": "Alb", "images": [{"url": "http://img"}]},
                "external_ids": {"isrc": "US123"}}
        });
        let p = parse_player(&v);
        assert_eq!(p.track_id.as_deref(), Some("abc"));
        assert_eq!(p.artists, vec!["A", "B"]);
        assert_eq!(p.isrc.as_deref(), Some("US123"));
        assert_eq!(p.volume, Some(55));
        assert_eq!(p.repeat, "track");
        assert!(p.is_playing);
    }
}
