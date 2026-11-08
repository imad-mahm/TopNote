pub mod cache;
pub mod lrc;
pub mod lrclib;
pub mod model;
pub mod musixmatch;
pub mod resolver;
pub mod spotify;

pub use model::*;

use serde::Serialize;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

#[derive(Default)]
pub struct Health {
    pub fails: u32,
    pub until: Option<Instant>,
    pub last_error: Option<String>,
}

pub struct LyricsService {
    pub cache: Mutex<rusqlite::Connection>,
    pub health: Mutex<HashMap<String, Health>>,
    /// (token, expires unix ms)
    pub spotify_token: tokio::sync::Mutex<Option<(String, i64)>>,
    /// (version, cipher, fetched)
    pub spotify_secret: tokio::sync::Mutex<Option<(String, Vec<u8>, Instant)>>,
    pub mxm_token: tokio::sync::Mutex<Option<String>>,
    /// Resolutions currently running, so the UI joins a prefetch instead of starting a duplicate.
    pub inflight: Mutex<HashMap<String, std::sync::Arc<tokio::sync::OnceCell<LyricsResult>>>>,
    /// Track ids whose prefetch already ran this session (cache hit or miss).
    pub prefetched: Mutex<std::collections::HashSet<String>>,
}

impl LyricsService {
    pub fn new(conn: rusqlite::Connection) -> Self {
        Self {
            cache: Mutex::new(conn),
            health: Mutex::new(HashMap::new()),
            spotify_token: Default::default(),
            spotify_secret: Default::default(),
            mxm_token: Default::default(),
            inflight: Default::default(),
            prefetched: Default::default(),
        }
    }

    pub fn health_report(&self) -> Vec<ProviderHealth> {
        let map = self.health.lock().unwrap();
        let now = Instant::now();
        resolver::PROVIDERS
            .iter()
            .map(|name| {
                let h = map.get(*name);
                let cooling = h.and_then(|h| h.until).filter(|u| *u > now).map(|u| (u - now).as_secs());
                ProviderHealth {
                    name: name.to_string(),
                    ok: cooling.is_none() && h.map(|h| h.fails == 0).unwrap_or(true),
                    fails: h.map(|h| h.fails).unwrap_or(0),
                    cooling_down_secs: cooling,
                    last_error: h.and_then(|h| h.last_error.clone()),
                }
            })
            .collect()
    }

    pub fn reset_health(&self) {
        self.health.lock().unwrap().clear();
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderHealth {
    pub name: String,
    pub ok: bool,
    pub fails: u32,
    pub cooling_down_secs: Option<u64>,
    pub last_error: Option<String>,
}
