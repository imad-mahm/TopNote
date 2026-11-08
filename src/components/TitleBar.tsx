import { getCurrentWindow } from "@tauri-apps/api/window";
import { api } from "../lib/ipc";
import { Close, Minus, Pin } from "./Icons";

export function TitleBar() {
  return (
    <div className="titlebar" data-tauri-drag-region>
      <span className="pin-indicator" title="Always on top" data-tauri-drag-region>
        <Pin width={12} height={12} />
      </span>
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
