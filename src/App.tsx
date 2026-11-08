import { useApp } from "./store/app";
import { Onboarding } from "./views/Onboarding";
import { Player } from "./views/Player";
import { Settings } from "./views/Settings";

export function App() {
  const ready = useApp((s) => s.ready);
  const s = useApp((st) => st.settings);
  const toast = useApp((st) => st.toast);
  if (!ready || !s) return null;
  if (location.hash === "#settings") {
    return (
      <>
        <Settings />
        {toast && <div className="toast">{toast}</div>}
      </>
    );
  }
  if (!s.onboarded) return <>{<Onboarding />}{toast && <div className="toast">{toast}</div>}</>;
  return <Player />;
}
