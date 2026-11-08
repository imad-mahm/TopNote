import { create } from "zustand";
import { api, metaOf, on, type AuthStatus, type LaunchStatus, type LyricsResult, type Playback, type Settings, type Translation } from "../lib/ipc";
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
  /** translation/romanization for the current lyrics, keyed by trackId|source */
  translation: { key: string; data: Translation | null; loading: boolean } | null;
  /** in-window panels */
  panel: "search" | "queue" | null;
  /** a player is being started for "Play" with nothing playing anywhere */
  launch: LaunchStatus | null;
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
  translation: null,
  panel: null,
  launch: null,
}));

/** Global offset + this song's remembered offset. */
export function effectiveOffset(st: { settings: Settings | null; playback: Playback | null } = useApp.getState()): number {
  const s = st.settings;
  if (!s) return 0;
  const id = st.playback?.trackId;
  return s.offsetMs + (id ? s.trackOffsets?.[id] ?? 0 : 0);
}

export function useEffectiveOffset(): number {
  const s = useApp((st) => st.settings);
  const id = useApp((st) => st.playback?.trackId ?? null);
  return (s?.offsetMs ?? 0) + (id && s ? s.trackOffsets?.[id] ?? 0 : 0);
}

/** Change the offset for the current song (or globally when nothing is playing). */
export function nudgeTrackOffset(delta: number | null) {
  const { settings: s, playback: p } = get();
  if (!s) return;
  const id = p?.trackId;
  if (!id) {
    if (delta != null) updateSettings({ offsetMs: clampOffset(s.offsetMs + delta) });
    return;
  }
  const v = delta == null ? 0 : clampOffset((s.trackOffsets?.[id] ?? 0) + delta);
  updateSettings({ trackOffsets: { [id]: v } } as Partial<Settings>);
}
const clampOffset = (v: number) => Math.max(-5000, Math.min(5000, v));

let translateReq = 0;
/** Load translation/romanization for the current lyrics when the mode asks for it. */
export async function loadTranslation() {
  const { settings: s, lyrics } = get();
  if (!s || s.translationMode === "off" || lyrics.state !== "ready" || !lyrics.result.lyrics) {
    if (get().translation) set({ translation: null });
    return;
  }
  const l = lyrics.result.lyrics;
  if (l.instrumental || !l.lines.length) return set({ translation: null });
  const key = `${lyrics.trackId}|${l.source}|${l.lines.length}`;
  const cur = get().translation;
  if (cur && cur.key === `${key}|${s.translateTo}`) return;
  const req = ++translateReq;
  set({ translation: { key: `${key}|${s.translateTo}`, data: null, loading: true } });
  try {
    const data = await api.translateLyrics(key, l.lines.map((x) => x.text), s.translateTo);
    if (req === translateReq) set({ translation: { key: `${key}|${s.translateTo}`, data, loading: false } });
  } catch (e) {
    if (req === translateReq) {
      set({ translation: { key: `${key}|${s.translateTo}`, data: null, loading: false } });
      toast("Translation unavailable");
      console.warn(e);
    }
  }
}

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
    loadTranslation();
  } catch (e) {
    if (req !== lyricsReq) return;
    set({ lyrics: { state: "error", trackId: meta.id, error: String(e) } });
  }
}

/** Use a manually picked lyrics search result for the current song. */
export async function pickLyrics(provider: string, id: string) {
  const p = get().playback;
  const meta = p && metaOf(p);
  if (!meta) return;
  const req = ++lyricsReq;
  set({ lyrics: { state: "loading", trackId: meta.id } });
  try {
    const result = await api.pickLyrics(meta, provider, id);
    if (req !== lyricsReq) return;
    set({ lyrics: { state: "ready", trackId: meta.id, result }, panel: null });
    toast("Lyrics saved for this song");
    loadTranslation();
  } catch (e) {
    if (req !== lyricsReq) return;
    toast(String(e));
    loadLyrics(false);
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
    toast(
      m.includes("no active device")
        ? "No active Spotify device"
        : m.includes("needs Spotify Premium")
          ? "That needs Spotify Premium"
          : m.includes("403")
            ? "Spotify refused that action"
            : m,
    );
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
  await on<Settings>("settings://changed", (s) => {
    const prev = get().settings;
    set({ settings: s });
    if (prev?.translationMode !== s.translationMode || prev?.translateTo !== s.translateTo) loadTranslation();
  });
  await on<boolean>("auth://changed", () => refreshAuth());
  await on<LaunchStatus>("launch://status", (l) => {
    if (l.phase === "working") return set({ launch: l });
    set({ launch: null });
    if (l.message) toast(l.message); // error, or why playback went to Spotify instead
  });
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
