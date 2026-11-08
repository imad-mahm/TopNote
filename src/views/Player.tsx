import { useEffect, useRef, useState } from "react";
import { ContextMenu } from "../components/ContextMenu";
import { ControlBar } from "../components/ControlBar";
import { Pause, Play } from "../components/Icons";
import { TitleBar } from "../components/TitleBar";
import { accentFromImage, readable } from "../lib/color";
import { useSize } from "../lib/hooks";
import { api } from "../lib/ipc";
import { layoutFor } from "../lib/layout";
import { control, toast, updateSettings, useApp, usePlayback } from "../store/app";
import { Artwork, ArtworkView, TrackInfo } from "./ArtworkView";
import { LyricsView } from "./LyricsView";

const FALLBACK_ACCENT = "#8ee6a8";

function useAccent() {
  const s = useApp((st) => st.settings);
  const art = useApp((st) => st.playback?.artUrl ?? null);
  const lyr = useApp((st) => st.lyrics);
  const [fromArt, setFromArt] = useState<string | null>(null);
  useEffect(() => {
    let alive = true;
    if (art) accentFromImage(art).then((c) => alive && setFromArt(c));
    else setFromArt(null);
    return () => {
      alive = false;
    };
  }, [art]);
  if (!s) return FALLBACK_ACCENT;
  if (s.accentMode === "fixed") return s.fixedAccent;
  if (s.accentMode === "spotify" && lyr.state === "ready" && lyr.result.lyrics?.colors) {
    return readable(lyr.result.lyrics.colors.background);
  }
  return fromArt ?? FALLBACK_ACCENT;
}

function StatusBanner() {
  const p = usePlayback();
  if (!p) return null;
  const map: Record<string, string | null> = {
    offline: "Offline · showing Windows media info",
    rateLimited: null, // silent back-off
    loggedOut: "Not logged in · open Settings",
  };
  const text = map[p.status];
  return text ? <div className="banner">{text}</div> : null;
}

export function Player() {
  const p = usePlayback();
  const settings = useApp((s) => s.settings)!;
  const toastMsg = useApp((s) => s.toast);
  const [rootRef, size] = useSize<HTMLDivElement>();
  const layout = layoutFor(size.w, size.h);
  const accent = useAccent();
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const volTimer = useRef<number>();
  const volValue = useRef<number | null>(null);

  const onWheel = (e: React.WheelEvent) => {
    if ((e.target as HTMLElement).closest(".lyrics-scroll")) return; // lyrics scroll themselves
    if (e.ctrlKey) {
      const fs = Math.max(0.6, Math.min(2, +(settings.fontScale + (e.deltaY < 0 ? 0.05 : -0.05)).toFixed(2)));
      updateSettings({ fontScale: fs });
      return;
    }
    if (!p || p.volume == null || !p.supportsVolume) return;
    const base = volValue.current ?? p.volume;
    const v = Math.max(0, Math.min(100, base + (e.deltaY < 0 ? 5 : -5)));
    volValue.current = v;
    toast(`Volume ${v}%`);
    clearTimeout(volTimer.current);
    volTimer.current = window.setTimeout(() => {
      control("volume", v);
      volValue.current = null;
    }, 250);
  };

  const style = {
    "--accent": accent,
    "--font-scale": settings.fontScale,
    opacity: settings.opacity,
  } as React.CSSProperties;

  const body = () => {
    if (!p) return null;
    if (p.status === "noDevice" && !p.trackId) {
      return (
        <div className="center-msg">
          <p>No active Spotify device</p>
          <button className="pill" onClick={() => control("transfer")}>Play on this PC</button>
        </div>
      );
    }
    if (p.status === "loggedOut" && !p.trackId) {
      return (
        <div className="center-msg">
          <p>Not logged in to Spotify</p>
          <button className="pill" onClick={() => api.openSettings()}>Open settings</button>
        </div>
      );
    }
    if (layout === "strip") {
      return (
        <div className="strip-row">
          <Artwork url={p.artUrl} className="thumb" />
          {settings.view === "lyrics" ? <LyricsView p={p} layout={layout} /> : <TrackInfo p={p} />}
          <button className="ic play" onClick={() => control("toggle")}>{p.isPlaying ? <Pause /> : <Play />}</button>
        </div>
      );
    }
    if (layout === "wide") {
      return (
        <div className="wide-row">
          <Artwork url={p.artUrl} className={`side ${p.isPlaying ? "" : "paused"}`} />
          <div className="wide-main">
            {settings.view === "lyrics" ? <LyricsView p={p} layout={layout} /> : <TrackInfo p={p} />}
          </div>
        </div>
      );
    }
    if (settings.view === "lyrics") {
      return (
        <div className="lyrics-layout">
          {layout === "tall" && (
            <div className="mini-head">
              <Artwork url={p.artUrl} className="thumb" />
              <TrackInfo p={p} />
            </div>
          )}
          <LyricsView p={p} layout={layout} />
        </div>
      );
    }
    return <ArtworkView p={p} layout={layout} />;
  };

  return (
    <div
      ref={rootRef}
      className={`shell layout-${layout} view-${settings.view} ${p?.isPlaying ? "" : "is-paused"}`}
      style={style}
      onWheel={onWheel}
      onDoubleClick={(e) => {
        if ((e.target as HTMLElement).closest("button, .progress, .lyric-line, .menu")) return;
        updateSettings({ view: settings.view === "lyrics" ? "artwork" : "lyrics" });
      }}
      onContextMenu={(e) => {
        e.preventDefault();
        setMenu({ x: e.clientX, y: e.clientY });
      }}
    >
      {settings.blurBackground && p?.artUrl && (
        <div className="bg" style={{ backgroundImage: `url(${p.artUrl})` }} />
      )}
      <div className="scrim" />
      <TitleBar />
      <StatusBanner />
      <main className="content">{body()}</main>
      {layout !== "strip" && p && p.trackId && <ControlBar compact={layout === "compact"} onMenu={(x, y) => setMenu({ x, y })} />}
      {menu && <ContextMenu x={menu.x} y={menu.y} onClose={() => setMenu(null)} />}
      {toastMsg && <div className="toast">{toastMsg}</div>}
    </div>
  );
}
