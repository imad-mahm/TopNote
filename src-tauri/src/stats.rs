//! Local listening history. It never leaves the machine.
use crate::lyrics::cache;
use crate::playback::{smtc::now_ms, PlaybackState};
use crate::state::AppState;
use rusqlite::{params, Connection};
use serde::Serialize;

/// Plays shorter than this aren't recorded (skips).
const MIN_LISTEN_MS: i64 = 10_000;
/// Largest gap between two polls that still counts as listening (sleep, slow hidden polling).
const MAX_STEP_MS: i64 = 20_000;

#[derive(Debug, Clone, Default)]
pub struct Session {
    pub track_id: String,
    pub title: String,
    pub artist: String,
    pub art_url: Option<String>,
    pub duration_ms: i64,
    pub started_at: i64,
    pub listened_ms: i64,
    pub last_at: i64,
    pub playing: bool,
}

fn flush(c: &Connection, s: &Session) {
    if s.listened_ms < MIN_LISTEN_MS || s.track_id.is_empty() {
        return;
    }
    let _ = c.execute(
        "INSERT INTO plays (track_id, title, artist, art_url, started_at, listened_ms, duration_ms) VALUES (?1,?2,?3,?4,?5,?6,?7)",
        params![s.track_id, s.title, s.artist, s.art_url, s.started_at, s.listened_ms, s.duration_ms],
    );
}

/// Feed every playback observation; accumulates wall-clock listening time per track.
pub fn observe(st: &AppState, p: &PlaybackState) {
    if !st.settings().record_stats {
        return;
    }
    let now = now_ms();
    let mut cur = st.listen.lock().unwrap();
    let id = p.track_id.clone().filter(|_| p.kind == "track");
    if let (Some(s), Some(i)) = (cur.as_mut(), id.as_ref()) {
        if &s.track_id == i {
            if s.playing {
                s.listened_ms += (now - s.last_at).clamp(0, MAX_STEP_MS);
            }
            s.last_at = now;
            s.playing = p.is_playing;
            return;
        }
    }
    if let Some(prev) = cur.take() {
        flush(&st.lyrics.cache.lock().unwrap(), &prev);
    }
    *cur = id.map(|track_id| Session {
        track_id,
        title: p.title.clone(),
        artist: p.artists.first().cloned().unwrap_or_default(),
        art_url: p.art_url.clone(),
        duration_ms: p.duration_ms,
        started_at: now,
        listened_ms: 0,
        last_at: now,
        playing: p.is_playing,
    });
}

/// Save the running session (on quit).
pub fn flush_current(st: &AppState) {
    let s = st.listen.lock().unwrap().take();
    if let Some(s) = s {
        flush(&st.lyrics.cache.lock().unwrap(), &s);
    }
}

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TopTrack {
    pub track_id: String,
    pub title: String,
    pub artist: String,
    pub art_url: Option<String>,
    pub plays: i64,
    pub listened_ms: i64,
}

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TopArtist {
    pub artist: String,
    pub plays: i64,
    pub listened_ms: i64,
}

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ListeningStats {
    pub total_ms: i64,
    pub plays: i64,
    pub tracks: i64,
    pub since: Option<i64>,
    pub top_tracks: Vec<TopTrack>,
    pub top_artists: Vec<TopArtist>,
    /// Played tracks that have no lyrics anywhere.
    pub no_lyrics: Vec<TopTrack>,
    pub with_lyrics_pct: Option<f64>,
}

fn top_tracks(c: &Connection, limit: i64) -> Vec<TopTrack> {
    let Ok(mut q) = c.prepare(
        "SELECT track_id, MAX(title), MAX(artist), MAX(art_url), COUNT(*), SUM(listened_ms) FROM plays
         GROUP BY track_id ORDER BY COUNT(*) DESC, SUM(listened_ms) DESC LIMIT ?1",
    ) else {
        return vec![];
    };
    let rows = q.query_map(params![limit], |r| {
        Ok(TopTrack {
            track_id: r.get(0)?,
            title: r.get(1)?,
            artist: r.get(2)?,
            art_url: r.get(3)?,
            plays: r.get(4)?,
            listened_ms: r.get(5)?,
        })
    });
    match rows {
        Ok(it) => it.flatten().collect(),
        Err(_) => vec![],
    }
}

fn top_artists(c: &Connection) -> Vec<TopArtist> {
    let Ok(mut q) = c.prepare(
        "SELECT artist, COUNT(*), SUM(listened_ms) FROM plays WHERE artist <> '' GROUP BY artist ORDER BY SUM(listened_ms) DESC LIMIT 5",
    ) else {
        return vec![];
    };
    let rows = q.query_map([], |r| Ok(TopArtist { artist: r.get(0)?, plays: r.get(1)?, listened_ms: r.get(2)? }));
    match rows {
        Ok(it) => it.flatten().collect(),
        Err(_) => vec![],
    }
}

pub fn compute(c: &Connection) -> ListeningStats {
    let (total_ms, plays, tracks, since): (i64, i64, i64, Option<i64>) = c
        .query_row(
            "SELECT COALESCE(SUM(listened_ms),0), COUNT(*), COUNT(DISTINCT track_id), MIN(started_at) FROM plays",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap_or((0, 0, 0, None));
    let now = now_ms();
    let mut known = 0;
    let mut with = 0;
    let mut no_lyrics = Vec::new();
    for t in top_tracks(c, 500) {
        match cache::found(c, &t.track_id, now) {
            Some(true) => {
                known += 1;
                with += 1;
            }
            Some(false) => {
                known += 1;
                if no_lyrics.len() < 8 {
                    no_lyrics.push(t);
                }
            }
            None => {}
        }
    }
    ListeningStats {
        total_ms,
        plays,
        tracks,
        since,
        top_tracks: top_tracks(c, 5),
        top_artists: top_artists(c),
        no_lyrics,
        with_lyrics_pct: (known > 0).then(|| with as f64 * 100.0 / known as f64),
    }
}

pub fn reset(c: &Connection) {
    let _ = c.execute("DELETE FROM plays", []);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    #[test]
    fn aggregates() {
        let c = cache::open(Path::new(":memory:")).unwrap();
        let mk = |id: &str, artist: &str, ms: i64| Session {
            track_id: id.into(),
            title: format!("T{id}"),
            artist: artist.into(),
            listened_ms: ms,
            started_at: 5,
            ..Default::default()
        };
        flush(&c, &mk("a", "X", 60_000));
        flush(&c, &mk("a", "X", 30_000));
        flush(&c, &mk("b", "Y", 200_000));
        flush(&c, &mk("c", "Y", 2_000)); // too short, skipped
        let s = compute(&c);
        assert_eq!(s.plays, 3);
        assert_eq!(s.tracks, 2);
        assert_eq!(s.total_ms, 290_000);
        assert_eq!(s.top_tracks[0].track_id, "a");
        assert_eq!(s.top_artists[0].artist, "Y");
        assert_eq!(s.since, Some(5));
    }
}
