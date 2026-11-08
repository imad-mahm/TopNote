//! Spotify's own lyrics (color-lyrics) using the web player's token, obtained with the
//! user's `sp_dc` cookie + TOTP. Unofficial: secrets rotate, see `settings.spotify_secrets_url`.
use super::model::*;
use super::LyricsService;
use crate::{secrets, state::USER_AGENT};
use hmac::{Hmac, Mac};
use serde_json::Value;
use sha1::Sha1;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Last-known cipher, used when the remote secrets list can't be fetched.
const FALLBACK_VERSION: &str = "61";
const FALLBACK_CIPHER: &[u8] = &[
    44, 55, 47, 42, 70, 40, 34, 114, 76, 74, 50, 111, 120, 97, 75, 76, 94, 102, 43, 69, 49, 120, 118, 80, 64, 78,
];

pub fn derive_key(cipher: &[u8]) -> Vec<u8> {
    cipher
        .iter()
        .enumerate()
        .map(|(i, b)| (b ^ ((i % 33) as u8 + 9)).to_string())
        .collect::<String>()
        .into_bytes()
}

pub fn totp(key: &[u8], unix_secs: u64) -> String {
    let counter = unix_secs / 30;
    let mut mac = Hmac::<Sha1>::new_from_slice(key).expect("hmac key");
    mac.update(&counter.to_be_bytes());
    let r = mac.finalize().into_bytes();
    let o = (r[19] & 0x0f) as usize;
    let code = ((r[o] as u32 & 0x7f) << 24) | ((r[o + 1] as u32) << 16) | ((r[o + 2] as u32) << 8) | r[o + 3] as u32;
    format!("{:06}", code % 1_000_000)
}

pub fn parse_secret_dict(v: &Value) -> Option<(String, Vec<u8>)> {
    v.as_object()?
        .iter()
        .filter_map(|(k, arr)| {
            let ver: u32 = k.parse().ok()?;
            let bytes: Vec<u8> = arr.as_array()?.iter().filter_map(|b| b.as_u64().map(|n| n as u8)).collect();
            Some((ver, k.clone(), bytes))
        })
        .max_by_key(|(ver, _, _)| *ver)
        .map(|(_, k, b)| (k, b))
}

async fn secret(svc: &LyricsService, http: &reqwest::Client, url: &str) -> (String, Vec<u8>) {
    let mut g = svc.spotify_secret.lock().await;
    if let Some((ver, bytes, at)) = g.as_ref() {
        if at.elapsed() < Duration::from_secs(3600) {
            return (ver.clone(), bytes.clone());
        }
    }
    let fetched = async {
        let v: Value = http.get(url).timeout(Duration::from_secs(3)).send().await.ok()?.json().await.ok()?;
        parse_secret_dict(&v)
    }
    .await;
    let (ver, bytes) = fetched.unwrap_or_else(|| (FALLBACK_VERSION.into(), FALLBACK_CIPHER.to_vec()));
    *g = Some((ver.clone(), bytes.clone(), Instant::now()));
    (ver, bytes)
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

async fn access_token(svc: &LyricsService, http: &reqwest::Client, sp_dc: &str, secrets_url: &str) -> PResult<String> {
    let mut g = svc.spotify_token.lock().await;
    let now_ms = (now_secs() * 1000) as i64;
    if let Some((tok, exp)) = g.as_ref() {
        if *exp > now_ms + 30_000 {
            return Ok(tok.clone());
        }
    }
    let (ver, cipher) = secret(svc, http, secrets_url).await;
    let code = totp(&derive_key(&cipher), now_secs());
    let resp = http
        .get("https://open.spotify.com/api/token")
        .header("User-Agent", USER_AGENT)
        .header("Cookie", format!("sp_dc={sp_dc}"))
        .query(&[
            ("reason", "init"),
            ("productType", "web-player"),
            ("totp", &code),
            ("totpServer", &code),
            ("totpVer", &ver),
        ])
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(ProviderError::new(format!(
            "Spotify web token HTTP {} (secret v{ver} may be outdated)",
            resp.status()
        )));
    }
    let v: Value = resp.json().await?;
    if v["isAnonymous"].as_bool() == Some(true) {
        return Err(ProviderError::cooldown(
            "sp_dc cookie is invalid or expired — paste a fresh one",
            Duration::from_secs(10 * 60),
        ));
    }
    let tok = v["accessToken"].as_str().unwrap_or_default().to_string();
    if tok.is_empty() {
        return Err(ProviderError::new("Spotify web token missing"));
    }
    let exp = v["accessTokenExpirationTimestampMs"].as_i64().unwrap_or(now_ms + 3_000_000);
    *g = Some((tok.clone(), exp));
    Ok(tok)
}

