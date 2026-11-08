import { useEffect, useMemo, useRef, useState } from "react";
import { activeIndex } from "../lib/clock";
import { useFrame } from "../lib/hooks";
import type { Layout } from "../lib/layout";
import type { Line, Lyrics, Playback } from "../lib/ipc";
import { control, loadLyrics, updateSettings, useApp } from "../store/app";
import { Note } from "../components/Icons";

const PROVIDERS = ["spotify", "musixmatch", "lrclib"];
const LABEL: Record<string, string> = { spotify: "Spotify", musixmatch: "Musixmatch", lrclib: "LRCLIB" };

/** Extra lines (romanization / translation) per lyric line, as chosen in settings. */
type Sub = string[];
function useSubs(trackId: string, l: Lyrics): (i: number) => Sub {
  const mode = useApp((s) => s.settings?.translationMode ?? "off");
  const tr = useApp((s) => s.translation);
  return useMemo(() => {
    const data = tr?.data;
    if (mode === "off" || !data || !tr.key.startsWith(`${trackId}|${l.source}|${l.lines.length}|`)) return () => [];
    return (i: number) => {
      const out: string[] = [];
      const ro = data.romanized[i];
      const t = data.translated[i];
      if ((mode === "romanize" || mode === "both") && ro) out.push(ro);
      if ((mode === "translate" || mode === "both") && t) out.push(t);
      return out;
    };
  }, [mode, tr, trackId, l]);
}

function SubLines({ sub }: { sub: Sub }) {
  if (!sub.length) return null;
  return (
    <>
      {sub.map((t, i) => (
        <span key={i} className="lyric-sub">{t}</span>
      ))}
    </>
  );
}

function SourceBadge({ l }: { l: Lyrics }) {
  const loading = useApp((s) => s.translation?.loading ?? false);
  const picked = useApp((s) => s.lyrics.state === "ready" && s.lyrics.result.attempts.some((a) => a.outcome === "picked"));
  const next = () => {
    const i = PROVIDERS.indexOf(l.source);
    loadLyrics(true, PROVIDERS[(i + 1) % PROVIDERS.length]);
  };
  return (
    <button className="source-badge" onClick={next} title="Try another lyrics source">
      {LABEL[l.source] ?? l.source}
      {picked && <span className="unsynced"> · picked</span>}
      {l.sync === "none" && <span className="unsynced"> · unsynced</span>}
      {l.sync === "word" && <span className="unsynced"> · word</span>}
      {loading && <span className="unsynced"> · translating…</span>}
    </button>
  );
}

function Dots({ progress }: { progress?: number }) {
  return (
    <span className="dots" style={{ "--gp": progress ?? 0 } as React.CSSProperties}>
      <i /><i /><i />
    </span>
  );
}

export function LyricsView({ p, layout, overlay }: { p: Playback; layout: Layout; overlay?: boolean }) {
  const lyr = useApp((s) => s.lyrics);
  const manualSearch = useApp((s) => s.settings?.manualSearch ?? false);

  if (p.kind === "episode") return <Msg text="Lyrics aren't available for podcasts" />;
  if (p.kind === "ad") return <Msg text="Advertisement" />;
  if (!p.trackId) return <Msg text="Nothing playing" />;
  if (lyr.state === "loading" || lyr.state === "idle") return <div className="center-msg"><div className="spinner" /></div>;
  if (lyr.state === "error") return <Msg text="Couldn't load lyrics" action={["Retry", () => loadLyrics(true)]} />;
  const l = lyr.result.lyrics;
  if (!l) {
    if (overlay) return <Msg text={`${p.title} · ${p.artists[0] ?? ""}`} />;
    return (
      <Msg
        text="No lyrics found"
        action={
          manualSearch && layout !== "strip"
            ? ["Search lyrics", () => useApp.setState({ panel: "search" })]
            : ["Show artwork", () => updateSettings({ view: "artwork" })]
        }
      />
    );
  }
  if (l.instrumental) return <div className="center-msg"><Note width={28} height={28} /><p>Instrumental</p><SourceBadge l={l} /></div>;
  if (l.sync === "none") return <PlainLyrics trackId={lyr.trackId} l={l} overlay={overlay} />;
  return <SyncedLyrics key={lyr.trackId + l.source} trackId={lyr.trackId} l={l} mode={overlay ? "overlay" : layout === "strip" ? "strip" : "full"} />;
}

function Msg({ text, action }: { text: string; action?: [string, () => void] }) {
  return (
    <div className="center-msg">
      <p>{text}</p>
      {action && <button className="pill" onClick={action[1]}>{action[0]}</button>}
    </div>
  );
}

function PlainLyrics({ trackId, l, overlay }: { trackId: string; l: Lyrics; overlay?: boolean }) {
  const subs = useSubs(trackId, l);
  return (
    <div className={`lyrics plain ${overlay ? "in-overlay" : ""}`}>
      <div className="lyrics-scroll">
        {l.lines.map((ln, i) => (
          <p key={i} className="lyric-line plain-line">
            {ln.text || " "}
            <SubLines sub={subs(i)} />
          </p>
        ))}
      </div>
      <SourceBadge l={l} />
    </div>
  );
}

