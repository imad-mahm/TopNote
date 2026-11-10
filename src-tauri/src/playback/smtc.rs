//! The OS media session of Spotify desktop: instant change detection, an offline fallback, and
//! playback commands that work on Free accounts (the Web API refuses them there).
//!
//! Windows reads System Media Transport Controls; Linux reads Spotify's MPRIS interface over D-Bus.
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
    /// Spotify track id, when the OS media session reports one (MPRIS does).
    pub track_id: Option<String>,
    pub art_url: Option<String>,
}

pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

#[cfg(any(windows, target_os = "linux"))]
pub fn spawn(st: Arc<AppState>) {
    std::thread::Builder::new()
        .name("smtc".into())
        .spawn(move || imp::run(st))
        .ok();
}

#[cfg(not(any(windows, target_os = "linux")))]
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

#[cfg(any(windows, target_os = "linux"))]
pub async fn command(cmd: Command) -> crate::error::Result<()> {
    tauri::async_runtime::spawn_blocking(move || imp::command(cmd))
        .await
        .map_err(|e| crate::error::msg(e.to_string()))?
}

#[cfg(not(any(windows, target_os = "linux")))]
pub async fn command(_cmd: Command) -> crate::error::Result<()> {
    Err(crate::error::msg("Media controls aren't available on this system"))
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
                track_id: None,
                art_url: None,
            }));
        }
        Ok(None)
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use super::*;
    use crate::error::{msg, AppError};
    use std::collections::HashMap;
    use std::sync::OnceLock;
    use zbus::blocking::{fdo::DBusProxy, Connection, Proxy};
    use zbus::zvariant::{OwnedValue, Value};

    const PREFIX: &str = "org.mpris.MediaPlayer2.spotify";
    const PATH: &str = "/org/mpris/MediaPlayer2";
    const PLAYER: &str = "org.mpris.MediaPlayer2.Player";

    static CONN: OnceLock<Connection> = OnceLock::new();

    fn bus() -> zbus::Result<Connection> {
        if let Some(c) = CONN.get() {
            return Ok(c.clone());
        }
        let c = Connection::session()?;
        Ok(CONN.get_or_init(|| c).clone())
    }

    fn zerr(e: impl std::fmt::Display) -> AppError {
        msg(format!("media controls: {e}"))
    }

    /// Bus name of the running Spotify ("org.mpris.MediaPlayer2.spotify", or one with an instance suffix).
    fn find(conn: &Connection) -> zbus::Result<Option<String>> {
        let names = DBusProxy::new(conn)?.list_names()?;
        Ok(names.into_iter().map(|n| n.to_string()).find(|n| n.starts_with(PREFIX)))
    }

    fn player<'a>(conn: &'a Connection, dest: &'a str) -> zbus::Result<Proxy<'a>> {
        Proxy::new(conn, dest, PATH, PLAYER)
    }

    pub fn command(cmd: Command) -> crate::error::Result<()> {
        let conn = bus().map_err(zerr)?;
        let dest = find(&conn).map_err(zerr)?.ok_or(AppError::NoDevice)?;
        let p = player(&conn, &dest).map_err(zerr)?;
        let rejected = |e: &dyn std::fmt::Display| {
            log::warn!("MPRIS {cmd:?}: {e}");
            match cmd {
                Command::Shuffle(_) | Command::Repeat(_) | Command::Seek(_) => AppError::PremiumRequired,
                _ => msg("Spotify didn't accept that command"),
            }
        };
        match &cmd {
            Command::Play => p.call::<_, _, ()>("Play", &()).map_err(|e| rejected(&e)),
            Command::Pause => p.call::<_, _, ()>("Pause", &()).map_err(|e| rejected(&e)),
            Command::Toggle => p.call::<_, _, ()>("PlayPause", &()).map_err(|e| rejected(&e)),
            Command::Next => p.call::<_, _, ()>("Next", &()).map_err(|e| rejected(&e)),
            Command::Previous => p.call::<_, _, ()>("Previous", &()).map_err(|e| rejected(&e)),
            Command::Seek(ms) => {
                let target = (*ms).max(0) * 1000;
                let meta: HashMap<String, OwnedValue> = p.get_property("Metadata").map_err(zerr)?;
                let track = meta.get("mpris:trackid").and_then(|v| match &**v {
                    Value::ObjectPath(o) => Some(o.to_owned()),
                    Value::Str(s) => zbus::zvariant::ObjectPath::try_from(s.as_str()).ok().map(|o| o.into_owned()),
                    _ => None,
                });
                let by_set = track.map(|t| p.call::<_, _, ()>("SetPosition", &(t, target)));
                match by_set {
                    Some(Ok(())) => Ok(()),
                    _ => {
                        // relative seek as a fallback
                        let now: i64 = p.get_property("Position").map_err(zerr)?;
                        p.call::<_, _, ()>("Seek", &(target - now)).map_err(|e| rejected(&e))
                    }
                }
            }
            Command::Shuffle(on) => p.set_property("Shuffle", *on).map_err(|e| rejected(&e)),
            Command::Repeat(mode) => {
                let v = match mode.as_str() {
                    "track" => "Track",
                    "context" => "Playlist",
                    _ => "None",
                };
                p.set_property("LoopStatus", v).map_err(|e| rejected(&e))
            }
        }
    }

    pub fn run(st: Arc<AppState>) {
        let conn = loop {
            match bus() {
                Ok(c) => break c,
                Err(e) => {
                    log::warn!("media controls: no D-Bus session bus ({e}); retrying");
                    std::thread::sleep(Duration::from_secs(10));
                }
            }
        };
        let mut dest: Option<String> = None;
        let mut last_lookup = std::time::Instant::now() - Duration::from_secs(10);
        let mut last_key = String::new();
        loop {
            if dest.is_none() && last_lookup.elapsed() >= Duration::from_secs(2) {
                last_lookup = std::time::Instant::now();
                dest = find(&conn).ok().flatten();
            }
            let snap = match &dest {
                Some(d) => match read(&conn, d) {
                    Ok(s) => s,
                    Err(_) => {
                        dest = None; // Spotify closed; look again shortly
                        None
                    }
                },
                None => None,
            };
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

    fn text(v: &OwnedValue) -> Option<String> {
        match &**v {
            Value::Str(s) => Some(s.to_string()),
            _ => None,
        }
    }

    fn number(v: &OwnedValue) -> Option<i64> {
        match &**v {
            Value::I64(n) => Some(*n),
            Value::U64(n) => Some(*n as i64),
            Value::I32(n) => Some(*n as i64),
            Value::U32(n) => Some(*n as i64),
            _ => None,
        }
    }

    fn strings(v: &OwnedValue) -> Vec<String> {
        match &**v {
            Value::Array(a) => a
                .iter()
                .filter_map(|x| match x {
                    Value::Str(s) => Some(s.to_string()),
                    _ => None,
                })
                .collect(),
            Value::Str(s) => vec![s.to_string()],
            _ => vec![],
        }
    }

    fn read(conn: &Connection, dest: &str) -> zbus::Result<Option<SmtcSnapshot>> {
        let p = player(conn, dest)?;
        let status: String = p.get_property("PlaybackStatus")?;
        let meta: HashMap<String, OwnedValue> = p.get_property("Metadata")?;
        let title = meta.get("xesam:title").and_then(text).unwrap_or_default();
        if title.is_empty() && status == "Stopped" {
            return Ok(None);
        }
        let position_us: i64 = p.get_property("Position").unwrap_or(0);
        let duration_ms = meta.get("mpris:length").and_then(number).unwrap_or(0) / 1000;
        let track_id = meta
            .get("mpris:trackid")
            .and_then(|v| match &**v {
                Value::ObjectPath(o) => Some(o.to_string()),
                Value::Str(s) => Some(s.to_string()),
                _ => None,
            })
            .and_then(|t| t.strip_prefix("/com/spotify/track/").map(String::from))
            .filter(|id| id.len() == 22 && id.chars().all(|c| c.is_ascii_alphanumeric()));
        let art_url = meta
            .get("mpris:artUrl")
            .and_then(text)
            // older Spotify builds publish an open.spotify.com link that redirects; the CDN is stable
            .map(|u| u.replace("https://open.spotify.com/image/", "https://i.scdn.co/image/"))
            .filter(|u| u.starts_with("https://"));
        let position_ms = (position_us / 1000).max(0);
        Ok(Some(SmtcSnapshot {
            title,
            artist: meta.get("xesam:artist").map(strings).unwrap_or_default().join(", "),
            album: meta.get("xesam:album").and_then(text).unwrap_or_default(),
            playing: status == "Playing",
            position_ms: position_ms.min(duration_ms.max(position_ms)),
            duration_ms,
            at_ms: now_ms(),
            track_id,
            art_url,
        }))
    }
}
