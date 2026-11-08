//! Windows System Media Transport Controls: instant change detection + offline fallback.
use crate::state::AppState;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Clone, Default, Debug)]
pub struct SmtcSnapshot {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub playing: bool,
    pub position_ms: i64,
    pub duration_ms: i64,
    /// unix ms at which position_ms was valid
    pub at_ms: i64,
}

pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

#[cfg(windows)]
pub fn spawn(st: Arc<AppState>) {
    std::thread::Builder::new()
        .name("smtc".into())
        .spawn(move || imp::run(st))
        .ok();
}

#[cfg(not(windows))]
pub fn spawn(_st: Arc<AppState>) {}

/// A playback command sent to Spotify through Windows' media controls. This works on Free
/// accounts, where the Web API refuses every player command.
#[derive(Debug, Clone)]
pub enum Command {
    Play,
    Pause,
    Toggle,
    Next,
    Previous,
    Seek(i64),
    Shuffle(bool),
    /// "off" | "context" | "track"
    Repeat(String),
}

#[cfg(windows)]
pub async fn command(cmd: Command) -> crate::error::Result<()> {
    tauri::async_runtime::spawn_blocking(move || imp::command(cmd))
        .await
        .map_err(|e| crate::error::msg(e.to_string()))?
}

#[cfg(not(windows))]
pub async fn command(_cmd: Command) -> crate::error::Result<()> {
    Err(crate::error::msg("Media controls are only available on Windows"))
}

#[cfg(windows)]
mod imp {
    use super::*;
    use crate::error::{msg, AppError};
    use windows::Media::Control::{
        GlobalSystemMediaTransportControlsSession as Session,
        GlobalSystemMediaTransportControlsSessionManager as Manager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
    };
    use windows::Media::MediaPlaybackAutoRepeatMode as RepeatMode;

    fn werr(e: windows::core::Error) -> AppError {
        msg(format!("Windows media controls: {e}"))
    }

    pub fn command(cmd: Command) -> crate::error::Result<()> {
        unsafe {
            let _ = windows::Win32::System::WinRT::RoInitialize(windows::Win32::System::WinRT::RO_INIT_MULTITHREADED);
        }
        let mgr = Manager::RequestAsync().and_then(|op| op.join()).map_err(werr)?;
        let s = find(&mgr).map_err(werr)?.ok_or(AppError::NoDevice)?;
        let op = match &cmd {
            Command::Play => s.TryPlayAsync(),
            Command::Pause => s.TryPauseAsync(),
            Command::Toggle => s.TryTogglePlayPauseAsync(),
            Command::Next => s.TrySkipNextAsync(),
            Command::Previous => s.TrySkipPreviousAsync(),
            Command::Seek(ms) => s.TryChangePlaybackPositionAsync((*ms).max(0) * 10_000),
            Command::Shuffle(on) => s.TryChangeShuffleActiveAsync(*on),
            Command::Repeat(mode) => s.TryChangeAutoRepeatModeAsync(match mode.as_str() {
                "track" => RepeatMode::Track,
                "context" => RepeatMode::List,
                _ => RepeatMode::None,
            }),
        };
        if op.and_then(|o| o.join()).map_err(werr)? {
            Ok(())
        } else {
            Err(match cmd {
                Command::Shuffle(_) | Command::Repeat(_) | Command::Seek(_) => AppError::PremiumRequired,
                _ => msg("Spotify didn't accept that command"),
            })
        }
    }

    fn find(mgr: &Manager) -> windows::core::Result<Option<Session>> {
        let sessions = mgr.GetSessions()?;
        for i in 0..sessions.Size()? {
            let s = sessions.GetAt(i)?;
            if s.SourceAppUserModelId()?.to_string().to_lowercase().contains("spotify") {
                return Ok(Some(s));
            }
        }
        Ok(None)
    }

    const FILETIME_UNIX_DIFF: i64 = 116_444_736_000_000_000;

    pub fn run(st: Arc<AppState>) {
        unsafe {
            let _ = windows::Win32::System::WinRT::RoInitialize(windows::Win32::System::WinRT::RO_INIT_MULTITHREADED);
        }
        let mgr = loop {
            match Manager::RequestAsync().and_then(|op| op.join()) {
                Ok(m) => break m,
                Err(_) => std::thread::sleep(Duration::from_secs(5)),
            }
        };
        let mut last_key = String::new();
        loop {
            let snap = read(&mgr).ok().flatten();
            let key = snap
                .as_ref()
                .map(|s| format!("{}|{}|{}", s.title, s.artist, s.playing))
                .unwrap_or_default();
            if key != last_key {
                last_key = key;
                st.poll_now.notify_one();
            }
            *st.smtc.lock().unwrap() = snap;
            std::thread::sleep(Duration::from_millis(400));
        }
    }

    fn read(mgr: &Manager) -> windows::core::Result<Option<SmtcSnapshot>> {
        if let Some(s) = find(mgr)? {
            let props = s.TryGetMediaPropertiesAsync()?.join()?;
            let info = s.GetPlaybackInfo()?;
            let playing = info.PlaybackStatus()? == Status::Playing;
            let tl = s.GetTimelineProperties()?;
            let mut position_ms = tl.Position()?.Duration / 10_000;
            let duration_ms = tl.EndTime()?.Duration / 10_000;
            let updated = (tl.LastUpdatedTime()?.UniversalTime - FILETIME_UNIX_DIFF) / 10_000;
            let now = now_ms();
            if playing && updated > 0 && updated <= now {
                position_ms += now - updated;
            }
            return Ok(Some(SmtcSnapshot {
                title: props.Title()?.to_string(),
                artist: props.Artist()?.to_string(),
                album: props.AlbumTitle()?.to_string(),
                playing,
                position_ms: position_ms.min(duration_ms.max(position_ms)),
                duration_ms,
                at_ms: now,
            }));
        }
        Ok(None)
    }
}
