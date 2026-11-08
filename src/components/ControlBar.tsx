import { control, updateSettings, useApp, usePlayback } from "../store/app";
import { Heart, ImageIcon, LyricsIcon, More, Next, Pause, Play, Prev, Repeat, Shuffle } from "./Icons";
import { ProgressBar } from "./ProgressBar";

export function ControlBar({ compact, onMenu }: { compact?: boolean; onMenu: (x: number, y: number) => void }) {
  const p = usePlayback();
  const view = useApp((s) => s.settings?.view ?? "artwork");
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
        {isTrack && (
          <button className={`ic sm ${p.liked ? "on" : ""}`} title={p.liked ? "Remove from Liked" : "Save to Liked"} onClick={() => control(p.liked ? "unlike" : "like")}>
            <Heart filled={p.liked} width={16} height={16} />
          </button>
        )}
        <button
          className="ic sm"
          title={view === "lyrics" ? "Show artwork" : "Show lyrics"}
          onClick={() => updateSettings({ view: view === "lyrics" ? "artwork" : "lyrics" })}
        >
          {view === "lyrics" ? <ImageIcon width={16} height={16} /> : <LyricsIcon width={16} height={16} />}
        </button>
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
