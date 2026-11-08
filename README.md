# Spotify Miniplayer

An always-on-top, freely resizable Spotify miniplayer for Windows, with synced lyrics. Built with Tauri v2 and React.

## Install
Run `src-tauri/target/release/bundle/nsis/Miniplayer_0.1.0_x64-setup.exe`, or build it yourself (see **Develop** below).

## First-run setup
1. Go to https://developer.spotify.com/dashboard and create an app.
2. Add this redirect URI: `http://127.0.0.1:8898/callback`
3. Tick **Web API** and save.
4. Paste the app's **Client ID** into the miniplayer and log in. This needs Spotify Premium for playback control.
5. Optional: paste your `sp_dc` cookie from open.spotify.com to get Spotify's own lyrics. Find it in DevTools → Application → Cookies.

Your secrets (refresh token, sp_dc) are stored in Windows Credential Manager. Settings and the lyrics cache are stored in `%APPDATA%\lb.imad.spotify-miniplayer`.

## Lyrics sources
The app tries these sources in order (you can change the order in Settings → Lyrics):
1. Spotify, which needs sp_dc
2. Musixmatch, word-synced where available
3. LRCLIB
4. Plain unsynced lyrics

Failing sources are paused automatically (circuit breaker). Settings → Diagnostics shows each source's health.

The Spotify lyrics token relies on a TOTP secret that Spotify rotates. The app fetches the current secret from the URL set in Settings → Lyrics. The secret bundled with the app as a fallback may be out of date; when it fails, the app falls back to Musixmatch.

## Controls
| | |
|---|---|
| Double-click | Switch between lyrics and artwork |
| Mouse wheel | Volume (over the lyrics, it scrolls instead) |
| Ctrl + wheel | Font size |
| Right-click | Menu (offset, opacity, click-through, …) |
| Click a lyric line | Seek to that line |
| Ctrl+Alt+M / L / V | Show or hide / click-through / switch view |
| Ctrl+Alt+↑ / ↓ | Lyric offset ±50 ms |

## Develop
```
npm install
npm run tauri dev      # development run
npm test               # frontend tests
cd src-tauri && cargo test
npm run tauri build    # NSIS installer
```

## Disclaimer
The Spotify lyrics and Musixmatch endpoints are unofficial and are used for personal use only. This app is not affiliated with Spotify or Musixmatch.