function SyncedLyrics({ trackId, l, mode }: { trackId: string; l: Lyrics; mode: "full" | "strip" | "overlay" }) {
  const viewport = useRef<HTMLDivElement>(null);
  const lineEls = useRef<(HTMLDivElement | null)[]>([]);
  const wordEls = useRef<(HTMLSpanElement | null)[]>([]);
  const introDots = useRef<HTMLSpanElement>(null);
  const [idx, setIdx] = useState(-1);
  const [follow, setFollow] = useState(true);
  const userUntil = useRef(0);
  const wordSync = useApp((s) => s.settings?.wordSync ?? true);
  const subs = useSubs(trackId, l);
  const lines = l.lines;
  const first = lines[0]?.startMs ?? 0;
  const compact = mode !== "full";

  useFrame((pos, now) => {
    const i = activeIndex(lines, pos);
    if (i !== idx) setIdx(i);
    if (!follow && now > userUntil.current) setFollow(true);
    // intro gap progress
    if (introDots.current && first > 0) introDots.current.style.setProperty("--gp", String(Math.min(1, pos / first)));
    // word wipe on the active line
    const ln = lines[i];
    if (ln?.words && wordSync) {
      ln.words.forEach((w, wi) => {
        const el = wordEls.current[wi];
        if (!el) return;
        const end = ln.words![wi + 1]?.startMs ?? ln.endMs ?? w.startMs + 400;
        const pr = Math.max(0, Math.min(1, (pos - w.startMs) / Math.max(1, end - w.startMs)));
        el.style.setProperty("--wp", `${(pr * 100).toFixed(1)}%`);
      });
    }
  }, [lines, idx, follow, wordSync, first]);

  // Auto-follow: keep the active line around 40% height.
  useEffect(() => {
    if (!follow || compact) return;
    const vp = viewport.current;
    const el = idx >= 0 ? lineEls.current[idx] : null;
    if (!vp) return;
    const top = el ? el.offsetTop - vp.clientHeight * 0.4 + el.clientHeight / 2 : 0;
    vp.scrollTo({ top, behavior: "smooth" });
  }, [idx, follow, compact]);

  const onUserScroll = () => {
    userUntil.current = Date.now() + 4000;
    setFollow(false);
  };

  const showIntro = first > 3000;

  if (mode === "strip") {
    const cur = lines[idx];
    const nxt = lines[idx + 1];
    const sub = idx >= 0 ? subs(idx) : [];
    return (
      <div className="lyrics strip">
        <div className="strip-cur">{idx < 0 && showIntro ? <Dots /> : cur?.text || <Dots />}</div>
        <div className="strip-next">{sub[0] ?? nxt?.text ?? ""}</div>
      </div>
    );
  }

  if (mode === "overlay") {
    const cur = lines[idx];
    const nxt = lines[idx + 1];
    return (
      <div className="lyrics overlay-lyrics">
        <div key={idx} className="ov-cur">
          {idx < 0 ? <Dots /> : cur?.text.trim() ? cur.text : <Dots />}
          {idx >= 0 && <SubLines sub={subs(idx)} />}
        </div>
        <div className="ov-next">{nxt?.text ?? ""}</div>
      </div>
    );
  }

  return (
    <div className="lyrics synced">
      <div ref={viewport} className="lyrics-scroll" onWheel={onUserScroll} onTouchMove={onUserScroll}>
        <div className="lyrics-pad" />
        {showIntro && (
          <div className={`lyric-line ${idx < 0 ? "active" : "past"}`}>
            <span ref={introDots}><Dots /></span>
          </div>
        )}
        {lines.map((ln, i) => (
          <LyricRow
            key={i}
            ln={ln}
            sub={subs(i)}
            state={i === idx ? "active" : i < idx ? "past" : "future"}
            refFn={(el) => (lineEls.current[i] = el)}
            wordRefs={i === idx && wordSync ? wordEls : null}
          />
        ))}
        <div className="lyrics-pad" />
      </div>
      {!follow && (
        <button className="pill resume" onClick={() => { userUntil.current = 0; setFollow(true); }}>
          Resume
        </button>
      )}
      <SourceBadge l={l} />
    </div>
  );
}

function LyricRow({
  ln,
  sub,
  state,
  refFn,
  wordRefs,
}: {
  ln: Line;
  sub: Sub;
  state: "active" | "past" | "future";
  refFn: (el: HTMLDivElement | null) => void;
  wordRefs: React.MutableRefObject<(HTMLSpanElement | null)[]> | null;
}) {
  const content = useMemo(() => {
    if (!ln.text.trim()) return <Dots />;
    if (wordRefs && ln.words) {
      return ln.words.map((w, wi) => (
        <span key={wi} className="word" ref={(el) => (wordRefs.current[wi] = el)}>
          {w.text}
        </span>
      ));
    }
    return ln.text;
  }, [ln, wordRefs]);

  return (
    <div
      ref={refFn}
      className={`lyric-line ${state} ${wordRefs && ln.words ? "wordsync" : ""}`}
      onClick={() => control("seek", ln.startMs)}
      title="Jump here"
    >
      {content}
      <SubLines sub={sub} />
    </div>
  );
}