fn hex(n: &Value) -> String {
    let v = n.as_i64().unwrap_or(0) as u32 & 0x00ff_ffff;
    format!("#{v:06x}")
}

pub fn parse_color_lyrics(v: &Value, duration_ms: i64) -> Option<Lyrics> {
    let l = &v["lyrics"];
    let sync = match l["syncType"].as_str()? {
        "LINE_SYNCED" => SyncType::Line,
        "SYLLABLE_SYNCED" => SyncType::Line,
        _ => SyncType::None,
    };
    let mut lines: Vec<Line> = l["lines"]
        .as_array()?
        .iter()
        .map(|x| {
            let text = x["words"].as_str().unwrap_or_default();
            Line {
                start_ms: x["startTimeMs"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0),
                end_ms: x["endTimeMs"].as_str().and_then(|s| s.parse().ok()).filter(|e: &i64| *e > 0),
                text: if text.trim() == "♪" { String::new() } else { text.to_string() },
                words: None,
            }
        })
        .collect();
    if lines.is_empty() {
        return None;
    }
    if sync != SyncType::None {
        fill_ends(&mut lines, duration_ms);
    }
    let colors = v["colors"].is_object().then(|| Colors {
        background: hex(&v["colors"]["background"]),
        text: hex(&v["colors"]["text"]),
        highlight: hex(&v["colors"]["highlightText"]),
    });
    Some(Lyrics {
        colors,
        language: l["language"].as_str().map(String::from),
        ..Lyrics::new("spotify", sync, lines)
    })
}

pub async fn fetch(svc: &LyricsService, http: &reqwest::Client, meta: &TrackMeta, secrets_url: &str) -> PResult<Option<Lyrics>> {
    let Some(id) = meta.spotify_id() else {
        return Err(ProviderError::skip("no Spotify track id"));
    };
    let Some(sp_dc) = secrets::get("sp_dc") else {
        return Err(ProviderError::skip("no sp_dc cookie set"));
    };
    let tok = access_token(svc, http, &sp_dc, secrets_url).await?;
    let resp = http
        .get(format!("https://spclient.wg.spotify.com/color-lyrics/v2/track/{id}"))
        .query(&[("format", "json"), ("vocalRemoval", "false"), ("market", "from_token")])
        .header("User-Agent", USER_AGENT)
        .header("App-Platform", "WebPlayer")
        .header("Authorization", format!("Bearer {tok}"))
        .send()
        .await?;
    match resp.status().as_u16() {
        200 => {
            let v: Value = resp.json().await?;
            Ok(parse_color_lyrics(&v, meta.duration_ms))
        }
        404 => Ok(None),
        401 => {
            *svc.spotify_token.lock().await = None;
            Err(ProviderError::new("Spotify lyrics token rejected"))
        }
        429 => Err(ProviderError::cooldown("Spotify lyrics rate limited", Duration::from_secs(120))),
        s => Err(ProviderError::new(format!("Spotify lyrics HTTP {s}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn totp_rfc6238() {
        // RFC 6238 SHA1 vector, truncated to 6 digits.
        assert_eq!(totp(b"12345678901234567890", 59), "287082");
        assert_eq!(totp(b"12345678901234567890", 1111111109), "081804");
    }
    #[test]
    fn picks_highest_secret_version() {
        let v = serde_json::json!({"9": [1,2], "12": [3,4], "10": [5]});
        assert_eq!(parse_secret_dict(&v), Some(("12".into(), vec![3, 4])));
    }
    #[test]
    fn parses_color_lyrics() {
        let v = serde_json::json!({
            "lyrics": {"syncType": "LINE_SYNCED", "language": "en", "lines": [
                {"startTimeMs": "1000", "words": "Hi", "endTimeMs": "0"},
                {"startTimeMs": "3000", "words": "♪", "endTimeMs": "0"}
            ]},
            "colors": {"background": -9013642, "text": -16777216, "highlightText": -1}
        });
        let l = parse_color_lyrics(&v, 10000).unwrap();
        assert_eq!(l.sync, SyncType::Line);
        assert_eq!(l.lines[0].end_ms, Some(3000));
        assert_eq!(l.lines[1].text, "");
        assert_eq!(l.colors.unwrap().highlight, "#ffffff");
    }
}
