import { create } from "zustand";
import { api, metaOf, on, type AuthStatus, type LyricsResult, type Playback, type Settings } from "../lib/ipc";
import { emptyAnchor, estimate, reconcile, type Anchor } from "../lib/clock";

type LyricsState =
  | { state: "idle" }
  | { state: "loading"; trackId: string }
  | { state: "ready"; trackId: string; result: LyricsResult }
  | { state: "error"; trackId: string; error: string };

type AppStore = {
  ready: boolean;
  settings: Settings | null;
  auth: AuthStatus | null;
  playback: Playback | null;
  anchor: Anchor;
  lyrics: LyricsState;
  clickThrough: boolean;
  /** optimistic overrides until next poll */
  pending: Partial<Playback>;
  toast: string | null;
};

export const useApp = create<AppStore>(() => ({
  ready: false,
  settings: null,
  auth: null,
  playback: null,
  anchor: emptyAnchor,
  lyrics: { state: "idle" },
  clickThrough: false,
  pending: {},
  toast: null,
}));

const set = useApp.setState;
const get = useApp.getState;

let toastTimer: number | undefined;
export function toast(msg: string) {
  set({ toast: msg });
  clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => set({ toast: null }), 2600);
}

function applyPlayback(p: Playback) {
  const prev = get().playback;
  set({ playback: p, pending: {}, anchor: reconcile(get().anchor, p, Date.now()) });
  if (p.trackId !== prev?.trackId) loadLyrics(false);
}

let lyricsReq = 0;
export async function loadLyrics(force: boolean, provider?: string) {
  const p = get().playback;
  const meta = p && metaOf(p);
  if (!meta) {
    set({ lyrics: { state: "idle" } });
    return;
  }
  const req = ++lyricsReq;
  set({ lyrics: { state: "loading", trackId: meta.id } });
  try {
    const result = await api.getLyrics(meta, force, provider);
    if (req !== lyricsReq) return;
    set({ lyrics: { state: "ready", trackId: meta.id, result } });
    if (provider && !result.lyrics) toast(`No lyrics from ${provider}`);
  } catch (e) {
    if (req !== lyricsReq) return;
    set({ lyrics: { state: "error", trackId: meta.id, error: String(e) } });
  }
}

export async function control(action: Parameters<typeof api.control>[0], value?: number) {
  const p = get().playback;
  // optimistic UI
  if (p) {
    if (action === "toggle") {
      const now = Date.now();
      const optimistic = { ...p, isPlaying: !p.isPlaying, progressMs: estimate(get().anchor, now), ts: now } as Playback;
      set({ pending: { isPlaying: !p.isPlaying }, anchor: reconcile({ ...get().anchor, at: 0 }, optimistic, now) });
    } else if (action === "seek" && value != null) {
      const now = Date.now();
      set({ anchor: reconcile({ ...get().anchor, at: 0 }, { ...p, progressMs: value, ts: now }, now) });
    } else if (action === "like" || action === "unlike") {
      set({ pending: { ...get().pending, liked: action === "like" } });
    } else if (action === "volume" && value != null) {
      set({ pending: { ...get().pending, volume: value } });
    } else if (action === "shuffle") {
      set({ pending: { ...get().pending, shuffle: !p.shuffle } });
    }
  }
  try {
    await api.control(action, value);
  } catch (e) {
    set({ pending: {} });
    const m = String(e);
    toast(m.includes("no active device") ? "No active Spotify device" : m.includes("403") ? "Spotify refused that action" : m);
  }
}

export async function updateSettings(patch: Partial<Settings>) {
  const s = await api.setSettings(patch);
  set({ settings: s });
}

export async function refreshAuth() {
  set({ auth: await api.authStatus() });
}

export async function boot() {
  const [settings, auth, playback] = await Promise.all([api.getSettings(), api.authStatus(), api.getPlayback()]);
  set({ settings, auth, ready: true });
  applyPlayback(playback);
  await on<Playback>("playback://state", applyPlayback);
  await on<Settings>("settings://changed", (s) => set({ settings: s }));
  await on<boolean>("auth://changed", () => refreshAuth());
  await on<boolean>("window://clickThrough", (v) => {
    set({ clickThrough: v });
    toast(v ? "Click-through on (Ctrl+Alt+L to exit)" : "Click-through off");
  });
  await on<null>("ui://toggleView", () => {
    const s = get().settings;
    if (s) updateSettings({ view: s.view === "lyrics" ? "artwork" : "lyrics" });
  });
}

/** Playback merged with optimistic overrides. */
export function usePlayback(): Playback | null {
  const p = useApp((s) => s.playback);
  const pending = useApp((s) => s.pending);
  return p ? { ...p, ...pending } : null;
}
