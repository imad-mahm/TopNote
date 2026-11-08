import { useState } from "react";
import { api } from "../lib/ipc";
import { refreshAuth, toast, updateSettings, useApp } from "../store/app";
import { TitleBar } from "../components/TitleBar";

export function Onboarding() {
  const s = useApp((st) => st.settings)!;
  const auth = useApp((st) => st.auth);
  const [step, setStep] = useState(auth?.loggedIn ? 2 : s.clientId ? 1 : 0);
  const [clientId, setClientId] = useState(s.clientId);
  // first setup defaults to on; a returning user keeps what they chose
  const [inApp, setInApp] = useState(s.clientId ? s.inAppPlayback : true);
  const [spdc, setSpdc] = useState("");
  const [busy, setBusy] = useState(false);

  const run = async (fn: () => Promise<unknown>, next?: number) => {
    setBusy(true);
    try {
      await fn();
      await refreshAuth();
      if (next != null) setStep(next);
    } catch (e) {
      toast(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="shell onboarding">
      <div className="scrim" />
      <TitleBar />
      <div className="ob-body">
        <div className="ob-steps">{[0, 1, 2].map((i) => <i key={i} className={i <= step ? "on" : ""} />)}</div>
        {step === 0 && (
          <>
            <h2>Connect a Spotify app</h2>
            <ol>
              <li>Open <a onClick={() => window.open("https://developer.spotify.com/dashboard")}>developer.spotify.com/dashboard</a> and create an app.</li>
              <li>Add the redirect URI <code className="sel">{auth?.redirectUri ?? "http://127.0.0.1:8898/callback"}</code></li>
              <li>Tick <b>Web API</b>{inApp && <> and <b>Web Playback SDK</b></>}, save, and paste the Client ID below.</li>
            </ol>
            <input placeholder="Client ID" value={clientId} onChange={(e) => setClientId(e.target.value.trim())} />
            <label className="ob-check">
              <input type="checkbox" checked={inApp} onChange={(e) => setInApp(e.target.checked)} />
              <span>
                <b>Play music in TopNote</b>
                <small>
                  Lets Play work with Spotify closed. With Premium the music plays inside TopNote; otherwise TopNote
                  starts Spotify in the background.
                </small>
              </span>
            </label>
            <button
              className="pill primary"
              disabled={busy || clientId.length < 20}
              onClick={() => run(() => updateSettings({ clientId, inAppPlayback: inApp }), 1)}
            >
              Continue
            </button>
          </>
        )}
        {step === 1 && (
          <>
            <h2>Log in</h2>
            <p>Your browser will open to authorise TopNote.</p>
            <button className="pill primary" disabled={busy} onClick={() => run(() => api.login(), 2)}>
              {busy ? "Waiting for browser…" : "Log in with Spotify"}
            </button>
            <button className="link" onClick={() => setStep(0)}>Change Client ID</button>
          </>
        )}
        {step === 2 && (
          <>
            <h2>Spotify lyrics (optional)</h2>
            <p>
              For Spotify's own synced lyrics, paste the <code>sp_dc</code> cookie from open.spotify.com
              (DevTools → Application → Cookies). Skip to use Musixmatch / LRCLIB.
            </p>
            <input type="password" placeholder="sp_dc cookie" value={spdc} onChange={(e) => setSpdc(e.target.value.trim())} />
            <div className="row">
              <button className="pill" disabled={busy} onClick={() => run(() => updateSettings({ onboarded: true }))}>Skip</button>
              <button
                className="pill primary"
                disabled={busy || spdc.length < 20}
                onClick={() => run(async () => { await api.setSpDc(spdc); await updateSettings({ onboarded: true }); })}
              >
                Save & finish
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
