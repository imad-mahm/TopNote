//! TopNote's entry in Windows' media flyout (the one above Wi-Fi / Bluetooth) while TopNote's own
//! player is the one playing. WebView2's own entry is switched off (it only knew the page title and
//! showed up as "Unknown application"); this one carries the real song, artist and artwork, and its
//! buttons go through the normal playback controls.
//!
//! All SMTC calls happen on the main (UI) thread, which owns the window.
use crate::playback::PlaybackState;
use crate::state::AppState;
use std::cell::RefCell;
use std::sync::{Arc, Mutex, OnceLock};
use tauri::{AppHandle, Manager};
use windows::core::{factory, HSTRING};
use windows::Foundation::{TimeSpan, TypedEventHandler, Uri};
use windows::Media::{
    MediaPlaybackStatus, MediaPlaybackType, PlaybackPositionChangeRequestedEventArgs, SystemMediaTransportControls,
    SystemMediaTransportControlsButton, SystemMediaTransportControlsButtonPressedEventArgs,
    SystemMediaTransportControlsTimelineProperties,
};
use windows::Storage::Streams::RandomAccessStreamReference;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::WinRT::ISystemMediaTransportControlsInterop;

static APP: OnceLock<AppHandle> = OnceLock::new();

thread_local! {
    static SMTC: RefCell<Option<SystemMediaTransportControls>> = const { RefCell::new(None) };
}

/// What the flyout currently shows, so unchanged polls cost nothing.
#[derive(Clone, PartialEq, Default)]
struct Shown {
    enabled: bool,
    track: Option<String>,
    playing: bool,
}
static SHOWN: Mutex<Option<Shown>> = Mutex::new(None);

/// Give the process TopNote's app id (the one the installer puts on its shortcuts), so Windows
/// shows "TopNote" and its icon instead of "Unknown application". Call before any window exists.
pub fn set_app_id(id: &str) {
    unsafe {
        let _ = windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(&HSTRING::from(id));
    }
}

fn act(action: &'static str, value: Option<serde_json::Value>) {
    let Some(app) = APP.get() else { return };
    let st = app.state::<Arc<AppState>>().inner().clone();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = crate::playback::control(&app, &st, action, value).await {
            log::warn!("media flyout {action}: {e}");
        }
    });
}

/// Must run on the main thread (Tauri `setup`).
pub fn install(app: &AppHandle) {
    let _ = APP.set(app.clone());
    let Some(w) = crate::window::main_window(app) else { return };
    let Ok(raw) = w.hwnd() else { return };
    let r = (|| -> windows::core::Result<SystemMediaTransportControls> {
        let interop = factory::<SystemMediaTransportControls, ISystemMediaTransportControlsInterop>()?;
        let smtc: SystemMediaTransportControls = unsafe { interop.GetForWindow(HWND(raw.0 as _))? };
        smtc.SetIsEnabled(false)?;
        smtc.SetIsPlayEnabled(true)?;
        smtc.SetIsPauseEnabled(true)?;
        smtc.SetIsNextEnabled(true)?;
        smtc.SetIsPreviousEnabled(true)?;
        smtc.ButtonPressed(&TypedEventHandler::<
            SystemMediaTransportControls,
            SystemMediaTransportControlsButtonPressedEventArgs,
        >::new(|_, args| {
            if let Some(a) = args.as_ref() {
                match a.Button()? {
                    SystemMediaTransportControlsButton::Play => act("play", None),
                    SystemMediaTransportControlsButton::Pause => act("pause", None),
                    SystemMediaTransportControlsButton::Next => act("next", None),
                    SystemMediaTransportControlsButton::Previous => act("previous", None),
                    _ => {}
                }
            }
            Ok(())
        }))?;
        smtc.PlaybackPositionChangeRequested(&TypedEventHandler::<
            SystemMediaTransportControls,
            PlaybackPositionChangeRequestedEventArgs,
        >::new(|_, args| {
            if let Some(a) = args.as_ref() {
                let ms = a.RequestedPlaybackPosition()?.Duration / 10_000;
                act("seek", Some(serde_json::json!(ms)));
            }
            Ok(())
        }))?;
        Ok(smtc)
    })();
    match r {
        Ok(smtc) => SMTC.with(|s| *s.borrow_mut() = Some(smtc)),
        Err(e) => log::warn!("media flyout unavailable: {e}"),
    }
}

/// Mirror the playback state into the flyout. Shown only while TopNote's own player is the
/// active device; when Spotify desktop plays, Spotify has its own entry.
pub fn update(app: &AppHandle, p: &PlaybackState) {
    let mine = p.source == "webapi"
        && p.device_name.as_deref() == Some(crate::playback::sdk::DEVICE_NAME)
        && p.status == "ok"
        && !p.title.is_empty();
    let next = Shown {
        enabled: mine,
        track: mine.then(|| p.track_id.clone().unwrap_or_else(|| p.title.clone())),
        playing: mine && p.is_playing,
    };
    let changed = {
        let mut cur = SHOWN.lock().unwrap();
        let changed = cur.as_ref() != Some(&next);
        *cur = Some(next.clone());
        changed
    };
    // the timeline moves every poll; only worth sending while the entry is visible
    if !changed && !mine {
        return;
    }
    let p = p.clone();
    let _ = app.run_on_main_thread(move || {
        SMTC.with(|s| {
            let s = s.borrow();
            let Some(smtc) = s.as_ref() else { return };
            if let Err(e) = apply(smtc, &p, &next, changed) {
                log::warn!("media flyout update: {e}");
            }
        })
    });
}

fn apply(smtc: &SystemMediaTransportControls, p: &PlaybackState, s: &Shown, changed: bool) -> windows::core::Result<()> {
    if !s.enabled {
        smtc.SetPlaybackStatus(MediaPlaybackStatus::Stopped)?;
        smtc.DisplayUpdater()?.ClearAll()?;
        return smtc.SetIsEnabled(false);
    }
    if changed {
        let du = smtc.DisplayUpdater()?;
        du.ClearAll()?;
        du.SetType(MediaPlaybackType::Music)?;
        let m = du.MusicProperties()?;
        m.SetTitle(&HSTRING::from(&p.title))?;
        m.SetArtist(&HSTRING::from(p.artists.join(", ")))?;
        m.SetAlbumTitle(&HSTRING::from(&p.album))?;
        if let Some(url) = p.art_url.as_deref().filter(|u| u.starts_with("https://")) {
            if let Ok(uri) = Uri::CreateUri(&HSTRING::from(url)) {
                du.SetThumbnail(&RandomAccessStreamReference::CreateFromUri(&uri)?)?;
            }
        }
        du.Update()?;
        smtc.SetPlaybackStatus(if s.playing { MediaPlaybackStatus::Playing } else { MediaPlaybackStatus::Paused })?;
        smtc.SetIsEnabled(true)?;
    }
    let tl = SystemMediaTransportControlsTimelineProperties::new()?;
    let ts = |ms: i64| TimeSpan { Duration: ms.max(0) * 10_000 };
    tl.SetStartTime(ts(0))?;
    tl.SetMinSeekTime(ts(0))?;
    tl.SetEndTime(ts(p.duration_ms))?;
    tl.SetMaxSeekTime(ts(p.duration_ms))?;
    tl.SetPosition(ts(p.progress_ms.min(p.duration_ms)))?;
    smtc.UpdateTimelineProperties(&tl)
}
