use super::lrc::parse_lrc;
use super::model::*;
use serde_json::Value;

const UA: &str = "spotify-miniplayer/0.1 (personal desktop app)";

pub fn to_lyrics(v: &Value, duration_ms: i64) -> Option<Lyrics> {
    if v["instrumental"].as_bool() == Some(true) {
        return Some(Lyrics::instrumental("lrclib"));
    }
    if let Some(s) = v["syncedLyrics"].as_str().filter(|s| !s.trim().is_empty()) {
        let mut lines = parse_lrc(s);
        if !lines.is_empty() {
            fill_ends(&mut lines, duration_ms);
            return Some(Lyrics::new("lrclib", SyncType::Line, lines));
        }
    }
    v["plainLyrics"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .map(|s| Lyrics::plain("lrclib", s))
}

pub async fn fetch(http: &reqwest::Client, meta: &TrackMeta) -> PResult<Option<Lyrics>> {
    let dur = (meta.duration_ms as f64 / 1000.0).round() as i64;
    let resp = http
        .get("https://lrclib.net/api/get")
        .header("User-Agent", UA)
        .query(&[
            ("track_name", meta.title.as_str()),
            ("artist_name", meta.artist()),
            ("album_name", meta.album.as_str()),
            ("duration", &dur.to_string()),
        ])
        .send()
        .await?;
    match resp.status().as_u16() {
        200 => {
            let v: Value = resp.json().await?;
            if let Some(l) = to_lyrics(&v, meta.duration_ms) {
                return Ok(Some(l));
            }
        }
        404 => {}
        s if s >= 500 => return Err(ProviderError::new(format!("LRCLIB {s}"))),
        _ => {}
    }

    // Fuzzy fallback: search and take the closest duration match (±2s), synced preferred.
    let resp = http
        .get("https://lrclib.net/api/search")
        .header("User-Agent", UA)
        .query(&[("track_name", meta.title.as_str()), ("artist_name", meta.artist())])
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(ProviderError::new(format!("LRCLIB search {}", resp.status())));
    }
    let list: Vec<Value> = resp.json().await?;
    let mut best: Option<(&Value, i64)> = None;
    for item in &list {
        let d = item["duration"].as_f64().unwrap_or(0.0).round() as i64;
        let diff = (d - dur).abs();
        if dur > 0 && diff > 2 {
            continue;
        }
        let synced = item["syncedLyrics"].as_str().map(|s| !s.is_empty()).unwrap_or(false);
        let score = diff - if synced { 100 } else { 0 };
        if best.map(|(_, b)| score < b).unwrap_or(true) {
            best = Some((item, score));
        }
    }
    Ok(best.and_then(|(v, _)| to_lyrics(v, meta.duration_ms)))
}
