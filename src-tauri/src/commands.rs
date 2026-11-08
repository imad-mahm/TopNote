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
}

fn auth_status_of(st: &AppState) -> AuthStatus {
    AuthStatus {
        logged_in: auth::logged_in(),
        has_client_id: !st.settings().client_id.trim().is_empty(),
        has_sp_dc: secrets::get("sp_dc").is_some(),
        redirect_uri: auth::REDIRECT_URI,
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
    if serde_json::to_value(&old.shortcuts).ok() != serde_json::to_value(&new.shortcuts).ok() {
        window::register_shortcuts(app, st);
    }
    let _ = app.emit("settings://changed", &new);
    Ok(new)
}

pub fn bump_offset(app: &AppHandle, delta: i64) {
    let st = app.state::<Arc<AppState>>();
    let mut s = st.settings();
    s.offset_ms = (s.offset_ms + delta).clamp(-5000, 5000);
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
pub async fn control(st: St<'_>, action: String, value: Option<Value>) -> Result<()> {
    crate::playback::control(&st, &action, value).await
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

#[tauri::command]
pub fn open_settings(app: AppHandle) -> Result<()> {
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
