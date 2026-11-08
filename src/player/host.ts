// Hidden "player" window: hosts Spotify's Web Playback SDK so TopNote itself is a Spotify Connect
// device (Premium). Everything else (which device plays, verifying audio) is decided in Rust.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

type SdkError = { message: string };
type SpotifyPlayer = {
  addListener(ev: string, cb: (arg: any) => void): void;
  connect(): Promise<boolean>;
  activateElement?: () => Promise<void>;
  resume(): Promise<void>;
  pause(): Promise<void>;
  togglePlay(): Promise<void>;
  nextTrack(): Promise<void>;
  previousTrack(): Promise<void>;
  seek(ms: number): Promise<void>;
  setVolume(v: number): Promise<void>;
  getCurrentState(): Promise<SdkState | null>;
};
declare global {
  interface Window {
    onSpotifyWebPlaybackSDKReady?: () => void;
    Spotify?: { Player: new (opts: Record<string, unknown>) => SpotifyPlayer };
  }
}

const SDK_URL = "https://sdk.scdn.co/spotify-player.js";

type SdkImage = { url: string; width?: number | null };
type SdkTrack = {
  id: string | null;
  type?: string;
  name: string;
  duration_ms: number;
  artists: { name: string }[];
  album: { name: string; images: SdkImage[] };
};
type SdkState = {
  paused: boolean;
  loading?: boolean;
  position: number;
  duration: number;
  shuffle: boolean;
  repeat_mode: number;
  track_window: { current_track: SdkTrack | null; next_tracks: SdkTrack[] };
};

/** Largest image: the same URL the Web API puts first, so the main window's cache hits. */
const biggest = (imgs: SdkImage[] | undefined) =>
  imgs?.reduce<SdkImage | null>((a, b) => (!a || (b.width ?? 0) > (a.width ?? 0) ? b : a), null)?.url ?? null;

function summarize(s: SdkState, stalled = false) {
  const t = s.track_window.current_track;
  return {
    trackId: t?.id ?? null,
    kind: t?.type ?? "track",
    title: t?.name ?? "",
    artists: t?.artists.map((a) => a.name) ?? [],
    album: t?.album.name ?? null,
    artUrl: biggest(t?.album.images),
    durationMs: s.duration || t?.duration_ms || 0,
    positionMs: s.position,
    paused: s.paused,
    // Buffering: the SDK still says "playing" but the position isn't moving.
    stalled: stalled || !!s.loading,
    shuffle: s.shuffle,
    repeatMode: s.repeat_mode,
    nextArt: s.track_window.next_tracks.slice(0, 2).map((n) => biggest(n.album.images)).filter(Boolean),
  };
}

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
    // What TopNote was last told, to spot stalls and drift the SDK doesn't report itself.
    let told: { pos: number; at: number; moving: boolean } | null = null;
    const tell = (s: SdkState | null, stalled = false) => {
      if (s) {
        const sum = summarize(s, stalled);
        told = { pos: s.position, at: Date.now(), moving: !sum.paused && !sum.stalled };
        report("state", null, JSON.stringify(sum));
      } else {
        told = null;
        report("state", null, null);
      }
    };
    player.addListener("player_state_changed", (s: SdkState | null) => tell(s));
    // Once a second while playing: if the audio stalls (position stuck) or the clock drifted, say so
    // right away, so the main window freezes the bar instead of running ahead and jumping back.
    let lastPos = -1;
    setInterval(async () => {
      if (!told) return;
      const s = await player.getCurrentState().catch(() => null);
      if (!s || s.paused) {
        lastPos = -1;
        return;
      }
      const stuck = s.position === lastPos || !!s.loading;
      lastPos = s.position;
      const expected = told.moving ? told.pos + (Date.now() - told.at) : told.pos;
      if (stuck === told.moving || Math.abs(s.position - expected) > 1200) tell(s, stuck);
    }, 1000);
    try {
      await player.activateElement?.();
    } catch {
      /* autoplay is allowed by the browser args; activation is a no-op then */
    }
    // Controls from TopNote go straight to the player, no round trip through Spotify's servers.
    listen<{ action: string; value: number | null }>("player://command", ({ payload: { action, value } }) => {
      const run: Record<string, () => Promise<void>> = {
        play: () => player.resume(),
        pause: () => player.pause(),
        toggle: () => player.togglePlay(),
        next: () => player.nextTrack(),
        previous: () => player.previousTrack(),
        seek: () => player.seek(Math.max(0, value ?? 0)),
        volume: () => player.setVolume(Math.max(0, Math.min(100, value ?? 0)) / 100),
      };
      run[action]?.().catch((e) => report("playbackError", null, String(e)));
    });
    const ok = await player.connect().catch(() => false);
    if (!ok) report("connectFailed", null, "The player couldn't connect to Spotify");
  };
  const s = document.createElement("script");
  s.src = SDK_URL;
  s.async = true;
  s.onerror = () => report("loadFailed", null, "Couldn't load Spotify's player (offline?)");
  document.head.appendChild(s);
}
