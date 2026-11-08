import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api, type CacheStats, type ProviderHealth, type Settings as S } from "../lib/ipc";
import { refreshAuth, toast, updateSettings, useApp } from "../store/app";

const TABS = ["Account", "Lyrics", "Appearance", "Behaviour", "Diagnostics"] as const;
type Tab = (typeof TABS)[number];
const LABEL: Record<string, string> = { spotify: "Spotify", musixmatch: "Musixmatch", lrclib: "LRCLIB" };

export function Settings() {
  const s = useApp((st) => st.settings);
  const [tab, setTab] = useState<Tab>("Account");
  if (!s) return null;
  return (
    <div className="settings">
      <header data-tauri-drag-region>
        <span data-tauri-drag-region>Settings</span>
        <button className="ic sm" onClick={() => getCurrentWindow().close()}>✕</button>
      </header>
      <nav>{TABS.map((t) => <button key={t} className={t === tab ? "on" : ""} onClick={() => setTab(t)}>{t}</button>)}</nav>
      <section>
        {tab === "Account" && <Account s={s} />}
        {tab === "Lyrics" && <LyricsTab s={s} />}
        {tab === "Appearance" && <Appearance s={s} />}
        {tab === "Behaviour" && <Behaviour s={s} />}
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
      <Field label="sp_dc cookie" hint="Enables Spotify's own lyrics. Stored in Windows Credential Manager.">
        <span className={auth?.hasSpDc ? "ok" : "bad"}>{auth?.hasSpDc ? "Saved" : "Not set"}</span>
        <input type="password" placeholder="paste new value" value={spdc} onChange={(e) => setSpdc(e.target.value.trim())} />
        <button className="pill" disabled={spdc.length < 20} onClick={safe(async () => { await api.setSpDc(spdc); setSpdc(""); })}>Save</button>
        {auth?.hasSpDc && <button className="pill" onClick={safe(() => api.setSpDc(null))}>Remove</button>}
      </Field>
    </>
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
      <Field label={`Offset: ${s.offsetMs} ms`} hint="Positive = lyrics later.">
        <input type="range" min={-2000} max={2000} step={50} value={s.offsetMs} onChange={(e) => updateSettings({ offsetMs: +e.target.value })} />
      </Field>
      <Field label={`Font scale: ${s.fontScale.toFixed(2)}×`}>
        <input type="range" min={0.6} max={2} step={0.05} value={s.fontScale} onChange={(e) => updateSettings({ fontScale: +e.target.value })} />
      </Field>
      <Field label="Word-by-word highlight">
        <input type="checkbox" checked={s.wordSync} onChange={(e) => updateSettings({ wordSync: e.target.checked })} />
      </Field>
      <Field label="Spotify secrets URL" hint="Remote JSON with the current TOTP secret.">
        <input defaultValue={s.spotifySecretsUrl} onBlur={(e) => updateSettings({ spotifySecretsUrl: e.target.value.trim() })} />
      </Field>
    </>
  );
}

function Appearance({ s }: { s: S }) {
  return (
    <>
      <Field label={`Opacity: ${Math.round(s.opacity * 100)}%`}>
        <input type="range" min={0.3} max={1} step={0.05} value={s.opacity} onChange={(e) => updateSettings({ opacity: +e.target.value })} />
      </Field>
      <Field label="Blurred artwork background">
        <input type="checkbox" checked={s.blurBackground} onChange={(e) => updateSettings({ blurBackground: e.target.checked })} />
      </Field>
      <Field label="Accent colour">
        <select value={s.accentMode} onChange={(e) => updateSettings({ accentMode: e.target.value as S["accentMode"] })}>
          <option value="artwork">From artwork</option>
          <option value="spotify">Spotify lyric colours</option>
          <option value="fixed">Fixed</option>
        </select>
        {s.accentMode === "fixed" && (
          <input type="color" value={s.fixedAccent} onChange={(e) => updateSettings({ fixedAccent: e.target.value })} />
        )}
      </Field>
    </>
  );
}

function Behaviour({ s }: { s: S }) {
  const sc = s.shortcuts;
  const setSc = (k: keyof S["shortcuts"], v: string) => updateSettings({ shortcuts: { ...sc, [k]: v } });
  return (
    <>
      <Field label="Start with Windows">
        <input type="checkbox" checked={s.autostart} onChange={(e) => updateSettings({ autostart: e.target.checked })} />
      </Field>
      <Field label="Hide from taskbar">
        <input type="checkbox" checked={s.skipTaskbar} onChange={(e) => updateSettings({ skipTaskbar: e.target.checked })} />
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
