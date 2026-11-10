use crate::error::{msg, Result};
use crate::lyrics::{cache, resolver, LyricsResult, ProviderHealth, TrackMeta};
use crate::playback::PlaybackState;
use crate::settings::{self, Settings};
use crate::state::AppState;
use crate::{auth, secrets, window};
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt;

type St<'a> = State<'a, Arc<AppState>>;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthStatus {
    logged_in: bool,
    has_client_id: bool,
    has_sp_dc: bool,
    redirect_uri: &'static str,
    /// Whether the login allows playing inside TopNote (null until the first token refresh).
    can_stream: Option<bool>,
}

fn auth_status_of(st: &AppState) -> AuthStatus {
    AuthStatus {
        logged_in: auth::logged_in(),
        has_client_id: !st.settings().client_id.trim().is_empty(),
        has_sp_dc: secrets::get("sp_dc").is_some(),
        redirect_uri: auth::REDIRECT_URI,
        can_stream: auth::has_scope(st, "streaming"),
    }
}

fn apply_settings(app: &AppHandle, st: &AppState, new: Settings) -> Result<Settings> {
    let old = st.settings();
    settings::save(&st.config_dir, &new)?;
    *st.settings.lock().unwrap() = new.clone();
    if old.skip_taskbar != new.skip_taskbar {
        if let Some(w) = window::main_window(app) {
            let _ = w.set_skip_taskbar(new.skip_taskbar);
        }
    }
    if old.autostart != new.autostart {
        let al = app.autolaunch();
        let _ = if new.autostart { al.enable() } else { al.disable() };
    }
    if old.always_on_top != new.always_on_top {
        if let Some(w) = window::main_window(app) {
            let _ = w.set_always_on_top(new.always_on_top);
        }
        #[cfg(any(windows, target_os = "linux"))]
        if new.always_on_top {
            crate::topmost::raise(app);
        }
    }
    if old.in_app_playback != new.in_app_playback {
        crate::playback::sdk::reset(app, st);
    }
    if serde_json::to_value(&old.shortcuts).ok() != serde_json::to_value(&new.shortcuts).ok() {
        window::register_shortcuts(app, st);
    }
    let _ = app.emit("settings://changed", &new);
    Ok(new)
}

/// Shortcut offset nudges apply to the current song (remembered per track); global offset otherwise.
pub fn bump_offset(app: &AppHandle, delta: i64) {
    let st = app.state::<Arc<AppState>>();
    let mut s = st.settings();
    let id = st.playback.lock().unwrap().track_id.clone();
    match id {
        Some(id) => {
            let v = s.track_offsets.get(&id).copied().unwrap_or(0) + delta;
            let v = v.clamp(-5000, 5000);
            if v == 0 {
                s.track_offsets.remove(&id);
            } else {
                s.track_offsets.insert(id, v);
            }
        }
        None => s.offset_ms = (s.offset_ms + delta).clamp(-5000, 5000),
    }
    let _ = apply_settings(app, &st, s);
}

#[tauri::command]
pub fn get_settings(st: St) -> Settings {
    st.settings()
}

#[tauri::command]
pub fn set_settings(app: AppHandle, st: St, patch: Value) -> Result<Settings> {
    let merged = settings::merge(&st.settings(), &patch)?;
    apply_settings(&app, &st, merged)
}

#[tauri::command]
pub fn auth_status(st: St) -> AuthStatus {
    auth_status_of(&st)
}

#[tauri::command]
pub async fn login(app: AppHandle, st: St<'_>) -> Result<AuthStatus> {
    auth::login(&st).await?;
    crate::playback::recheck_plan(&st);
    crate::playback::sdk::reset(&app, &st);
    st.poll_now.notify_one();
    let _ = app.emit("auth://changed", true);
    Ok(auth_status_of(&st))
}

#[tauri::command]
pub async fn logout(app: AppHandle, st: St<'_>) -> Result<AuthStatus> {
    auth::logout(&st).await;
    st.poll_now.notify_one();
    let _ = app.emit("auth://changed", false);
    Ok(auth_status_of(&st))
}

#[tauri::command]
pub async fn set_sp_dc(st: St<'_>, value: Option<String>) -> Result<AuthStatus> {
    match value.map(|v| v.trim().trim_start_matches("sp_dc=").to_string()).filter(|v| !v.is_empty()) {
        Some(v) => secrets::set("sp_dc", &v)?,
        None => secrets::delete("sp_dc"),
    }
    *st.lyrics.spotify_token.lock().await = None;
    st.lyrics.health.lock().unwrap().remove("spotify");
    Ok(auth_status_of(&st))
}

#[tauri::command]
pub fn get_playback(st: St) -> PlaybackState {
    st.playback.lock().unwrap().clone()
}

#[tauri::command]
pub async fn control(app: AppHandle, st: St<'_>, action: String, value: Option<Value>) -> Result<()> {
    crate::playback::control(&app, &st, &action, value).await
}

