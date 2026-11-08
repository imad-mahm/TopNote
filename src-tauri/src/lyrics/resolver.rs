//! Provider chain: priority order, per-provider timeout, hedged start of the next provider
//! when the current one is slow, circuit breaker, cache.
use super::model::*;
use super::{cache, lrclib, musixmatch, spotify, Health};
use crate::error::Result;
use crate::playback::smtc::now_ms;
use crate::state::AppState;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::task::JoinHandle;

const PROVIDER_TIMEOUT: Duration = Duration::from_secs(4);
const HEDGE_AFTER: Duration = Duration::from_millis(1500);
const BREAKER_FAILS: u32 = 3;
const BREAKER_COOLDOWN: Duration = Duration::from_secs(10 * 60);

pub const PROVIDERS: [&str; 3] = ["spotify", "musixmatch", "lrclib"];

#[derive(Debug)]
pub enum Outcome {
    Found(Lyrics),
    NotFound,
    Skipped(String),
    Failed(String),
    Timeout,
}

type BoxFut = Pin<Box<dyn Future<Output = PResult<Option<Lyrics>>> + Send>>;

/// The function each provider slot runs. Swappable for tests.
pub type Runner = Arc<dyn Fn(String, TrackMeta) -> BoxFut + Send + Sync>;

pub fn live_runner(st: Arc<AppState>) -> Runner {
    Arc::new(move |name: String, meta: TrackMeta| {
        let st = st.clone();
        Box::pin(async move {
            let s = st.settings();
            match name.as_str() {
                "spotify" => spotify::fetch(&st.lyrics, &st.http, &meta, &s.spotify_secrets_url).await,
                "musixmatch" => musixmatch::fetch(&st.lyrics, &st.http, &meta, s.word_sync).await,
                "lrclib" => lrclib::fetch(&st.http, &meta).await,
                other => Err(ProviderError::skip(format!("unknown provider {other}"))),
            }
        }) as BoxFut
    })
}

fn breaker_open(st: &AppState, name: &str) -> Option<u64> {
    let h = st.lyrics.health.lock().unwrap();
    let until = h.get(name)?.until?;
    let now = Instant::now();
    (until > now).then(|| (until - now).as_secs())
}

fn record(st: &AppState, name: &str, res: &std::result::Result<Option<Lyrics>, ProviderError>) {
    let mut map = st.lyrics.health.lock().unwrap();
    let h = map.entry(name.to_string()).or_insert_with(Health::default);
    match res {
        Ok(_) => {
            h.fails = 0;
            h.until = None;
            h.last_error = None;
        }
        Err(e) if e.skip => {
            h.last_error = Some(e.msg.clone());
        }
        Err(e) => {
            h.fails += 1;
            h.last_error = Some(e.msg.clone());
            if let Some(cd) = e.cooldown {
                h.until = Some(Instant::now() + cd);
            } else if h.fails >= BREAKER_FAILS {
                h.until = Some(Instant::now() + BREAKER_COOLDOWN);
            }
        }
    }
}

fn run_one(st: Option<Arc<AppState>>, runner: Runner, name: String, meta: TrackMeta) -> JoinHandle<Outcome> {
    tokio::spawn(async move {
        if let Some(st) = &st {
            if let Some(secs) = breaker_open(st, &name) {
                return Outcome::Skipped(format!("paused for {secs}s after repeated errors"));
            }
        }
        match tokio::time::timeout(PROVIDER_TIMEOUT, runner(name.clone(), meta)).await {
            Err(_) => {
                if let Some(st) = &st {
                    record(st, &name, &Err(ProviderError::new("timed out")));
                }
                Outcome::Timeout
            }
            Ok(res) => {
                if let Some(st) = &st {
                    record(st, &name, &res);
                }
                match res {
                    Ok(Some(l)) => Outcome::Found(l),
                    Ok(None) => Outcome::NotFound,
                    Err(e) if e.skip => Outcome::Skipped(e.msg),
                    Err(e) => Outcome::Failed(e.msg),
                }
            }
        }
    })
}

