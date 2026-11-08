import { useEffect, useRef, useState } from "react";
import { control, updateSettings, useApp, usePlayback } from "../store/app";
import { Heart, ImageIcon, LyricsIcon, More, Next, Pause, Play, Prev, QueueIcon, Repeat, Shuffle, Volume } from "./Icons";
import { ProgressBar } from "./ProgressBar";

/** Speaker button that reveals a slider on hover; click toggles mute. */
function VolumeControl() {
  const p = usePlayback();
  const [draft, setDraft] = useState<number | null>(null);
  const timer = useRef<number>();
  const beforeMute = useRef(50);
  useEffect(() => () => clearTimeout(timer.current), []);
  if (!p || p.volume == null || !p.supportsVolume) return null;
  const v = draft ?? p.volume;

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

  return (
    <div className="vol">
      <button className="ic sm" title={v > 0 ? `Volume ${v}% · click to mute` : "Unmute"} onClick={toggleMute}>
        <Volume level={v === 0 ? 0 : v < 50 ? 1 : 2} width={16} height={16} />
      </button>
      <div className="vol-pop">
        <input
          type="range"
          min={0}
          max={100}
          step={1}
          value={v}
          aria-label="Volume"
          onChange={(e) => send(Number(e.target.value))}
          onPointerUp={(e) => send(Number((e.target as HTMLInputElement).value), true)}
        />
        <span className="mono">{v}</span>
      </div>
    </div>
  );
}

export function ControlBar({ compact, onMenu }: { compact?: boolean; onMenu: (x: number, y: number) => void }) {
  const p = usePlayback();
  const view = useApp((s) => s.settings?.view ?? "artwork");
  const overlay = useApp((s) => s.settings?.overlay ?? false);
  const showQueue = useApp((s) => s.settings?.showQueue ?? false);
  if (!p) return null;
  const isTrack = p.kind === "track";

  return (
    <div className={`controlbar ${compact ? "is-compact" : ""}`}>
      <ProgressBar showTimes={!compact} />
      <div className="controls">
        {!compact && (
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
        {!compact && (
          <button
            className={`ic ${p.repeat !== "off" ? "on" : ""}`}
            title={`Repeat: ${p.repeat}`}
            onClick={() => control("repeat")}
          >
            <Repeat one={p.repeat === "track"} />
          </button>
        )}
      </div>
      <div className="controls-side">
        <VolumeControl />
        {isTrack && (
          <button className={`ic sm ${p.liked ? "on" : ""}`} title={p.liked ? "Remove from Liked" : "Save to Liked"} onClick={() => control(p.liked ? "unlike" : "like")}>
            <Heart filled={p.liked} width={16} height={16} />
          </button>
        )}
        <button className={`ic sm ${showQueue ? "on" : ""}`} title={showQueue ? "Hide queue" : "Show queue"} onClick={() => updateSettings({ showQueue: !showQueue })}>
          <QueueIcon width={16} height={16} />
        </button>
        {!overlay && (
          <button
            className="ic sm"
            title={view === "lyrics" ? "Show artwork" : "Show lyrics"}
            onClick={() => updateSettings({ view: view === "lyrics" ? "artwork" : "lyrics" })}
          >
            {view === "lyrics" ? <ImageIcon width={16} height={16} /> : <LyricsIcon width={16} height={16} />}
          </button>
        )}
        <button
          className="ic sm"
          title="More"
          onClick={(e) => {
            const r = e.currentTarget.getBoundingClientRect();
            onMenu(r.right, r.top);
          }}
        >
          <More width={16} height={16} />
        </button>
      </div>
    </div>
  );
}
