use super::model::Line;

/// Parse "mm:ss.xx", "mm:ss.xxx", "mm:ss" or "mm:ss:xx" into ms.
fn parse_tag(t: &str) -> Option<i64> {
    let (m, rest) = t.split_once(':')?;
    let m: i64 = m.trim().parse().ok()?;
    let (s, frac) = match rest.split_once(['.', ':']) {
        Some((s, f)) => (s, f),
        None => (rest, ""),
    };
    let s: i64 = s.trim().parse().ok()?;
    let frac_ms = if frac.is_empty() {
        0
    } else {
        let f: i64 = frac.trim().parse().ok()?;
        match frac.trim().len() {
            1 => f * 100,
            2 => f * 10,
            _ => f,
        }
    };
    Some(m * 60_000 + s * 1000 + frac_ms)
}

/// Parse LRC text into time-sorted lines. Multiple leading tags produce repeated lines.
pub fn parse_lrc(src: &str) -> Vec<Line> {
    let mut out = Vec::new();
    for raw in src.lines() {
        let mut rest = raw.trim();
        let mut times = Vec::new();
        while rest.starts_with('[') {
            let Some(close) = rest.find(']') else { break };
            match parse_tag(&rest[1..close]) {
                Some(t) => times.push(t),
                None => break, // metadata tag like [ar:...]
            }
            rest = rest[close + 1..].trim_start();
        }
        for t in times {
            out.push(Line { start_ms: t, end_ms: None, text: rest.trim().to_string(), words: None });
        }
    }
    out.sort_by_key(|l| l.start_ms);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_basic_and_multi_tags() {
        let src = "[ar:Someone]\n[00:01.50]Hello\n[00:03.00][00:10.000]Chorus\n[00:05]\n";
        let l = parse_lrc(src);
        assert_eq!(l.len(), 4);
        assert_eq!(l[0].start_ms, 1500);
        assert_eq!(l[0].text, "Hello");
        assert_eq!(l[1].start_ms, 3000);
        assert_eq!(l[2].start_ms, 5000);
        assert_eq!(l[2].text, "");
        assert_eq!(l[3].start_ms, 10000);
        assert_eq!(l[3].text, "Chorus");
    }
    #[test]
    fn tag_formats() {
        assert_eq!(parse_tag("01:02.3"), Some(62_300));
        assert_eq!(parse_tag("01:02:30"), Some(62_300));
        assert_eq!(parse_tag("ar"), None);
    }
}
