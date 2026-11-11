import { canPlayInApp, credentialStore, isWindows } from "../lib/platform";
import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api, type CacheStats, type ListeningStats, type ProviderHealth, type Settings as S, type Theme } from "../lib/ipc";
import { refreshAuth, toast, updateSettings, useApp } from "../store/app";

const TABS = ["Account", "Lyrics", "Appearance", "Behaviour", "Advanced", "Diagnostics"] as const;
type Tab = (typeof TABS)[number];
const LABEL: Record<string, string> = { spotify: "Spotify", musixmatch: "Musixmatch", lrclib: "LRCLIB" };

const THEMES: [Theme, string][] = [
  ["glass", "Glass"],
  ["dark", "Dark"],
  ["light", "Light"],
  ["contrast", "High contrast"],
  ["songColor", "Song colour"],
  ["songCover", "Song cover"],
];

const FONTS: [S["lyricFont"], string][] = [
  ["inter", "Inter"],
  ["segoe", "Segoe UI"],
  ["serif", "Serif"],
  ["mono", "Monospace"],
  ["rounded", "Rounded"],
];

const LANGS: [string, string][] = [
  ["en", "English"], ["ar", "Arabic"], ["fr", "French"], ["es", "Spanish"], ["de", "German"], ["it", "Italian"],
  ["pt", "Portuguese"], ["tr", "Turkish"], ["ru", "Russian"], ["ja", "Japanese"], ["ko", "Korean"],
  ["zh-CN", "Chinese (Simplified)"], ["hi", "Hindi"], ["id", "Indonesian"], ["nl", "Dutch"], ["pl", "Polish"],
];

export function Settings() {
  const s = useApp((st) => st.settings);
  const [tab, setTab] = useState<Tab>("Account");
  if (!s) return null;
  return (
    <div className="settings">
      <header data-tauri-drag-region>
        <span data-tauri-drag-region>TopNote settings</span>
        <button className="ic sm" onClick={() => getCurrentWindow().close()}>✕</button>
      </header>
      <nav>{TABS.map((t) => <button key={t} className={t === tab ? "on" : ""} onClick={() => setTab(t)}>{t}</button>)}</nav>
      <section>
        {tab === "Account" && <Account s={s} />}
        {tab === "Lyrics" && <LyricsTab s={s} />}
        {tab === "Appearance" && <Appearance s={s} />}
        {tab === "Behaviour" && <Behaviour s={s} />}
        {tab === "Advanced" && <Advanced s={s} />}
        {tab === "Diagnostics" && <Diagnostics />}
      </section>
    </div>
  );
}

const safe = (fn: () => Promise<unknown>) => async () => {
  try { await fn(); await refreshAuth(); } catch (e) { toast(String(e)); }
};

function Field({ label, children, hint }: { label: string; children: React.ReactNode; hint?: string }) {
  return (
    <label className="field">
      <span>{label}</span>
      <div>{children}</div>
      {hint && <small>{hint}</small>}
    </label>
  );
}

function Account({ s }: { s: S }) {
  const auth = useApp((st) => st.auth);
  const [cid, setCid] = useState(s.clientId);
  const [spdc, setSpdc] = useState("");
  return (
    <>
      <Field label="Client ID" hint={`Redirect URI: ${auth?.redirectUri ?? ""}`}>
        <input value={cid} onChange={(e) => setCid(e.target.value.trim())} />
        <button className="pill" onClick={safe(() => updateSettings({ clientId: cid }))}>Save</button>
      </Field>
      <Field label="Spotify account">
        <span className={auth?.loggedIn ? "ok" : "bad"}>{auth?.loggedIn ? "Logged in" : "Not logged in"}</span>
        {auth?.loggedIn
          ? <button className="pill" onClick={safe(api.logout)}>Log out</button>
          : <button className="pill primary" onClick={safe(api.login)}>Log in</button>}
      </Field>
      <Playback s={s} />
      <Field label="sp_dc cookie" hint={`Enables Spotify's own lyrics. Stored in ${credentialStore}.`}>
        <span className={auth?.hasSpDc ? "ok" : "bad"}>{auth?.hasSpDc ? "Saved" : "Not set"}</span>
        <input type="password" placeholder="paste new value" value={spdc} onChange={(e) => setSpdc(e.target.value.trim())} />
        <button className="pill" disabled={spdc.length < 20} onClick={safe(async () => { await api.setSpDc(spdc); setSpdc(""); })}>Save</button>
        {auth?.hasSpDc && <button className="pill" onClick={safe(() => api.setSpDc(null))}>Remove</button>}
      </Field>
      <ListeningInfo s={s} />
    </>
  );
}

