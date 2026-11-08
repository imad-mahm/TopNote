use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackMeta {
    pub id: String,
    pub title: String,
    pub artists: Vec<String>,
    pub album: String,
    pub duration_ms: i64,
    pub isrc: Option<String>,
}

impl TrackMeta {
    pub fn spotify_id(&self) -> Option<&str> {
        (!self.id.starts_with("smtc:") && self.id.len() == 22).then_some(self.id.as_str())
    }
    pub fn artist(&self) -> &str {
        self.artists.first().map(String::as_str).unwrap_or("")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SyncType {
    Word,
    Line,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Word {
    pub start_ms: i64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Line {
    pub start_ms: i64,
    pub end_ms: Option<i64>,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub words: Option<Vec<Word>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Colors {
    pub background: String,
    pub text: String,
    pub highlight: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lyrics {
    pub source: String,
    pub sync: SyncType,
    pub lines: Vec<Line>,
    pub instrumental: bool,
    pub colors: Option<Colors>,
    pub language: Option<String>,
}

impl Lyrics {
    pub fn new(source: &str, sync: SyncType, lines: Vec<Line>) -> Self {
        Self { source: source.into(), sync, lines, instrumental: false, colors: None, language: None }
    }
    pub fn instrumental(source: &str) -> Self {
        Self { instrumental: true, ..Self::new(source, SyncType::None, vec![]) }
    }
    pub fn plain(source: &str, text: &str) -> Self {
        let lines = text
            .lines()
            .map(|l| Line { start_ms: 0, end_ms: None, text: l.trim().to_string(), words: None })
            .collect();
        Self::new(source, SyncType::None, lines)
    }
    pub fn is_synced(&self) -> bool {
        self.sync != SyncType::None || self.instrumental
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attempt {
    pub provider: String,
    /// synced | unsynced | notFound | skipped | failed | timeout
    pub outcome: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricsResult {
    pub track_id: String,
    pub lyrics: Option<Lyrics>,
    pub attempts: Vec<Attempt>,
    pub cached: bool,
}

#[derive(Debug)]
pub struct ProviderError {
    pub msg: String,
    /// trip the breaker immediately for this long
    pub cooldown: Option<Duration>,
    /// not a failure: provider not configured / not applicable
    pub skip: bool,
}

impl ProviderError {
    pub fn new(m: impl Into<String>) -> Self {
        Self { msg: m.into(), cooldown: None, skip: false }
    }
    pub fn cooldown(m: impl Into<String>, d: Duration) -> Self {
        Self { msg: m.into(), cooldown: Some(d), skip: false }
    }
    pub fn skip(m: impl Into<String>) -> Self {
        Self { msg: m.into(), cooldown: None, skip: true }
    }
}

impl From<reqwest::Error> for ProviderError {
    fn from(e: reqwest::Error) -> Self {
        Self::new(e.to_string())
    }
}
impl From<serde_json::Error> for ProviderError {
    fn from(e: serde_json::Error) -> Self {
        Self::new(format!("bad response: {e}"))
    }
}

pub type PResult<T> = std::result::Result<T, ProviderError>;

/// Fill missing end times from the next line's start.
pub fn fill_ends(lines: &mut [Line], duration_ms: i64) {
    for i in 0..lines.len() {
        if lines[i].end_ms.is_none() {
            lines[i].end_ms = Some(lines.get(i + 1).map(|n| n.start_ms).unwrap_or(duration_ms.max(lines[i].start_ms)));
        }
    }
}
