//! Line-by-line translation and romanization through Google's public "gtx" endpoint.
//! Results are cached per (track, lyrics source, target language).
use crate::error::{msg, Result};
use crate::lyrics::cache;
use crate::playback::smtc::now_ms;
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Translation {
    /// Detected source language, e.g. "ja".
    pub lang: String,
    /// One entry per input line; None when empty or identical to the source.
    pub translated: Vec<Option<String>>,
    pub romanized: Vec<Option<String>>,
}

/// Lines per request; keeps bodies small and alignment robust.
const CHUNK: usize = 40;

fn norm(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

type Parsed = (String, Vec<Option<String>>, Vec<Option<String>>);

/// Parse one gtx response for input lines that were joined by '\n'.
pub fn parse(v: &Value, inputs: &[String]) -> Parsed {
    let lang = v[2].as_str().unwrap_or_default().to_string();
    let mut trans = String::new();
    let mut roman = String::new();
    for seg in v[0].as_array().into_iter().flatten() {
        if let Some(t) = seg[0].as_str() {
            trans.push_str(t);
        } else if let Some(r) = seg[3].as_str() {
            // romanization segment: [null, null, translit(target), translit(source)]
            roman.push_str(r);
        }
    }
    let align = |joined: &str| -> Vec<Option<String>> {
        let parts: Vec<&str> = joined.split('\n').collect();
        if parts.len() != inputs.len() {
            return vec![None; inputs.len()];
        }
        parts
            .iter()
            .zip(inputs)
            .map(|(out, src)| {
                let out = out.trim();
                (!out.is_empty() && !src.trim().is_empty() && norm(out) != norm(src)).then(|| out.to_string())
            })
            .collect()
    };
    let t = align(&trans);
    let r = if roman.is_empty() { vec![None; inputs.len()] } else { align(&roman) };
    (lang, t, r)
}

async fn chunk(st: &AppState, lines: &[String], to: &str) -> Result<Parsed> {
    // Empty lines would collapse; send a placeholder and blank it afterwards.
    let src: Vec<String> =
        lines.iter().map(|l| if l.trim().is_empty() { "…".to_string() } else { l.replace('\n', " ") }).collect();
    let q = src.join("\n");
    let resp = st
        .http
        .post("https://translate.googleapis.com/translate_a/single")
        .query(&[("client", "gtx"), ("sl", "auto"), ("tl", to), ("dt", "t"), ("dt", "rm"), ("ie", "UTF-8"), ("oe", "UTF-8")])
        .form(&[("q", q.as_str())])
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(msg(format!("translate HTTP {}", resp.status())));
    }
    let v: Value = resp.json().await?;
    let (lang, mut t, mut r) = parse(&v, &src);
    for (i, l) in lines.iter().enumerate() {
        if l.trim().is_empty() {
            t[i] = None;
            r[i] = None;
        }
    }
    Ok((lang, t, r))
}

pub async fn translate(st: &AppState, key: &str, lines: Vec<String>, to: &str) -> Result<Translation> {
    let ck = format!("{key}|{to}");
    let hit = cache::get_translation(&st.lyrics.cache.lock().unwrap(), &ck);
    if let Some(t) = hit.and_then(|j| serde_json::from_str::<Translation>(&j).ok()) {
        if t.translated.len() == lines.len() {
            return Ok(t);
        }
    }
    let mut out = Translation::default();
    for part in lines.chunks(CHUNK) {
        let (lang, t, r) = chunk(st, part, to).await?;
        if out.lang.is_empty() {
            out.lang = lang;
        }
        out.translated.extend(t);
        out.romanized.extend(r);
    }
    // Already in the target language: nothing to translate.
    if out.lang == to {
        out.translated = vec![None; lines.len()];
    }
    if let Ok(j) = serde_json::to_string(&out) {
        cache::put_translation(&st.lyrics.cache.lock().unwrap(), &ck, &j, now_ms());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn parses_and_aligns() {
        let inputs = vec!["こんにちは".to_string(), "hello".to_string()];
        let v = json!([
            [["Hello\n", "こんにちは\n", null, null], ["hello", "hello", null, null], [null, null, "Hello\nhello", "Konnichiwa\nhello"]],
            null,
            "ja"
        ]);
        let (lang, t, r) = parse(&v, &inputs);
        assert_eq!(lang, "ja");
        assert_eq!(t, vec![Some("Hello".into()), None]);
        assert_eq!(r, vec![Some("Konnichiwa".into()), None]);
    }
    #[test]
    fn misaligned_is_dropped() {
        let inputs = vec!["a".to_string(), "b".to_string()];
        let v = json!([[["x y z", "a\nb", null, null]], null, "fr"]);
        let (_, t, _) = parse(&v, &inputs);
        assert_eq!(t, vec![None, None]);
    }
}