/// Access token for the in-app player page only (the Web Playback SDK runs in the webview).
#[tauri::command]
pub async fn player_token(window: tauri::WebviewWindow, st: St<'_>) -> Result<String> {
    if window.label() != crate::playback::sdk::LABEL {
        return Err(msg("not allowed"));
    }
    auth::access_token(&st).await
}

#[tauri::command]
pub fn player_event(
    window: tauri::WebviewWindow,
    app: AppHandle,
    st: St,
    kind: String,
    device_id: Option<String>,
    message: Option<String>,
) {
    if window.label() == crate::playback::sdk::LABEL {
        crate::playback::sdk::on_event(&app, &st, &kind, device_id, message);
    }
}

/// Bring a Spotify window TopNote hid back on screen.
#[tauri::command]
pub fn show_spotify() -> Result<()> {
    if crate::playback::desktop::has_hidden() {
        crate::playback::desktop::restore_hidden();
        Ok(())
    } else {
        crate::playback::desktop::launch()
    }
}

#[tauri::command]
pub async fn get_lyrics(st: St<'_>, meta: TrackMeta, force: Option<bool>, provider: Option<String>) -> Result<LyricsResult> {
    if let Some(p) = &provider {
        if !resolver::PROVIDERS.contains(&p.as_str()) {
            return Err(msg(format!("unknown provider {p}")));
        }
    }
    resolver::resolve(st.inner().clone(), meta, force.unwrap_or(false), provider).await
}

#[tauri::command]
pub fn provider_health(st: St) -> Vec<ProviderHealth> {
    st.lyrics.health_report()
}

#[tauri::command]
pub fn reset_provider_health(st: St) {
    st.lyrics.reset_health();
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheStats {
    entries: i64,
    bytes: i64,
    path: String,
}

#[tauri::command]
pub fn cache_stats(st: St) -> CacheStats {
    let (entries, bytes) = cache::stats(&st.lyrics.cache.lock().unwrap());
    CacheStats { entries, bytes, path: st.config_dir.display().to_string() }
}

#[tauri::command]
pub fn clear_cache(st: St) -> Result<()> {
    cache::clear(&st.lyrics.cache.lock().unwrap())?;
    Ok(())
}

#[tauri::command]
pub fn set_click_through(app: AppHandle, on: bool) {
    window::set_click_through(&app, on);
}

// Must be async: creating a webview from a sync command deadlocks on Windows
// (the window appears but stays blank white).
#[tauri::command]
pub async fn open_settings(app: AppHandle) -> Result<()> {
    window::open_settings(&app)?;
    Ok(())
}

#[tauri::command]
pub fn open_config_dir(st: St) -> Result<()> {
    tauri_plugin_opener::open_path(st.config_dir.display().to_string(), None::<&str>).map_err(|e| msg(e.to_string()))
}

#[tauri::command]
pub fn hide_main(app: AppHandle) {
    if let Some(w) = window::main_window(&app) {
        let _ = w.hide();
        app.state::<Arc<AppState>>().window_visible.store(false, std::sync::atomic::Ordering::Relaxed);
    }
}

#[tauri::command]
pub fn quit(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
pub async fn translate_lyrics(st: St<'_>, key: String, lines: Vec<String>, to: String) -> Result<crate::translate::Translation> {
    let to = if to.trim().is_empty() { "en".to_string() } else { to.trim().to_lowercase() };
    crate::translate::translate(&st, &key, lines, &to).await
}

#[tauri::command]
pub async fn search_lyrics(st: St<'_>, query: String) -> Result<Vec<crate::lyrics::lrclib::Candidate>> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(vec![]);
    }
    crate::lyrics::lrclib::search(&st.http, q).await.map_err(|e| msg(e.msg))
}

/// Use a hand-picked search result for this track (sticks until "Refetch lyrics").
#[tauri::command]
pub async fn pick_lyrics(st: St<'_>, meta: TrackMeta, provider: String, id: String) -> Result<LyricsResult> {
    if provider != "lrclib" {
        return Err(msg(format!("can't pick from {provider}")));
    }
    let lyrics = crate::lyrics::lrclib::get_by_id(&st.http, &id, meta.duration_ms)
        .await
        .map_err(|e| msg(e.msg))?
        .ok_or(msg("That entry has no lyrics"))?;
    let r = LyricsResult {
        track_id: meta.id.clone(),
        lyrics: Some(lyrics),
        attempts: vec![crate::lyrics::Attempt { provider, outcome: "picked".into(), detail: Some(id) }],
        cached: false,
    };
    cache::put_pick(&st.lyrics.cache.lock().unwrap(), &r, crate::playback::smtc::now_ms())?;
    Ok(r)
}

#[tauri::command]
pub async fn get_queue(st: St<'_>) -> Result<Vec<crate::playback::QueueItem>> {
    crate::playback::queue(st.inner()).await
}

#[tauri::command]
pub fn listening_stats(st: St) -> crate::stats::ListeningStats {
    crate::stats::compute(&st.lyrics.cache.lock().unwrap())
}

#[tauri::command]
pub fn reset_stats(st: St) {
    *st.listen.lock().unwrap() = None;
    crate::stats::reset(&st.lyrics.cache.lock().unwrap());
}