/// Run providers in priority order; stop at the first synced result, otherwise keep the first
/// unsynced one. If a provider is slower than HEDGE_AFTER, the next one is started in parallel.
pub async fn run_chain(st: Option<Arc<AppState>>, runner: Runner, order: &[String], meta: &TrackMeta) -> (Option<Lyrics>, Vec<Attempt>) {
    let n = order.len();
    let mut handles: Vec<Option<JoinHandle<Outcome>>> = (0..n).map(|_| None).collect();
    let mut started = 0usize;
    let start = |i: usize, handles: &mut Vec<Option<JoinHandle<Outcome>>>| {
        handles[i] = Some(run_one(st.clone(), runner.clone(), order[i].clone(), meta.clone()));
    };
    let mut attempts = Vec::new();
    let mut fallback: Option<Lyrics> = None;

    for i in 0..n {
        if started <= i {
            start(i, &mut handles);
            started = i + 1;
        }
        let mut h = handles[i].take().unwrap();
        let outcome = loop {
            if started < n {
                tokio::select! {
                    r = &mut h => break r.unwrap_or(Outcome::Failed("task panicked".into())),
                    _ = tokio::time::sleep(HEDGE_AFTER) => {
                        start(started, &mut handles);
                        started += 1;
                    }
                }
            } else {
                break (&mut h).await.unwrap_or(Outcome::Failed("task panicked".into()));
            }
        };
        let (label, detail, lyr) = match outcome {
            Outcome::Found(l) if l.is_synced() => ("synced", None, Some(l)),
            Outcome::Found(l) => ("unsynced", None, Some(l)),
            Outcome::NotFound => ("notFound", None, None),
            Outcome::Skipped(m) => ("skipped", Some(m), None),
            Outcome::Failed(m) => ("failed", Some(m), None),
            Outcome::Timeout => ("timeout", None, None),
        };
        attempts.push(Attempt { provider: order[i].clone(), outcome: label.into(), detail });
        match lyr {
            Some(l) if l.is_synced() => {
                for h in handles.iter_mut().flatten() {
                    h.abort();
                }
                return (Some(l), attempts);
            }
            Some(l) if fallback.is_none() => fallback = Some(l),
            _ => {}
        }
    }
    (fallback, attempts)
}

pub async fn resolve(st: Arc<AppState>, meta: TrackMeta, force: bool, only: Option<String>) -> Result<LyricsResult> {
    if only.is_none() {
        let c = st.lyrics.cache.lock().unwrap();
        if force {
            // "Refetch" goes back to automatic lyrics.
            cache::delete_pick(&c, &meta.id);
        } else if let Some(pick) = cache::get_pick(&c, &meta.id) {
            return Ok(pick);
        }
    }
    if !force && only.is_none() {
        let hit = cache::get(&st.lyrics.cache.lock().unwrap(), &meta.id, now_ms());
        if let Some(hit) = hit {
            return Ok(hit);
        }
        // Join an in-flight resolution for this track (e.g. a queue prefetch) instead of duplicating it.
        let cell = st.lyrics.inflight.lock().unwrap().entry(meta.id.clone()).or_default().clone();
        let id = meta.id.clone();
        let st2 = st.clone();
        let out = cell.get_or_init(|| resolve_uncached(st2, meta, None)).await.clone();
        st.lyrics.inflight.lock().unwrap().remove(&id);
        return Ok(out);
    }
    Ok(resolve_uncached(st, meta, only).await)
}

