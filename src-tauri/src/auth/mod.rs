//! Spotify Web API auth: Authorization Code + PKCE with a loopback redirect.
use crate::error::{msg, AppError, Result};
use crate::{secrets, state::AppState};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::{distributions::Alphanumeric, Rng};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

pub const REDIRECT_URI: &str = "http://127.0.0.1:8898/callback";
const SCOPES: &str = "user-read-playback-state user-modify-playback-state user-read-currently-playing user-library-read user-library-modify";
const REFRESH_KEY: &str = "refresh_token";

pub struct Tokens {
    pub access: String,
    pub expires_at: Instant,
}

pub fn logged_in() -> bool {
    secrets::get(REFRESH_KEY).is_some()
}

fn random_string(n: usize) -> String {
    rand::thread_rng().sample_iter(&Alphanumeric).take(n).map(char::from).collect()
}

pub fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

pub async fn login(st: &AppState) -> Result<()> {
    let client_id = st.settings().client_id;
    if client_id.trim().is_empty() {
        return Err(msg("Enter your Spotify Client ID first"));
    }
    let verifier = random_string(64);
    let csrf = random_string(16);
    let listener = TcpListener::bind("127.0.0.1:8898")
        .await
        .map_err(|e| msg(format!("Port 8898 is busy ({e}). Close whatever uses it and retry.")))?;

    let mut url = url::Url::parse("https://accounts.spotify.com/authorize").unwrap();
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", client_id.trim())
        .append_pair("scope", SCOPES)
        .append_pair("redirect_uri", REDIRECT_URI)
        .append_pair("code_challenge_method", "S256")
        .append_pair("code_challenge", &challenge(&verifier))
        .append_pair("state", &csrf);
    tauri_plugin_opener::open_url(url.as_str(), None::<&str>).map_err(|e| msg(e.to_string()))?;

    let code = tokio::time::timeout(Duration::from_secs(180), wait_for_code(listener, &csrf))
        .await
        .map_err(|_| msg("Login timed out"))??;

    let tokens = token_request(
        st,
        &[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", REDIRECT_URI),
            ("client_id", client_id.trim()),
            ("code_verifier", &verifier),
        ],
    )
    .await?;
    *st.auth.lock().await = Some(tokens);
    Ok(())
}

async fn wait_for_code(listener: TcpListener, csrf: &str) -> Result<String> {
    loop {
        let (mut sock, _) = listener.accept().await?;
        let mut buf = vec![0u8; 8192];
        let n = sock.read(&mut buf).await.unwrap_or(0);
        let req = String::from_utf8_lossy(&buf[..n]).to_string();
        let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();
        if !path.starts_with("/callback") {
            let _ = sock.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n").await;
            continue;
        }
        let params: HashMap<String, String> = url::Url::parse(&format!("http://localhost{path}"))
            .map(|u| u.query_pairs().into_owned().collect())
            .unwrap_or_default();
        let (ok, body) = match (params.get("code"), params.get("error")) {
            (Some(_), _) if params.get("state").map(String::as_str) == Some(csrf) => (true, "You're logged in. You can close this tab and go back to the miniplayer."),
            (Some(_), _) => (false, "Login failed: state mismatch. Try again."),
            (_, Some(_)) => (false, "Login was cancelled."),
            _ => (false, "Login failed."),
        };
        let html = format!(
            "<!doctype html><meta charset=utf-8><title>Miniplayer</title><body style=\"font:16px system-ui;background:#111;color:#eee;display:grid;place-items:center;height:100vh;margin:0\"><p>{body}</p>"
        );
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            html.len(),
            html
        );
        let _ = sock.write_all(resp.as_bytes()).await;
        let _ = sock.shutdown().await;
        return if ok {
            Ok(params["code"].clone())
        } else {
            Err(msg(body))
        };
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
    refresh_token: Option<String>,
}

async fn token_request(st: &AppState, form: &[(&str, &str)]) -> Result<Tokens> {
    let resp = st
        .http
        .post("https://accounts.spotify.com/api/token")
        .form(form)
        .send()
        .await?;
    let status = resp.status();
    let text = resp.text().await?;
    if !status.is_success() {
        if text.contains("invalid_grant") {
            secrets::delete(REFRESH_KEY);
            return Err(AppError::NotLoggedIn);
        }
        return Err(msg(format!("Spotify token error {status}: {text}")));
    }
    let tr: TokenResponse = serde_json::from_str(&text)?;
    if let Some(r) = tr.refresh_token {
        secrets::set(REFRESH_KEY, &r)?;
    }
    Ok(Tokens {
        access: tr.access_token,
        expires_at: Instant::now() + Duration::from_secs(tr.expires_in),
    })
}

/// Valid access token, refreshing 60s before expiry.
pub async fn access_token(st: &AppState) -> Result<String> {
    let mut guard = st.auth.lock().await;
    if let Some(t) = guard.as_ref() {
        if t.expires_at > Instant::now() + Duration::from_secs(60) {
            return Ok(t.access.clone());
        }
    }
    let refresh = secrets::get(REFRESH_KEY).ok_or(AppError::NotLoggedIn)?;
    let client_id = st.settings().client_id;
    let t = token_request(
        st,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", &refresh),
            ("client_id", client_id.trim()),
        ],
    )
    .await?;
    let access = t.access.clone();
    *guard = Some(t);
    Ok(access)
}

pub async fn invalidate(st: &AppState) {
    *st.auth.lock().await = None;
}

pub async fn logout(st: &AppState) {
    secrets::delete(REFRESH_KEY);
    invalidate(st).await;
}

#[cfg(test)]
mod tests {
    #[test]
    fn pkce_challenge_rfc7636() {
        // RFC 7636 Appendix B
        assert_eq!(
            super::challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }
}
