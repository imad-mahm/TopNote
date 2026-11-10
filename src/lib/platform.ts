/** Which OS the webview runs on. Used to drop options that only exist on Windows and to name things right. */
const ua = typeof navigator === "undefined" ? "" : navigator.userAgent;

export const isWindows = /Windows/i.test(ua);
export const isLinux = /Linux/i.test(ua) && !/Android/i.test(ua);

/** Where secrets are kept. */
export const credentialStore = isWindows ? "Windows Credential Manager" : "your system keyring (Secret Service)";
/** TopNote's own Spotify player needs Widevine in the webview, which only WebView2 (Windows) has. */
export const canPlayInApp = isWindows;