function Playback({ s }: { s: S }) {
  const auth = useApp((st) => st.auth);
  const premium = useApp((st) => st.playback?.premium ?? null);
  const plan = premium == null ? "Plan unknown" : premium ? "Premium" : "Free";
  const needsLogin = s.inAppPlayback && auth?.loggedIn && auth.canStream === false && premium !== false;
  return (
    <Field
      label="Playback"
      hint={
        premium === false
          ? "Free account: controls go through the system media controls, and Play starts Spotify in the background when it's closed."
          : canPlayInApp
            ? "With Spotify closed, Play starts TopNote's own player (Premium), or Spotify in the background."
            : "With Spotify closed, Play starts Spotify in the background."
      }
    >
      <span className={premium === false ? "" : premium ? "ok" : ""}>{plan}</span>
      {canPlayInApp && (
        <label style={{ display: "flex", gap: 6, alignItems: "center" }}>
          <input type="checkbox" checked={s.inAppPlayback} onChange={(e) => updateSettings({ inAppPlayback: e.target.checked })} />
          Play music in TopNote
        </label>
      )}
      {needsLogin && (
        <button className="pill primary" title="Your login is from before this was turned on" onClick={safe(api.login)}>
          Log in again to enable
        </button>
      )}
      <button className="pill" title="Show the Spotify window TopNote started" onClick={safe(api.showSpotify)}>
        Show Spotify
      </button>
    </Field>
  );
}

function duration(ms: number): string {
  const m = Math.round(ms / 60000);
  if (m < 60) return `${m} min`;
  const h = Math.floor(m / 60);
  return h < 48 ? `${h} h ${m % 60} min` : `${Math.round(h / 24)} days`;
}

