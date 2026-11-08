import { useEffect, useRef, useState } from "react";
import { api, type Candidate } from "../lib/ipc";
import { pickLyrics, useApp } from "../store/app";
import { Close, Search } from "./Icons";

const mmss = (ms: number) => {
  const s = Math.round(ms / 1000);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
};

/** Manual lyrics search (LRCLIB) for when automatic matching picks the wrong song. */
export function SearchPanel() {
  const p = useApp((s) => s.playback);
  const initial = p ? `${p.title} ${p.artists[0] ?? ""}`.trim() : "";
  const [q, setQ] = useState(initial);
  const [results, setResults] = useState<Candidate[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [open, setOpen] = useState<string | null>(null);
  const input = useRef<HTMLInputElement>(null);
  const req = useRef(0);
  const close = () => useApp.setState({ panel: null });

  const run = async (query: string) => {
    const r = ++req.current;
    setLoading(true);
    setError(null);
    try {
      const res = await api.searchLyrics(query);
      if (r === req.current) setResults(res);
    } catch (e) {
      if (r === req.current) setError(String(e));
    } finally {
      if (r === req.current) setLoading(false);
    }
  };

  useEffect(() => {
    input.current?.focus();
    input.current?.select();
    if (initial) run(initial);
    const esc = (e: KeyboardEvent) => e.key === "Escape" && close();
    window.addEventListener("keydown", esc);
    return () => window.removeEventListener("keydown", esc);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const dur = p?.durationMs ?? 0;
  return (
    <aside className="panel search" aria-label="Search lyrics">
      <header>
        <form
          className="search-box"
          onSubmit={(e) => {
            e.preventDefault();
            run(q);
          }}
        >
          <Search width={14} height={14} />
          <input ref={input} value={q} onChange={(e) => setQ(e.target.value)} placeholder="Song and artist" spellCheck={false} />
        </form>
        <button className="ic sm" title="Close" onClick={close}>
          <Close width={12} height={12} />
        </button>
      </header>
      <div className="panel-body">
        {loading && <div className="spinner" />}
        {error && <p className="panel-msg">{error}</p>}
        {!loading && results?.length === 0 && <p className="panel-msg">Nothing found. Try fewer words.</p>}
        <ol>
          {results?.map((c) => {
            const close = dur > 0 && Math.abs(c.durationMs - dur) <= 3000;
            return (
              <li key={c.provider + c.id} className={open === c.id ? "open" : ""}>
                <div className="cand" onClick={() => setOpen(open === c.id ? null : c.id)}>
                  <span className="q-text">
                    <span className="q-title">{c.title}</span>
                    <span className="q-artist">
                      {c.artist}
                      {c.album ? ` · ${c.album}` : ""}
                    </span>
                  </span>
                  <span className="tags">
                    {c.instrumental ? <em>instrumental</em> : c.synced ? <em className="good">synced</em> : <em>plain</em>}
                    <em className={close ? "good" : ""} title={close ? "Same length as this song" : ""}>
                      {mmss(c.durationMs)}
                    </em>
                  </span>
                </div>
                {open === c.id && (
                  <div className="cand-more">
                    <pre>{c.preview || "(no preview)"}</pre>
                    <button className="pill primary" onClick={() => pickLyrics(c.provider, c.id)}>
                      Use these lyrics
                    </button>
                  </div>
                )}
              </li>
            );
          })}
        </ol>
        <p className="panel-note">Results come from LRCLIB. Your pick is remembered for this song until you choose “Refetch lyrics”.</p>
      </div>
    </aside>
  );
}
