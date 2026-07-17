// The restart screen, shown once the store is found to have moved ahead of this build.
//
// A store newer than this build leaves nothing this build can do, so the screen takes the whole window rather
// than letting a half-working app show through behind it.
//
// The only button is restart. The executable on disk is already the new version (the GUI and the CLI ship
// together), so starting again is enough to come back on it. Nothing is fetched: this is not a self-update.
import { useEffect, useState } from "react";
import { invoke } from "../core/ipc";
import { currentLang, normalizeLang, t } from "../core/i18n";
import { inTauri } from "../core/snapshot";

/**
 * The gate that announces the overtaking — a store too new to open. When this is noticed at startup the snapshot
 * has never been read, so `currentLang()` falls back to the default; the real language is fetched again from
 * `config.json`, which is a separate file from the store and is not subject to the version gate.
 */
export function RestartGate() {
  const [lang, setLang] = useState(currentLang);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    if (!inTauri()) return;
    let alive = true;
    void invoke<string | null>("ui_language")
      .then((code) => { if (alive) setLang(normalizeLang(code)); })
      .catch(() => {}); // unreadable: stay on the default, which beats not showing the restart screen at all
    return () => { alive = false; };
  }, []);

  async function restart() {
    // On success the process never comes back. If it does return, or throws, the restart failed — ask for a
    // manual quit.
    setFailed(false);
    try {
      if (!inTauri()) throw new Error("not in tauri");
      await invoke("restart_app");
      setFailed(true);
    } catch {
      setFailed(true);
    }
  }

  return (
    <div className="setup__overlay">
      <div className="setup__modal">
        <div className="setup__hero">
          <div className="setup__goose">🪿</div>
          <h2>{t("restart.title", lang)}</h2>
          <p className="muted">{t("restart.intro", lang)}</p>
        </div>

        <p className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("restart.how", lang)}</p>

        {failed && (
          <div className="restart__error">
            <pre style={{ whiteSpace: "pre-wrap", margin: 0 }}>{t("restart.failed", lang)}</pre>
          </div>
        )}

        <div className="restart__actions">
          <button className="btn btn--primary" onClick={() => void restart()}>
            {t("restart.button", lang)}
          </button>
        </div>
      </div>
    </div>
  );
}
