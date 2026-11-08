import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Playback = {
  status: "ok" | "idle" | "noDevice" | "loggedOut" | "offline" | "rateLimited";
  kind: "track" | "episode" | "ad" | "unknown";
  trackId: string | null;
  title: string;
  artists: string[];
  album: string;
  artUrl: string | null;
  durationMs: number;
  progressMs: number;
  isPlaying: boolean;
  shuffle: boolean;
  repeat: "off" | "context" | "track";
  volume: number | null;
  supportsVolume: boolean;
  liked: boolean;
  isrc: string | null;
  deviceName: string | null;
  description: string | null;
  source: "webapi" | "smtc";
  ts: number;
};

export type Word = { startMs: number; text: string };
export type Line = { startMs: number; endMs: number | null; text: string; words?: Word[] };
export type Lyrics = {
  source: "spotify" | "musixmatch" | "lrclib";
  sync: "word" | "line" | "none";
  lines: Line[];
  instrumental: boolean;
  colors: { background: string; text: string; highlight: string } | null;
  language: string | null;
};
export type Attempt = { provider: string; outcome: string; detail: string | null };
export type LyricsResult = { trackId: string; lyrics: Lyrics | null; attempts: Attempt[]; cached: boolean };

export type Shortcuts = {
  toggleWindow: string;
  clickThrough: string;
  toggleView: string;
  offsetUp: string;
  offsetDown: string;
};

export type Settings = {
  clientId: string;
  onboarded: boolean;
  providerOrder: string[];
  offsetMs: number;
  fontScale: number;
  wordSync: boolean;
  opacity: number;
  blurBackground: boolean;
  accentMode: "artwork" | "spotify" | "fixed";
  fixedAccent: string;
  skipTaskbar: boolean;
  autostart: boolean;
  snapEdges: boolean;
  view: "artwork" | "lyrics";
  spotifySecretsUrl: string;
  shortcuts: Shortcuts;
};

export type AuthStatus = { loggedIn: boolean; hasClientId: boolean; hasSpDc: boolean; redirectUri: string };
export type ProviderHealth = { name: string; ok: boolean; fails: number; coolingDownSecs: number | null; lastError: string | null };
export type CacheStats = { entries: number; bytes: number; path: string };

export type TrackMeta = {
  id: string;
  title: string;
  artists: string[];
  album: string;
  durationMs: number;
  isrc: string | null;
};

export type Action =
  | "play" | "pause" | "toggle" | "next" | "previous" | "seek" | "shuffle" | "repeat"
  | "volume" | "like" | "unlike" | "transfer";

export const api = {
  getSettings: () => invoke<Settings>("get_settings"),
  setSettings: (patch: Partial<Settings> | Record<string, unknown>) => invoke<Settings>("set_settings", { patch }),
  authStatus: () => invoke<AuthStatus>("auth_status"),
  login: () => invoke<AuthStatus>("login"),
  logout: () => invoke<AuthStatus>("logout"),
  setSpDc: (value: string | null) => invoke<AuthStatus>("set_sp_dc", { value }),
  getPlayback: () => invoke<Playback>("get_playback"),
  control: (action: Action, value?: number) => invoke<void>("control", { action, value }),
  getLyrics: (meta: TrackMeta, force = false, provider?: string) =>
    invoke<LyricsResult>("get_lyrics", { meta, force, provider }),
  providerHealth: () => invoke<ProviderHealth[]>("provider_health"),
  resetProviderHealth: () => invoke<void>("reset_provider_health"),
  cacheStats: () => invoke<CacheStats>("cache_stats"),
  clearCache: () => invoke<void>("clear_cache"),
  setClickThrough: (on: boolean) => invoke<void>("set_click_through", { on }),
  openSettings: () => invoke<void>("open_settings"),
  openConfigDir: () => invoke<void>("open_config_dir"),
  hideMain: () => invoke<void>("hide_main"),
  quit: () => invoke<void>("quit"),
};

export function on<T>(event: string, cb: (payload: T) => void): Promise<UnlistenFn> {
  return listen<T>(event, (e) => cb(e.payload));
}

export function metaOf(p: Playback): TrackMeta | null {
  if (!p.trackId || p.kind !== "track") return null;
  return { id: p.trackId, title: p.title, artists: p.artists, album: p.album, durationMs: p.durationMs, isrc: p.isrc };
}
