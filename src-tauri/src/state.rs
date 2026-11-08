use crate::auth::Tokens;
use crate::lyrics::LyricsService;
use crate::playback::{smtc::SmtcSnapshot, PlaybackState};
use crate::settings::Settings;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;
use std::time::Instant;
use tokio::sync::Notify;

pub const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36";

pub struct AppState {
    pub http: reqwest::Client,
    pub config_dir: PathBuf,
    pub settings: Mutex<Settings>,
    pub auth: tokio::sync::Mutex<Option<Tokens>>,
    pub poll_now: Notify,
    pub backoff_until: Mutex<Option<Instant>>,
    pub playback: Mutex<PlaybackState>,
    pub smtc: Mutex<Option<SmtcSnapshot>>,
    pub lyrics: LyricsService,
    pub window_visible: AtomicBool,
}

impl AppState {
    pub fn settings(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }
}
