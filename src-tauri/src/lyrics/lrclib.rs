use super::lrc::parse_lrc;
use super::model::*;
use serde::Serialize;
use serde_json::Value;

const UA: &str = "TopNote/1.0 (https://github.com/imad-mahm/TopNote)";

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

/// A search hit the user can pick by hand.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub provider: String,
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: i64,
    pub synced: bool,
    pub has_plain: bool,
    pub instrumental: bool,
    /// First couple of lyric lines, to tell versions apart.
    pub preview: String,
}

fn candidate(v: &Value) -> Option<Candidate> {
    let synced = v["syncedLyrics"].as_str().map(|s| !s.trim().is_empty()).unwrap_or(false);
    let plain = v["plainLyrics"].as_str().unwrap_or_default();
    let preview = plain.lines().filter(|l| !l.trim().is_empty()).take(2).collect::<Vec<_>>().join(" / ");
    Some(Candidate {
        provider: "lrclib".into(),
        id: v["id"].as_i64()?.to_string(),
        title: v["trackName"].as_str().unwrap_or_default().into(),
        artist: v["artistName"].as_str().unwrap_or_default().into(),
        album: v["albumName"].as_str().unwrap_or_default().into(),
        duration_ms: (v["duration"].as_f64().unwrap_or(0.0) * 1000.0).round() as i64,
        synced,
        has_plain: !plain.trim().is_empty(),
        instrumental: v["instrumental"].as_bool() == Some(true),
        preview,
    })
}

/// Free-text search, synced results first.
pub async fn search(http: &reqwest::Client, q: &str) -> PResult<Vec<Candidate>> {
    let resp = http.get("https://lrclib.net/api/search").header("User-Agent", UA).query(&[("q", q)]).send().await?;
    if !resp.status().is_success() {
        return Err(ProviderError::new(format!("LRCLIB search {}", resp.status())));
    }
    let list: Vec<Value> = resp.json().await?;
    let mut out: Vec<Candidate> = list.iter().filter_map(candidate).collect();
    out.sort_by_key(|c| !c.synced);
    out.truncate(30);
    Ok(out)
}

pub async fn get_by_id(http: &reqwest::Client, id: &str, duration_ms: i64) -> PResult<Option<Lyrics>> {
    let id: i64 = id.parse().map_err(|_| ProviderError::new("bad LRCLIB id"))?;
    let resp = http.get(format!("https://lrclib.net/api/get/{id}")).header("User-Agent", UA).send().await?;
    if resp.status().as_u16() == 404 {
        return Ok(None);
    }
    if !resp.status().is_success() {
        return Err(ProviderError::new(format!("LRCLIB {}", resp.status())));
    }
    let v: Value = resp.json().await?;
    Ok(to_lyrics(&v, duration_ms))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn candidate_parse() {
        let v = serde_json::json!({"id": 7, "trackName": "T", "artistName": "A", "albumName": "B", "duration": 201.4,
            "instrumental": false, "plainLyrics": "one\n\ntwo\nthree", "syncedLyrics": "[00:01.00]one"});
        let c = candidate(&v).unwrap();
        assert_eq!(c.id, "7");
        assert_eq!(c.duration_ms, 201400);
        assert!(c.synced);
        assert_eq!(c.preview, "one / two");
    }
}
