//! Musixmatch via the (unofficial) desktop-app API.
use super::model::*;
use super::LyricsService;
use rand::Rng;
use serde_json::Value;
use std::time::Duration;

const BASE: &str = "https://apic-desktop.musixmatch.com/ws/1.1/";
const APP_ID: &str = "web-desktop-app-v1.0";
const COOKIE: &str = "AWSELB=0; AWSELBCORS=0";

fn status(v: &Value) -> i64 {
    v["message"]["header"]["status_code"].as_i64().unwrap_or(0)
}

async fn get(http: &reqwest::Client, method: &str, params: &[(&str, String)]) -> PResult<Value> {
    let resp = http
        .get(format!("{BASE}{method}"))
        .header("Cookie", COOKIE)
        .header("authority", "apic-desktop.musixmatch.com")
        .query(params)
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(ProviderError::new(format!("Musixmatch HTTP {}", resp.status())));
    }
    Ok(resp.json().await?)
}

async fn token(svc: &LyricsService, http: &reqwest::Client) -> PResult<String> {
    if let Some(t) = svc.mxm_token.lock().await.clone() {
        return Ok(t);
    }
    let t: u64 = rand::thread_rng().gen();
    let v = get(
        http,
        "token.get",
        &[("app_id", APP_ID.into()), ("user_language", "en".into()), ("t", t.to_string())],
    )
    .await?;
    if status(&v) == 401 {
        return Err(ProviderError::cooldown("Musixmatch captcha / rate limit", Duration::from_secs(15 * 60)));
    }
    let tok = v["message"]["body"]["user_token"].as_str().unwrap_or_default().to_string();
    if tok.is_empty() || tok.starts_with("UpgradeOnly") {
        return Err(ProviderError::cooldown("Musixmatch token refused", Duration::from_secs(5 * 60)));
    }
    *svc.mxm_token.lock().await = Some(tok.clone());
    Ok(tok)
}

pub fn parse_subtitles(body: &str) -> PResult<Vec<Line>> {
    let arr: Vec<Value> = serde_json::from_str(body)?;
    Ok(arr
        .iter()
        .map(|e| Line {
            start_ms: (e["time"]["total"].as_f64().unwrap_or(0.0) * 1000.0).round() as i64,
            end_ms: None,
            text: e["text"].as_str().unwrap_or_default().to_string(),
            words: None,
        })
        .collect())
}

pub fn parse_richsync(body: &str) -> PResult<Vec<Line>> {
    let arr: Vec<Value> = serde_json::from_str(body)?;
    Ok(arr
        .iter()
        .map(|e| {
            let ts = e["ts"].as_f64().unwrap_or(0.0);
            let te = e["te"].as_f64().unwrap_or(ts);
            let mut words: Vec<Word> = Vec::new();
            for w in e["l"].as_array().into_iter().flatten() {
                let c = w["c"].as_str().unwrap_or_default();
                let start = ((ts + w["o"].as_f64().unwrap_or(0.0)) * 1000.0).round() as i64;
                // Spaces/punctuation-only fragments attach to the previous word.
                match words.last_mut() {
                    Some(last) if c.trim().is_empty() => last.text.push_str(c),
                    _ => words.push(Word { start_ms: start, text: c.to_string() }),
                }
            }
            Line {
                start_ms: (ts * 1000.0).round() as i64,
                end_ms: Some((te * 1000.0).round() as i64),
                text: e["x"].as_str().unwrap_or_default().to_string(),
                words: (!words.is_empty()).then_some(words),
            }
        })
        .collect())
}