async fn resolve_uncached(st: Arc<AppState>, meta: TrackMeta, only: Option<String>) -> LyricsResult {
    let order: Vec<String> = match &only {
        Some(p) => vec![p.clone()],
        None => st.settings().provider_order,
    };
    let (lyrics, attempts) = run_chain(Some(st.clone()), live_runner(st.clone()), &order, &meta).await;
    let result = LyricsResult { track_id: meta.id.clone(), lyrics, attempts, cached: false };
    // Explicit source picks are remembered; a failed explicit pick doesn't overwrite a good cache entry.
    let all_failed = result.attempts.iter().all(|a| a.outcome == "failed" || a.outcome == "timeout");
    if (only.is_none() && !all_failed) || result.lyrics.is_some() {
        let _ = cache::put(&st.lyrics.cache.lock().unwrap(), meta.isrc.as_deref(), &result, now_ms());
    }
    result
}

/// Is there a fresh cache entry (or manual pick) for this track?
pub fn is_cached(st: &AppState, id: &str) -> bool {
    lyrics_ready(st, id).is_some()
}

/// None = not resolved yet; Some(found).
pub fn lyrics_ready(st: &AppState, id: &str) -> Option<bool> {
    cache::found(&st.lyrics.cache.lock().unwrap(), id, now_ms())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta() -> TrackMeta {
        TrackMeta { id: "x".into(), title: "t".into(), artists: vec!["a".into()], album: "b".into(), duration_ms: 1000, isrc: None }
    }

    fn runner(spec: Vec<(&'static str, u64, &'static str)>) -> Runner {
        Arc::new(move |name: String, _m: TrackMeta| {
            let (_, delay, kind) = *spec.iter().find(|(n, _, _)| *n == name).unwrap();
            Box::pin(async move {
                tokio::time::sleep(Duration::from_millis(delay)).await;
                match kind {
                    "synced" => Ok(Some(Lyrics::new(&name, SyncType::Line, vec![]))),
                    "plain" => Ok(Some(Lyrics::plain(&name, "x"))),
                    "none" => Ok(None),
                    "skip" => Err(ProviderError::skip("nope")),
                    _ => Err(ProviderError::new("boom")),
                }
            }) as BoxFut
        })
    }

    fn order() -> Vec<String> {
        PROVIDERS.iter().map(|s| s.to_string()).collect()
    }

    #[tokio::test]
    async fn first_synced_wins() {
        let r = runner(vec![("spotify", 10, "synced"), ("musixmatch", 10, "synced"), ("lrclib", 10, "synced")]);
        let (l, a) = run_chain(None, r, &order(), &meta()).await;
        assert_eq!(l.unwrap().source, "spotify");
        assert_eq!(a.len(), 1);
    }

    #[tokio::test]
    async fn falls_through_failures_and_skips() {
        let r = runner(vec![("spotify", 5, "skip"), ("musixmatch", 5, "fail"), ("lrclib", 5, "synced")]);
        let (l, a) = run_chain(None, r, &order(), &meta()).await;
        assert_eq!(l.unwrap().source, "lrclib");
        let outcomes: Vec<_> = a.iter().map(|x| x.outcome.as_str()).collect();
        assert_eq!(outcomes, ["skipped", "failed", "synced"]);
    }

    #[tokio::test]
    async fn unsynced_kept_as_fallback() {
        let r = runner(vec![("spotify", 5, "plain"), ("musixmatch", 5, "none"), ("lrclib", 5, "none")]);
        let (l, _) = run_chain(None, r, &order(), &meta()).await;
        let l = l.unwrap();
        assert_eq!(l.source, "spotify");
        assert_eq!(l.sync, SyncType::None);
    }

    #[tokio::test]
    async fn timeout_moves_on() {
        let r = runner(vec![("spotify", 6000, "synced"), ("musixmatch", 5, "synced"), ("lrclib", 5, "synced")]);
        let t = Instant::now();
        let (l, a) = run_chain(None, r, &order(), &meta()).await;
        assert_eq!(l.unwrap().source, "musixmatch");
        assert_eq!(a[0].outcome, "timeout");
        assert!(t.elapsed() < Duration::from_secs(5));
    }
}