function ListeningInfo({ s }: { s: S }) {
  const [st, setSt] = useState<ListeningStats | null>(null);
  const reload = () => api.listeningStats().then(setSt).catch(() => setSt(null));
  useEffect(() => {
    reload();
    const t = setInterval(reload, 10000);
    return () => clearInterval(t);
  }, []);
  return (
    <div className="field">
      <span>Your listening</span>
      <div>
        <label style={{ display: "flex", gap: 6, alignItems: "center" }}>
          <input type="checkbox" checked={s.recordStats} onChange={(e) => updateSettings({ recordStats: e.target.checked })} />
          Keep local listening stats
        </label>
        <button
          className="pill"
          disabled={!st?.plays}
          onClick={async () => {
            if (!confirm("Delete all listening history recorded by TopNote?")) return;
            await api.resetStats();
            reload();
            toast("Listening stats cleared");
          }}
        >
          Reset
        </button>
      </div>
      <small>Recorded only on this PC while TopNote is running. Plays under 10 seconds are skipped.</small>
      {st && st.plays > 0 ? (
        <div className="stats">
          <div className="stat-row">
            <div className="stat"><b>{duration(st.totalMs)}</b><small>listened</small></div>
            <div className="stat"><b>{st.plays}</b><small>plays</small></div>
            <div className="stat"><b>{st.tracks}</b><small>different songs</small></div>
            {st.withLyricsPct != null && <div className="stat"><b>{Math.round(st.withLyricsPct)}%</b><small>had lyrics</small></div>}
          </div>
          {st.since && <small>Since {new Date(st.since).toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" })}</small>}
          <h4>Top songs</h4>
          <ol className="toplist">
            {st.topTracks.map((t, i) => (
              <li key={t.trackId}>
                <span className="rank">{i + 1}</span>
                {t.artUrl ? <img src={t.artUrl} alt="" /> : <span className="q-art" />}
                <span className="q-text"><span className="q-title">{t.title}</span><span className="q-artist">{t.artist}</span></span>
                <span className="meta">{t.plays}× · {duration(t.listenedMs)}</span>
              </li>
            ))}
          </ol>
          {st.topArtists.length > 0 && (
            <>
              <h4>Top artists</h4>
              <ol className="toplist">
                {st.topArtists.map((a, i) => (
                  <li key={a.artist} className="noart">
                    <span className="rank">{i + 1}</span>
                    <span className="q-title">{a.artist}</span>
                    <span className="meta">{a.plays} plays · {duration(a.listenedMs)}</span>
                  </li>
                ))}
              </ol>
            </>
          )}
          {st.noLyrics.length > 0 && (
            <>
              <h4>Played without lyrics</h4>
              <ol className="toplist">
                {st.noLyrics.map((t, i) => (
                  <li key={t.trackId}>
                    <span className="rank">{i + 1}</span>
                    {t.artUrl ? <img src={t.artUrl} alt="" /> : <span className="q-art" />}
                    <span className="q-text"><span className="q-title">{t.title}</span><span className="q-artist">{t.artist}</span></span>
                    <span className="meta">{t.plays}×</span>
                  </li>
                ))}
              </ol>
              {!s.manualSearch && <small>Tip: turn on manual lyrics search (Advanced) to fix these by hand.</small>}
            </>
          )}
        </div>
      ) : (
        <small>{s.recordStats ? "Nothing yet. Play a few songs." : "Recording is off."}</small>
      )}
    </div>
  );
}

function LyricsTab({ s }: { s: S }) {
  const move = (i: number, d: number) => {
    const o = [...s.providerOrder];
    const j = i + d;
    if (j < 0 || j >= o.length) return;
    [o[i], o[j]] = [o[j], o[i]];
    updateSettings({ providerOrder: o });
  };
  const perSong = Object.keys(s.trackOffsets ?? {}).length;
  return (
    <>
      <Field label="Provider order">
        <ol className="order">
          {s.providerOrder.map((p, i) => (
            <li key={p}>
              {LABEL[p] ?? p}
              <span>
                <button className="ic sm" onClick={() => move(i, -1)} disabled={i === 0}>▲</button>
                <button className="ic sm" onClick={() => move(i, 1)} disabled={i === s.providerOrder.length - 1}>▼</button>
              </span>
            </li>
          ))}
        </ol>
      </Field>
      <Field label={`Offset for all songs: ${s.offsetMs} ms`} hint="Positive = lyrics later. Per-song offsets (right-click → This song, or the offset shortcuts) add on top.">
        <input type="range" min={-2000} max={2000} step={50} value={s.offsetMs} onChange={(e) => updateSettings({ offsetMs: +e.target.value })} />
      </Field>
      <Field label={`Per-song offsets: ${perSong}`}>
        <button
          className="pill"
          disabled={!perSong}
          onClick={() => updateSettings({ trackOffsets: Object.fromEntries(Object.keys(s.trackOffsets).map((k) => [k, 0])) })}
        >
          Clear all
        </button>
      </Field>
      <Field label="Translation" hint="Adds a line under each lyric. Lyrics are sent to Google Translate; results are cached.">
        <select value={s.translationMode} onChange={(e) => updateSettings({ translationMode: e.target.value as S["translationMode"] })}>
          <option value="off">Off</option>
          <option value="translate">Translation</option>
          <option value="romanize">Romanization</option>
          <option value="both">Romanization + translation</option>
        </select>
        <select value={s.translateTo} onChange={(e) => updateSettings({ translateTo: e.target.value })} disabled={s.translationMode === "off" || s.translationMode === "romanize"}>
          {LANGS.map(([code, name]) => <option key={code} value={code}>{name}</option>)}
        </select>
      </Field>
      <Field label={`Font scale: ${s.fontScale.toFixed(2)}×`}>
        <input type="range" min={0.6} max={2} step={0.05} value={s.fontScale} onChange={(e) => updateSettings({ fontScale: +e.target.value })} />
      </Field>
      <Field label="Word-by-word highlight">
        <input type="checkbox" checked={s.wordSync} onChange={(e) => updateSettings({ wordSync: e.target.checked })} />
      </Field>
    </>
  );
}

function Appearance({ s }: { s: S }) {
  return (
    <>
      <div className="field">
        <span>Theme</span>
        <div className="themes">
          {THEMES.map(([id, name]) => (
            <button key={id} className={`theme-swatch ${s.theme === id ? "on" : ""}`} onClick={() => updateSettings({ theme: id })}>
              <i className={`sw-${id}`} />
              {name}
            </button>
          ))}
        </div>
        <small>Song colour paints the window with the artwork's colours. Song cover puts the full cover behind everything.</small>
      </div>
      <Field label={`Background opacity: ${Math.round(s.bgOpacity * 100)}%`} hint="At 0% the window is fully see-through; lyrics get a soft outline so they stay readable.">
        <input type="range" min={0} max={1} step={0.05} value={s.bgOpacity} onChange={(e) => updateSettings({ bgOpacity: +e.target.value })} />
      </Field>
      <Field label={`Text opacity: ${Math.round(s.textOpacity * 100)}%`} hint="Lyrics, titles and artwork. Controls always show fully when they appear.">
        <input type="range" min={0.2} max={1} step={0.05} value={s.textOpacity} onChange={(e) => updateSettings({ textOpacity: +e.target.value })} />
      </Field>
      <Field label={`Corner radius: ${s.radius}px`}>
        <input type="range" min={0} max={28} step={1} value={s.radius} onChange={(e) => updateSettings({ radius: +e.target.value })} />
      </Field>
      <Field label="Lyrics font">
        <select value={s.lyricFont} onChange={(e) => updateSettings({ lyricFont: e.target.value as S["lyricFont"] })}>
          {FONTS.map(([id, name]) => <option key={id} value={id}>{name}</option>)}
        </select>
      </Field>
      {s.theme === "glass" && (
        <Field label="Blurred artwork background">
          <input type="checkbox" checked={s.blurBackground} onChange={(e) => updateSettings({ blurBackground: e.target.checked })} />
        </Field>
      )}
      <Field label="Accent colour">
        <select value={s.accentMode} onChange={(e) => updateSettings({ accentMode: e.target.value as S["accentMode"] })} disabled={s.theme === "contrast"}>
          <option value="artwork">From artwork</option>
          <option value="spotify">Spotify lyric colours</option>
          <option value="fixed">Fixed</option>
        </select>
        {s.accentMode === "fixed" && (
          <input type="color" value={s.fixedAccent} onChange={(e) => updateSettings({ fixedAccent: e.target.value })} />
        )}
      </Field>
      <Field label="Lyrics overlay" hint="Lyrics only: no artwork or panel, just the current and next line floating over your screen.">
        <input type="checkbox" checked={s.overlay} onChange={(e) => updateSettings({ overlay: e.target.checked })} />
      </Field>
      <Field label={`Overlay background: ${Math.round(s.overlayBgOpacity * 100)}%`}>
        <input type="range" min={0} max={1} step={0.05} value={s.overlayBgOpacity} onChange={(e) => updateSettings({ overlayBgOpacity: +e.target.value })} />
      </Field>
      <Field label="Thin progress line when controls are hidden">
        <input type="checkbox" checked={s.miniProgress} onChange={(e) => updateSettings({ miniProgress: e.target.checked })} />
      </Field>
    </>
  );
}

function Behaviour({ s }: { s: S }) {
  const sc = s.shortcuts;
  const setSc = (k: keyof S["shortcuts"], v: string) => updateSettings({ shortcuts: { ...sc, [k]: v } });
  return (
    <>
      <Field label={isWindows ? "Start with Windows" : "Start at login"}>
        <input type="checkbox" checked={s.autostart} onChange={(e) => updateSettings({ autostart: e.target.checked })} />
      </Field>
      <Field label="Hide from taskbar">
        <input type="checkbox" checked={s.skipTaskbar} onChange={(e) => updateSettings({ skipTaskbar: e.target.checked })} />
      </Field>
      {isWindows && (
        <Field label="Taskbar preview buttons" hint="Previous / play-pause / next under the taskbar thumbnail. Needs “Hide from taskbar” off.">
          <input type="checkbox" checked={s.taskbarButtons} onChange={(e) => updateSettings({ taskbarButtons: e.target.checked })} />
        </Field>
      )}
      <Field label="Show the queue beside or under the lyrics" hint="Needs a window at least 600 px wide or 480 px tall. Smaller windows open the queue as a popup instead.">
        <input type="checkbox" checked={s.dockQueue} onChange={(e) => updateSettings({ dockQueue: e.target.checked })} />
      </Field>
      <Field label="Snap to screen edges">
        <input type="checkbox" checked={s.snapEdges} onChange={(e) => updateSettings({ snapEdges: e.target.checked })} />
      </Field>
      {(Object.keys(sc) as (keyof typeof sc)[]).map((k) => (
        <Field key={k} label={k.replace(/([A-Z])/g, " $1").toLowerCase()}>
          <input className="mono" defaultValue={sc[k]} onBlur={(e) => e.target.value !== sc[k] && setSc(k, e.target.value.trim())} />
        </Field>
      ))}
    </>
  );
}

function Advanced({ s }: { s: S }) {
  return (
    <>
      <Field
        label="Manual lyrics search"
        hint="Adds “Search lyrics…” to the right-click menu so you can pick the right lyrics when the automatic match is wrong. Picks are remembered per song."
      >
        <input type="checkbox" checked={s.manualSearch} onChange={(e) => updateSettings({ manualSearch: e.target.checked })} />
      </Field>
      <Field label="Spotify secrets URL" hint="Remote JSON with the current TOTP secret.">
        <input defaultValue={s.spotifySecretsUrl} onBlur={(e) => updateSettings({ spotifySecretsUrl: e.target.value.trim() })} />
      </Field>
    </>
  );
}

function Diagnostics() {
  const [health, setHealth] = useState<ProviderHealth[]>([]);
  const [cache, setCache] = useState<CacheStats | null>(null);
  const reload = async () => {
    setHealth(await api.providerHealth());
    setCache(await api.cacheStats());
  };
  useEffect(() => { reload(); const t = setInterval(reload, 3000); return () => clearInterval(t); }, []);
  return (
    <>
      <Field label="Providers">
        <table className="health">
          <tbody>
            {health.map((h) => (
              <tr key={h.name}>
                <td>{LABEL[h.name] ?? h.name}</td>
                <td className={h.ok ? "ok" : "bad"}>{h.ok ? "OK" : h.coolingDownSecs ? `paused ${Math.ceil(h.coolingDownSecs / 60)}m` : `${h.fails} fails`}</td>
                <td className="err">{h.lastError ?? ""}</td>
              </tr>
            ))}
          </tbody>
        </table>
        <button className="pill" onClick={async () => { await api.resetProviderHealth(); reload(); }}>Reset</button>
      </Field>
      <Field label="Lyrics cache" hint={cache?.path}>
        <span>{cache ? `${cache.entries} entries · ${(cache.bytes / 1024).toFixed(0)} KB` : "…"}</span>
        <button className="pill" onClick={async () => { await api.clearCache(); reload(); toast("Cache cleared"); }}>Clear</button>
      </Field>
      <Field label="Config folder">
        <button className="pill" onClick={() => api.openConfigDir()}>Open</button>
      </Field>
    </>
  );
}
