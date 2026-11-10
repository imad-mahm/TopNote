import { useEffect, useRef, useState } from "react";
import { ContextMenu } from "../components/ContextMenu";
import { ControlBar } from "../components/ControlBar";
import { Pause, Play } from "../components/Icons";
import { MiniProgress } from "../components/ProgressBar";
import { QueuePanel } from "../components/QueuePanel";
import { SearchPanel } from "../components/SearchPanel";
import { TitleBar } from "../components/TitleBar";
import { paletteFromImage, readable, type Palette } from "../lib/color";
import { useQueueDock, useSize } from "../lib/hooks";
import { api, type Settings } from "../lib/ipc";
import { layoutFor } from "../lib/layout";
import { control, toast, updateSettings, useApp, usePlayback } from "../store/app";
import { Artwork, ArtworkView, TrackInfo } from "./ArtworkView";
import { LyricsView } from "./LyricsView";

const FALLBACK_ACCENT = "#8ee6a8";
const CONTRAST_ACCENT = "#ffd400";

export const FONTS: Record<Settings["lyricFont"], string> = {
  inter: `"Inter Variable", "Segoe UI", system-ui, sans-serif`,
  segoe: `"Segoe UI Variable Display", "Segoe UI", system-ui, sans-serif`,
  serif: `Georgia, Cambria, "Times New Roman", serif`,
  mono: `"Cascadia Code", "Cascadia Mono", Consolas, ui-monospace, monospace`,
  rounded: `"Arial Rounded MT Bold", "Varela Round", Nunito, "Segoe UI", sans-serif`,
};

function usePalette(): Palette | null {
  const art = useApp((st) => st.playback?.artUrl ?? null);
  const [pal, setPal] = useState<Palette | null>(null);
  useEffect(() => {
    let alive = true;
    if (art) paletteFromImage(art).then((c) => alive && setPal(c));
    else setPal(null);
    return () => {
      alive = false;
    };
  }, [art]);
  return pal;
}

function accentFor(s: Settings, pal: Palette | null, spotifyBg: string | null): string {
  if (s.theme === "contrast") return CONTRAST_ACCENT;
  let a: string;
  if (s.accentMode === "fixed") a = s.fixedAccent;
  else if (s.accentMode === "spotify" && spotifyBg) a = readable(spotifyBg);
  else a = pal?.accent ?? FALLBACK_ACCENT;
  // pale accents vanish on the light theme; darken them there
  return s.theme === "light" ? `color-mix(in oklab, ${a} 55%, #000)` : a;
}

function StatusBanner() {
  const p = usePlayback();
  if (!p) return null;
  const map: Record<string, string | null> = {
    offline: "Offline · showing media info from Spotify desktop",
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
  const panel = useApp((s) => s.panel);
  const lyr = useApp((s) => s.lyrics);
  const launch = useApp((s) => s.launch);
  // layout follows the lyrics/artwork column, which shrinks when the queue is docked
  const [stageRef, size] = useSize<HTMLDivElement>();
  const layout = layoutFor(size.w, size.h);
  const queue = useQueueDock();
  const dockMode = queue.dock.mode;

  // growing the window while the popup is open turns it into the docked queue
  useEffect(() => {
    if (panel === "queue" && (dockMode === "side" || dockMode === "bottom")) {
      useApp.setState({ panel: null });
      if (!settings.dockQueue) updateSettings({ dockQueue: true });
    }
  }, [panel, dockMode, settings.dockQueue]);
  const pal = usePalette();
  const spotifyBg = lyr.state === "ready" ? lyr.result.lyrics?.colors?.background ?? null : null;
  const accent = accentFor(settings, pal, spotifyBg);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const volTimer = useRef<number>();
  const volValue = useRef<number | null>(null);
  const overlay = settings.overlay;
  const theme = settings.theme;

  const onWheel = (e: React.WheelEvent) => {
    if ((e.target as HTMLElement).closest(".lyrics-scroll, .panel")) return; // these scroll themselves (incl. the queue)
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
    "--radius": `${settings.radius}px`,
    "--lyric-font": FONTS[settings.lyricFont] ?? FONTS.inter,
    "--overlay-bg": `rgba(0,0,0,${settings.overlayBgOpacity})`,
    "--song-bg": pal?.bg ?? "#1c1c1c",
    "--song-bg2": pal?.bg2 ?? "#0c0c0c",
    opacity: settings.opacity,
  } as React.CSSProperties;

  const body = () => {
    if (!p) return null;
    if (p.status === "noDevice" && !p.trackId) {
      return (
        <div className="center-msg" aria-live="polite">
          {launch ? (
            <>
              <div className="spinner" />
              <p>{launch.message}</p>
            </>
          ) : (
            <>
              <p>Nothing is playing</p>
              <button className="pill" onClick={() => control("transfer")}>Play</button>
            </>
          )}
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
    if (overlay) return <LyricsView p={p} layout={layout} overlay />;
    if (layout === "strip") {
      return (
        <div className="strip-row">
          <Artwork url={p.artUrl} className="thumb" />
          {settings.view === "lyrics" ? <LyricsView p={p} layout={layout} /> : <TrackInfo p={p} />}
          <button className="ic play" onClick={() => control("toggle")}>
            {launch ? <span className="spinner sm" /> : p.isPlaying ? <Pause /> : <Play />}
          </button>
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

  const showBg = !overlay && p?.artUrl && ((theme === "glass" && settings.blurBackground) || theme === "songCover");
  const hasTrack = !!(p && p.trackId);

  return (
    <div
      className={[
        "shell",
        `layout-${layout}`,
        `view-${settings.view}`,
        `theme-${overlay ? "overlay" : theme}`,
        overlay ? "is-overlay" : "",
        p?.isPlaying ? "" : "is-paused",
        queue.docked ? `queue-${dockMode}` : "",
      ].join(" ")}
      style={{ ...style, "--q-size": `${queue.dock.size}px` } as React.CSSProperties}
      onWheel={onWheel}
      onDoubleClick={(e) => {
        if ((e.target as HTMLElement).closest("button, .progress, .lyric-line, .menu, .panel, input")) return;
        if (overlay) return;
        updateSettings({ view: settings.view === "lyrics" ? "artwork" : "lyrics" });
      }}
      onContextMenu={(e) => {
        if ((e.target as HTMLElement).closest("input")) return;
        e.preventDefault();
        setMenu({ x: e.clientX, y: e.clientY });
      }}
    >
      {showBg && <div className={theme === "songCover" ? "bg cover" : "bg"} style={{ backgroundImage: `url(${p!.artUrl})` }} />}
      <div className="scrim" />
      <TitleBar />
      <div className="dock">
        <div className="stage" ref={stageRef}>
          <StatusBanner />
          <main className="content">{body()}</main>
          {layout !== "strip" && hasTrack && (
            <ControlBar onMenu={(x, y) => setMenu({ x, y })} />
          )}
          {settings.miniProgress && hasTrack && p!.durationMs > 0 && <MiniProgress />}
        </div>
        {queue.docked && <QueuePanel mode={dockMode as "side" | "bottom"} />}
      </div>
      {queue.popup && <QueuePanel mode="popup" />}
      {panel === "search" && settings.manualSearch && layout !== "strip" && <SearchPanel />}
      {menu && <ContextMenu x={menu.x} y={menu.y} onClose={() => setMenu(null)} />}
      {toastMsg && <div className="toast">{toastMsg}</div>}
    </div>
  );
}
