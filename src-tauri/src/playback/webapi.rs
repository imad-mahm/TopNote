use crate::error::{msg, AppError, Result};
use crate::{auth, state::AppState};
use reqwest::Method;
use serde_json::Value;
use std::time::{Duration, Instant};

/// Call the Spotify Web API. Ok(None) for empty (204) responses.
pub async fn call(st: &AppState, method: Method, path: &str, body: Option<Value>) -> Result<Option<Value>> {
    if let Some(until) = *st.backoff_until.lock().unwrap() {
        let now = Instant::now();
        if now < until {
            return Err(AppError::RateLimited((until - now).as_secs().max(1)));
        }
    }
    for attempt in 0..2 {
        let token = auth::access_token(st).await?;
        let mut req = st
            .http
            .request(method.clone(), format!("https://api.spotify.com/v1{path}"))
            .bearer_auth(&token);
        req = match &body {
            Some(b) => req.json(b),
            None if method != Method::GET => req.header(reqwest::header::CONTENT_LENGTH, "0"),
            None => req,
        };
        let resp = req.send().await?;
        let status = resp.status().as_u16();
        match status {
            200..=299 => {
                let text = resp.text().await.unwrap_or_default();
                if text.trim().is_empty() {
                    return Ok(None);
                }
                return Ok(serde_json::from_str(&text).ok());
            }
            401 if attempt == 0 => {
                auth::invalidate(st).await;
                continue;
            }
            429 => {
                let secs = resp
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(5)
                    .clamp(1, 600);
                *st.backoff_until.lock().unwrap() = Some(Instant::now() + Duration::from_secs(secs));
                return Err(AppError::RateLimited(secs));
            }
            404 => {
                let text = resp.text().await.unwrap_or_default();
                if text.contains("NO_ACTIVE_DEVICE") || text.contains("No active device") {
                    return Err(AppError::NoDevice);
                }
                return Err(msg(format!("Spotify API 404: {text}")));
            }
            403 => {
                let text = resp.text().await.unwrap_or_default();
                return Err(msg(format!("Spotify refused the action (403): {text}")));
            }
            _ => {
                let text = resp.text().await.unwrap_or_default();
                return Err(msg(format!("Spotify API {status}: {text}")));
            }
        }
    }
    Err(AppError::NotLoggedIn)
}
