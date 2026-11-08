import { useCallback, useEffect, useRef, useState } from "react";
import { api, type QueueItem } from "../lib/ipc";
import { control, updateSettings, useApp } from "../store/app";
import { Close } from "./Icons";

const STATUS: Record<QueueItem["lyrics"], string> = {
  ready: "Lyrics ready",
  none: "No lyrics found",
  pending: "Lyrics not fetched yet",
};

const mmss = (ms: number) => {
  const s = Math.round(ms / 1000);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
};

/** "Up next" list: lyrics status per song, click to skip ahead. */
export function QueuePanel() {
  const trackId = useApp((s) => s.playback?.trackId ?? null);
  const [items, setItems] = useState<QueueItem[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<number | null>(null);
  const req = useRef(0);

  const reload = useCallback(async () => {
    const r = ++req.current;
    try {
      const q = await api.getQueue();
      if (r === req.current) {
        setItems(q);
        setError(null);
      }
    } catch (e) {
      if (r === req.current) setError(String(e).includes("no active device") ? "No active device" : "Couldn't load the queue");
    }
  }, []);

  // refresh on track change, shortly after (lyrics prefetch finishes), and every 15 s
  useEffect(() => {
    reload();
    const soon = window.setTimeout(reload, 4000);
    const t = window.setInterval(reload, 15000);
    return () => {
      clearTimeout(soon);
      clearInterval(t);
    };
  }, [trackId, reload]);

  const skip = async (i: number) => {
    setBusy(i);
    await control("skipTo", i + 1);
    setBusy(null);
    reload();
  };

  return (
    <aside className="panel queue" aria-label="Queue">
      <header>
        <span>Up next</span>
        <button className="ic sm" title="Close" onClick={() => updateSettings({ showQueue: false })}>
          <Close width={12} height={12} />
        </button>
      </header>
      <div className="panel-body">
        {error && <p className="panel-msg">{error}</p>}
        {!error && items === null && <div className="spinner" />}
        {!error && items?.length === 0 && <p className="panel-msg">The queue is empty</p>}
        <ol>
          {items?.map((q, i) => (
            <li key={`${i}-${q.id}`}>
              <button className={busy === i ? "busy" : ""} onClick={() => skip(i)} disabled={busy != null} title={`Skip to this (${i + 1} ahead)`}>
                {q.artUrl ? <img src={q.artUrl} alt="" /> : <span className="q-art" />}
                <span className="q-text">
                  <span className="q-title">{q.title}</span>
                  <span className="q-artist">{q.artists.join(", ")}</span>
                </span>
                <span className="q-dur mono">{mmss(q.durationMs)}</span>
                <i className={`q-dot ${q.lyrics}`} title={STATUS[q.lyrics]} />
              </button>
            </li>
          ))}
        </ol>
        {items && items.length > 0 && (
          <p className="panel-note">Spotify's API can't reorder the queue, so drag-to-reorder isn't available.</p>
        )}
      </div>
    </aside>
  );
}
