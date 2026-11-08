import React from "react";
import ReactDOM from "react-dom/client";
import "@fontsource-variable/inter";
import "./styles/tokens.css";
import "./styles/app.css";
import { App } from "./App";
import { boot } from "./store/app";
import { startPlayerHost } from "./player/host";

if (location.hash === "#player") {
  // hidden audio-only window: no UI, no app store
  startPlayerHost();
} else {
  if (location.hash === "#settings") document.documentElement.classList.add("is-settings");
  boot().catch((e) => console.error("boot failed", e));

  ReactDOM.createRoot(document.getElementById("root")!).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
}
