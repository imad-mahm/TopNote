import { getCurrentWindow } from "@tauri-apps/api/window";
import { api } from "../lib/ipc";
import { setFullscreen, updateSettings, useApp } from "../store/app";
import { Close, Fullscreen, Minus, Pin } from "./Icons";

export function TitleBar() {
  const pinned = useApp((s) => s.settings?.alwaysOnTop ?? true);
  const fullscreen = useApp((s) => s.fullscreen);
  return (
    <div
      className="titlebar"
      // drag to move; double-click maximises / restores (handled by Tauri's drag-region script)
      data-tauri-drag-region={fullscreen ? undefined : true}
    >
      <button
        className={`tb-btn pin-indicator ${pinned ? "" : "unpinned"}`}
        title={pinned ? "Pinned on top · click to unpin" : "Not on top · click to pin"}
        aria-pressed={pinned}
        onClick={() => updateSettings({ alwaysOnTop: !pinned })}
      >
        <Pin width={12} height={12} off={!pinned} />
      </button>
      <div className="titlebar-buttons">
        <button className="tb-btn" title={fullscreen ? "Exit fullscreen (F11)" : "Fullscreen (F11)"} onClick={() => setFullscreen(!fullscreen)}>
          <Fullscreen width={12} height={12} exit={fullscreen} />
        </button>
        <button className="tb-btn" title="Minimise" onClick={() => getCurrentWindow().minimize()}>
          <Minus width={12} height={12} />
        </button>
        <button className="tb-btn close" title="Hide to tray" onClick={() => api.hideMain()}>
          <Close width={12} height={12} />
        </button>
      </div>
    </div>
  );
}
