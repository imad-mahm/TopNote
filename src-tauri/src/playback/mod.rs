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
                track_id: if same && prev.track_id.is_some() {
                    prev.track_id.clone()
                } else {
                    Some(format!("smtc:{}|{}", sm.artist, sm.title))
                },
                title: sm.title,
                artists: sm.artist.split(", ").map(String::from).collect(),
                album: sm.album,
                art_url: if same { prev.art_url.clone() } else { None },
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

fn publish(app: &AppHandle, st: &AppState, p: PlaybackState) {
    let _ = app.emit("playback://state", &p);
    *st.playback.lock().unwrap() = p;
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
    match webapi::call(st, Method::GET, "/me/player?additional_types=episode", None).await {
        Ok(Some(v)) => {
            let mut p = parse_player(&v);
            let prev_id = st.playback.lock().unwrap().track_id.clone();
            let changed = p.track_id != prev_id;
            let new_track = p.track_id.clone().filter(|_| changed && p.kind == "track");
            if !changed {
                p.liked = st.playback.lock().unwrap().liked;
            }
            let playing = p.is_playing;
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
                let st2 = st.clone();
                tauri::async_runtime::spawn(async move { prefetch_queue(st2).await });
            }
            if hidden {
                Duration::from_secs(15)
            } else if playing {
                Duration::from_millis(1000)
            } else {
                Duration::from_secs(5)
            }
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

async fn check_liked(st: &AppState, id: &str) -> Result<bool> {
    let v = webapi::call(st, Method::GET, &format!("/me/tracks/contains?ids={id}"), None).await?;
    Ok(v.and_then(|v| v[0].as_bool()).unwrap_or(false))
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
async fn prefetch_queue(st: Arc<AppState>) {
    if PREFETCH_RUNNING.swap(true, Ordering::AcqRel) {
        return; // a pass is already running; it will be re-triggered on the next track change / recheck
    }
    LAST_QUEUE_CHECK.store(now_ms(), Ordering::Relaxed);
    let res = webapi::call(&st, Method::GET, "/me/player/queue", None).await;
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
    for t in tasks {
        let _ = t.await;
    }
    PREFETCH_RUNNING.store(false, Ordering::Release);
}

/// Playback controls. Returns after the API accepted the command and schedules a fast re-poll.
pub async fn control(st: &Arc<AppState>, action: &str, value: Option<Value>) -> Result<()> {
    let num = || value.as_ref().and_then(|v| v.as_f64()).map(|f| f.round() as i64);
    let cur = st.playback.lock().unwrap().clone();
    let res = match action {
        "play" => webapi::call(st, Method::PUT, "/me/player/play", None).await,
        "pause" => webapi::call(st, Method::PUT, "/me/player/pause", None).await,
        "toggle" => {
            let path = if cur.is_playing { "/me/player/pause" } else { "/me/player/play" };
            webapi::call(st, Method::PUT, path, None).await
        }
        "next" => webapi::call(st, Method::POST, "/me/player/next", None).await,
        "previous" => webapi::call(st, Method::POST, "/me/player/previous", None).await,
        "seek" => {
            let ms = num().ok_or(msg("seek needs a position"))?.max(0);
            webapi::call(st, Method::PUT, &format!("/me/player/seek?position_ms={ms}"), None).await
        }
        "shuffle" => {
            webapi::call(st, Method::PUT, &format!("/me/player/shuffle?state={}", !cur.shuffle), None).await
        }
        "repeat" => {
            let next = match cur.repeat.as_str() {
                "off" => "context",
                "context" => "track",
                _ => "off",
            };
            webapi::call(st, Method::PUT, &format!("/me/player/repeat?state={next}"), None).await
        }
        "volume" => {
            let v = num().ok_or(msg("volume needs a value"))?.clamp(0, 100);
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
        "transfer" => {
            let devices = webapi::call(st, Method::GET, "/me/player/devices", None).await?;
            let list = devices.as_ref().and_then(|d| d["devices"].as_array().cloned()).unwrap_or_default();
            let pick = list
                .iter()
                .find(|d| d["type"] == "Computer")
                .or_else(|| list.first())
                .and_then(|d| d["id"].as_str().map(String::from))
                .ok_or(msg("No Spotify device found. Open Spotify first."))?;
            webapi::call(st, Method::PUT, "/me/player", Some(json!({ "device_ids": [pick], "play": true }))).await
        }
        other => return Err(msg(format!("unknown action {other}"))),
    };
    res?;
    let st2 = st.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(350)).await;
        st2.poll_now.notify_one();
    });
    Ok(())
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
