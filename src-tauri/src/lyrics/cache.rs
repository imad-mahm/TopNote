use super::model::LyricsResult;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

const HIT_TTL_MS: i64 = 30 * 24 * 3600 * 1000;
const MISS_TTL_MS: i64 = 24 * 3600 * 1000;

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let c = Connection::open(path)?;
    c.execute_batch(
        "PRAGMA journal_mode=WAL;
         CREATE TABLE IF NOT EXISTS lyrics (
           track_id TEXT PRIMARY KEY,
           isrc TEXT,
           source TEXT,
           found INTEGER NOT NULL,
           json TEXT NOT NULL,
           fetched_at INTEGER NOT NULL
         );
         CREATE INDEX IF NOT EXISTS lyrics_isrc ON lyrics(isrc);",
    )?;
    Ok(c)
}

pub fn get(c: &Connection, track_id: &str, now_ms: i64) -> Option<LyricsResult> {
    let row: Option<(i64, String, i64)> = c
        .query_row(
            "SELECT found, json, fetched_at FROM lyrics WHERE track_id = ?1",
            params![track_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .ok()?;
    let (found, json, at) = row?;
    let ttl = if found == 1 { HIT_TTL_MS } else { MISS_TTL_MS };
    if now_ms - at > ttl {
        return None;
    }
    let mut r: LyricsResult = serde_json::from_str(&json).ok()?;
    r.cached = true;
    Some(r)
}

pub fn put(c: &Connection, isrc: Option<&str>, r: &LyricsResult, now_ms: i64) -> rusqlite::Result<()> {
    let json = serde_json::to_string(r).unwrap_or_default();
    c.execute(
        "INSERT OR REPLACE INTO lyrics (track_id, isrc, source, found, json, fetched_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            r.track_id,
            isrc,
            r.lyrics.as_ref().map(|l| l.source.clone()),
            r.lyrics.is_some() as i64,
            json,
            now_ms
        ],
    )?;
    Ok(())
}

pub fn clear(c: &Connection) -> rusqlite::Result<()> {
    c.execute("DELETE FROM lyrics", [])?;
    let _ = c.execute_batch("VACUUM;");
    Ok(())
}

pub fn stats(c: &Connection) -> (i64, i64) {
    c.query_row("SELECT COUNT(*), COALESCE(SUM(LENGTH(json)),0) FROM lyrics", [], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap_or((0, 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lyrics::model::*;
    #[test]
    fn ttl() {
        let c = open(Path::new(":memory:")).unwrap();
        let hit = LyricsResult {
            track_id: "a".into(),
            lyrics: Some(Lyrics::plain("lrclib", "x")),
            attempts: vec![],
            cached: false,
        };
        let miss = LyricsResult { track_id: "b".into(), lyrics: None, attempts: vec![], cached: false };
        put(&c, None, &hit, 0).unwrap();
        put(&c, None, &miss, 0).unwrap();
        assert!(get(&c, "a", 10 * 24 * 3600 * 1000).unwrap().cached);
        assert!(get(&c, "b", 2 * 3600 * 1000).is_some());
        assert!(get(&c, "b", 2 * 24 * 3600 * 1000).is_none());
        assert!(get(&c, "a", 31 * 24 * 3600 * 1000).is_none());
    }
}
