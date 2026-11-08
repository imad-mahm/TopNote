// Hidden "player" window: hosts Spotify's Web Playback SDK so TopNote itself is a Spotify Connect
// device (Premium). Everything else (which device plays, verifying audio) is decided in Rust.
import { invoke } from "@tauri-apps/api/core";

type SdkError = { message: string };
type SpotifyPlayer = {
  addListener(ev: string, cb: (arg: any) => void): void;
  connect(): Promise<boolean>;
  activateElement?: () => Promise<void>;
};
declare global {
  interface Window {
    onSpotifyWebPlaybackSDKReady?: () => void;
    Spotify?: { Player: new (opts: Record<string, unknown>) => SpotifyPlayer };
  }
}

const SDK_URL = "https://sdk.scdn.co/spotify-player.js";

function report(kind: string, deviceId?: string | null, message?: string | null) {
  invoke("player_event", { kind, deviceId: deviceId ?? null, message: message ?? null }).catch((e) =>
    console.warn("player_event failed", e),
  );
}

export function startPlayerHost() {
  window.onSpotifyWebPlaybackSDKReady = async () => {
    const player = new window.Spotify!.Player({
      name: "TopNote",
      getOAuthToken: (cb: (t: string) => void) => {
        invoke<string>("player_token")
          .then(cb)
          .catch((e) => report("authenticationError", null, String(e)));
      },
      volume: 0.8,
    });
    player.addListener("ready", ({ device_id }: { device_id: string }) => report("ready", device_id));
    player.addListener("not_ready", ({ device_id }: { device_id: string }) => report("notReady", device_id));
    player.addListener("initialization_error", (e: SdkError) => report("initializationError", null, e.message));
    player.addListener("authentication_error", (e: SdkError) => report("authenticationError", null, e.message));
    player.addListener("account_error", (e: SdkError) => report("accountError", null, e.message));
    player.addListener("playback_error", (e: SdkError) => report("playbackError", null, e.message));
    try {
      await player.activateElement?.();
    } catch {
      /* autoplay is allowed by the browser args; activation is a no-op then */
    }
    const ok = await player.connect().catch(() => false);
    if (!ok) report("connectFailed", null, "The player couldn't connect to Spotify");
  };
  const s = document.createElement("script");
  s.src = SDK_URL;
  s.async = true;
  s.onerror = () => report("loadFailed", null, "Couldn't load Spotify's player (offline?)");
  document.head.appendChild(s);
}
