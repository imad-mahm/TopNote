import { useEffect, useRef, useState } from "react";
import { useWindowWidth } from "../lib/hooks";
import { fitControls, sideButtons, type BarFit } from "../lib/layout";
import { control, updateSettings, useApp, usePlayback } from "../store/app";
import { Heart, ImageIcon, LyricsIcon, More, Next, Pause, Play, Prev, QueueIcon, Repeat, Shuffle, Volume } from "./Icons";
import { ProgressBar } from "./ProgressBar";

const NONE: BarFit = { pair: false, like: false, queue: false, volume: false, view: false };

/**
 * Which buttons the control bar shows at the current window size.
 * The ⋯ menu offers whatever is left out, so nothing becomes unreachable.
 */
export function useBarFit(): BarFit {
  const w = useWindowWidth();
  const p = usePlayback();
  const overlay = useApp((s) => s.settings?.overlay ?? false);
  if (!p || !p.trackId || window.innerHeight < 120) return NONE; // no bar in the strip layout
  return fitControls(w, {
    like: p.kind === "track",
    volume: p.volume != null && p.supportsVolume,
    view: !overlay,
  });
}

/** Debounced volume with mute memory, shared by the bar and the menu. */
export function useVolume() {
  const p = usePlayback();
  const [draft, setDraft] = useState<number | null>(null);
  const timer = useRef<number>();
  const beforeMute = useRef(50);
  useEffect(() => () => clearTimeout(timer.current), []);
  const available = !!p && p.volume != null && p.supportsVolume;
  const v = draft ?? p?.volume ?? 0;
  const send = (n: number, now = false) => {
    setDraft(n);
    clearTimeout(timer.current);
    const go = () => control("volume", n).finally(() => setDraft(null));
    if (now) go();
    else timer.current = window.setTimeout(go, 160);
  };
  const toggleMute = () => {
    if (v > 0) {
      beforeMute.current = v;
      send(0, true);
    } else send(beforeMute.current || 50, true);
  };
  return { available, v, send, toggleMute };
}

export function VolumeSlider({ vol }: { vol: ReturnType<typeof useVolume> }) {
  return (
    <>
      <input
        type="range"
        min={0}
        max={100}
        step={1}
        value={vol.v}
        aria-label="Volume"
        onChange={(e) => vol.send(Number(e.target.value))}
        onPointerUp={(e) => vol.send(Number((e.target as HTMLInputElement).value), true)}
      />
      <span className="mono vol-num">{vol.v}</span>
    </>
  );
}

/** Speaker button that reveals a slider on hover; click toggles mute. */
function VolumeControl() {
  const vol = useVolume();
  if (!vol.available) return null;
  return (
    <div className="vol">
      <button className="ic sm" title={vol.v > 0 ? `Volume ${vol.v}% · click to mute` : "Unmute"} onClick={vol.toggleMute}>
        <Volume level={vol.v === 0 ? 0 : vol.v < 50 ? 1 : 2} width={16} height={16} />
      </button>
      <div className="vol-pop">
        <VolumeSlider vol={vol} />
      </div>
    </div>
  );
}

export function ControlBar({ onMenu }: { onMenu: (x: number, y: number) => void }) {
  const p = usePlayback();
  const view = useApp((s) => s.settings?.view ?? "artwork");
  const overlay = useApp((s) => s.settings?.overlay ?? false);
  const showQueue = useApp((s) => s.settings?.showQueue ?? false);
  const fit = useBarFit();
  const w = useWindowWidth();
  if (!p) return null;
  const { left, right } = sideButtons(fit);

  const side = (k: (typeof right)[number]) => {
    switch (k) {
      case "volume":
        return <VolumeControl key={k} />;
      case "like":
        return (
          <button key={k} className={`ic sm ${p.liked ? "on" : ""}`} title={p.liked ? "Remove from Liked" : "Save to Liked"} onClick={() => control(p.liked ? "unlike" : "like")}>
            <Heart filled={p.liked} width={16} height={16} />
          </button>
        );
      case "queue":
        return (
          <button key={k} className={`ic sm ${showQueue ? "on" : ""}`} title={showQueue ? "Hide queue" : "Show queue"} onClick={() => updateSettings({ showQueue: !showQueue })}>
            <QueueIcon width={16} height={16} />
          </button>
        );
      case "view":
        return (
          <button key={k} className="ic sm" title={view === "lyrics" ? "Show artwork" : "Show lyrics"} onClick={() => updateSettings({ view: view === "lyrics" ? "artwork" : "lyrics" })}>
            {view === "lyrics" ? <ImageIcon width={16} height={16} /> : <LyricsIcon width={16} height={16} />}
          </button>
        );
      case "more":
        return (
          <button
            key={k}
            className="ic sm"
            title="More"
            onClick={(e) => {
              const r = e.currentTarget.getBoundingClientRect();
              onMenu(r.right, r.top);
            }}
          >
            <More width={16} height={16} />
          </button>
        );
      default:
        return null;
    }
  };

  return (
    <div className="controlbar">
      <ProgressBar showTimes={w > 300 && !overlay} />
      <div className="bar-row">
        <div className="bar-side left">{left.map(side)}</div>
        <div className="controls">
          {fit.pair && (
            <button className={`ic ${p.shuffle ? "on" : ""}`} title="Shuffle" onClick={() => control("shuffle")}>
              <Shuffle />
            </button>
          )}
          <button className="ic" title="Previous" onClick={() => control("previous")}>
            <Prev />
          </button>
          <button className="ic play" title={p.isPlaying ? "Pause" : "Play"} onClick={() => control("toggle")}>
            {p.isPlaying ? <Pause /> : <Play />}
          </button>
          <button className="ic" title="Next" onClick={() => control("next")}>
            <Next />
          </button>
          {fit.pair && (
            <button className={`ic ${p.repeat !== "off" ? "on" : ""}`} title={`Repeat: ${p.repeat}`} onClick={() => control("repeat")}>
              <Repeat one={p.repeat === "track"} />
            </button>
          )}
        </div>
        <div className="bar-side right">{right.map(side)}</div>
      </div>
    </div>
  );
}
