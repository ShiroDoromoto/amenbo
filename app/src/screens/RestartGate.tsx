// The restart screen, shown once the store is found to have moved ahead of this build.
//
// A store newer than this build leaves nothing this build can do, so the screen takes the whole window rather
// than letting a half-working app show through behind it.
//
// Restart is the first thing to try: usually the executable on disk is already the new version (the GUI and
// the CLI ship together), so starting again comes back on it. Nothing is fetched: this is not a self-update.
//
// When it is not — the store was carried forward by a build that is not installed here — restarting returns to
// this same screen, so the way out is spelled out beside the button: there is no downgrade, and the way back is
// a restore from the pre-migration backup. The refusal core wrote is shown verbatim there, because it names
// the version that wrote the store and nothing on this screen can ask the store anything.
import { useEffect, useState } from "react";
import { invoke } from "../core/ipc";
import { useCliCommandName } from "../core/cliCommand";
import { formatAheadDetail } from "../core/formatAhead";
import { currentLang, normalizeLang, t, tf } from "../core/i18n";
import { inTauri } from "../core/snapshot";
import { Icon } from "../components/Icon";

/**
 * The gate that announces the overtaking — a store too new to open. When this is noticed at startup the snapshot
 * has never been read, so `currentLang()` falls back to the default; the real language is fetched again from
 * `config.json`, which is a separate file from the store and is not subject to the version gate.
 */
export function RestartGate() {
  const [lang, setLang] = useState(currentLang);
  const [failed, setFailed] = useState(false);
  // The restore line is a command to type, and the build that is stuck here is the one whose CLI the
  // reader has beside them.
  const cli = useCliCommandName();
  // Read once: the flag never lowers, so neither does what raised it.
  const [detail] = useState(formatAheadDetail);

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
    <div className="modal__overlay">
      <div className="modal__card">
        <div className="modal__hero">
          <div className="modal__goose"><Icon name="goose" size="lg" /></div>
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

        <div className="restart__stuck">
          <h3>{t("restart.stuck.title", lang)}</h3>
          <p className="muted">{t("restart.stuck.intro", lang)}</p>
          {detail && (
            <pre className="restart__detail" style={{ whiteSpace: "pre-wrap" }}>
              {detail}
            </pre>
          )}
          <p className="muted">{t("restart.stuck.how", lang)}</p>
          <pre className="restart__detail" style={{ whiteSpace: "pre-wrap" }}>{tf("restart.stuck.command", { cmd: cli }, lang)}</pre>
          <p className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("restart.stuck.where", lang)}</p>
        </div>
      </div>
    </div>
  );
}
