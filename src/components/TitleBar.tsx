import { getCurrentWindow } from "@tauri-apps/api/window";
import { api } from "../lib/ipc";
import { updateSettings, useApp } from "../store/app";
import { Close, Minus, Pin } from "./Icons";

export function TitleBar() {
  const pinned = useApp((s) => s.settings?.alwaysOnTop ?? true);
  return (
    <div className="titlebar" data-tauri-drag-region>
      <button
        className={`tb-btn pin-indicator ${pinned ? "" : "unpinned"}`}
        title={pinned ? "Pinned on top · click to unpin" : "Not on top · click to pin"}
        aria-pressed={pinned}
        onClick={() => updateSettings({ alwaysOnTop: !pinned })}
      >
        <Pin width={12} height={12} off={!pinned} />
      </button>
      <div className="titlebar-buttons">
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