pub async fn fetch(svc: &LyricsService, http: &reqwest::Client, meta: &TrackMeta, word_sync: bool) -> PResult<Option<Lyrics>> {
    let tok = token(svc, http).await?;
    let dur = ((meta.duration_ms as f64) / 1000.0).round() as i64;
    let mut params: Vec<(&str, String)> = vec![
        ("format", "json".into()),
        ("namespace", "lyrics_richsynched".into()),
        ("subtitle_format", "mxm".into()),
        ("app_id", APP_ID.into()),
        ("usertoken", tok.clone()),
        ("q_album", meta.album.clone()),
        ("q_artist", meta.artist().into()),
        ("q_artists", meta.artists.join(",")),
        ("q_track", meta.title.clone()),
        ("q_duration", dur.to_string()),
        ("f_subtitle_length", dur.to_string()),
    ];
    if let Some(id) = meta.spotify_id() {
        params.push(("track_spotify_id", format!("spotify:track:{id}")));
    }
    if let Some(isrc) = &meta.isrc {
        params.push(("track_isrc", isrc.clone()));
    }
    let v = get(http, "macro.subtitles.get", &params).await?;
    if status(&v) == 401 {
        *svc.mxm_token.lock().await = None;
        let hint = v["message"]["header"]["hint"].as_str().unwrap_or("");
        return Err(if hint == "captcha" {
            ProviderError::cooldown("Musixmatch captcha", Duration::from_secs(15 * 60))
        } else {
            ProviderError::new("Musixmatch token expired")
        });
    }
    let calls = &v["message"]["body"]["macro_calls"];
    let matcher = &calls["matcher.track.get"]["message"];
    if status(matcher) == 404 {
        return Ok(None);
    }
    let track = &matcher["body"]["track"];
    if track["instrumental"].as_i64() == Some(1) {
        return Ok(Some(Lyrics::instrumental("musixmatch")));
    }

    if word_sync && track["has_richsync"].as_i64() == Some(1) {
        if let Some(ct) = track["commontrack_id"].as_i64() {
            let rv = get(
                http,
                "track.richsync.get",
                &[
                    ("format", "json".into()),
                    ("subtitle_format", "mxm".into()),
                    ("app_id", APP_ID.into()),
                    ("usertoken", tok.clone()),
                    ("commontrack_id", ct.to_string()),
                ],
            )
            .await;
            if let Ok(rv) = rv {
                if let Some(body) = rv["message"]["body"]["richsync"]["richsync_body"].as_str() {
                    if let Ok(lines) = parse_richsync(body) {
                        if !lines.is_empty() {
                            return Ok(Some(Lyrics::new("musixmatch", SyncType::Word, lines)));
                        }
                    }
                }
            }
        }
    }

    if let Some(body) = calls["track.subtitles.get"]["message"]["body"]["subtitle_list"][0]["subtitle"]["subtitle_body"].as_str() {
        let mut lines = parse_subtitles(body)?;
        if !lines.is_empty() {
            fill_ends(&mut lines, meta.duration_ms);
            return Ok(Some(Lyrics::new("musixmatch", SyncType::Line, lines)));
        }
    }

    let lyr = &calls["track.lyrics.get"]["message"]["body"]["lyrics"];
    if lyr["instrumental"].as_i64() == Some(1) {
        return Ok(Some(Lyrics::instrumental("musixmatch")));
    }
    Ok(lyr["lyrics_body"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .map(|s| Lyrics::plain("musixmatch", s)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn richsync_words() {
        let body = r#"[{"ts":12.5,"te":15.0,"l":[{"c":"Hello","o":0},{"c":" ","o":0.4},{"c":"world","o":0.5}],"x":"Hello world"}]"#;
        let l = parse_richsync(body).unwrap();
        assert_eq!(l[0].start_ms, 12500);
        assert_eq!(l[0].end_ms, Some(15000));
        let w = l[0].words.as_ref().unwrap();
        assert_eq!(w.len(), 2);
        assert_eq!(w[0].text, "Hello ");
        assert_eq!(w[1].start_ms, 13000);
    }
    #[test]
    fn subtitles() {
        let body = r#"[{"text":"A","time":{"total":1.25}},{"text":"","time":{"total":4}}]"#;
        let l = parse_subtitles(body).unwrap();
        assert_eq!(l[0].start_ms, 1250);
        assert_eq!(l[1].text, "");
    }
}
