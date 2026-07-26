import { useState, type ReactNode } from "react";
import { errText, t, tf } from "../core/i18n";
import { setPluginEnabled, type PluginInstall } from "../core/pluginInstalls";

/**
 * One installed plugin's switch, wherever a face draws it — the market's detail and the installed
 * screen both hand the same control to the same seam (`AMB-D-351`/`AMB-D-379`).
 *
 * **The consent is asked here, once per device.** Enabling means running somebody else's code on this
 * machine, so the first enable stops and asks; core records the answer, and `consented` on the row is what
 * keeps every later enable from asking again (a disable does not take it back — `disable ≠ uninstall`).
 *
 * **One switch, and the author says which** (`AMB-D-379`). A `machine` plugin has the device's, a
 * `project` plugin has one project's — so the second needs a project named before it can be moved, and the
 * picker is that, not a choice of level. Everything else that can refuse (a build this amenbo cannot speak
 * to, a `required` setting with no value) is core's judgement, shown here as the reason it gave.
 */
export function PluginGate({ install, projects, projectId, onProject, lead }: {
  install: PluginInstall;
  /** The projects a project-scoped gate can be moved in — the store's, for the picker below. */
  projects: { id: number; name: string }[];
  /** Which project this gate speaks for (`null` = none chosen yet). */
  projectId: number | null;
  onProject: (id: number | null) => void;
  /** Drawn first in the row — what the surrounding face wants said beside the switch. */
  lead?: ReactNode;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // What the last disable threw away, until the switch is moved again. Zero is not a state worth a line:
  // an empty queue is the ordinary case, and saying so every time would train the eye past the one time
  // it matters — the same silence the CLI keeps.
  const [dropped, setDropped] = useState(0);
  // Open only while the consent question is on screen. Not a stored answer: what is remembered lives on
  // the device (`consented`), and this is just the asking.
  const [asking, setAsking] = useState(false);

  const move = async (next: boolean) => {
    setBusy(true);
    setError(null);
    setDropped(0);
    try {
      const moved = await setPluginEnabled(install.name, projectId, next);
      setDropped(moved.droppedQueued);
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  };

  const perProject = install.scope === "project";
  const enabled = install.enabled === true;
  const where = t(perProject ? "plugins.gate.project" : "plugins.gate.machine");
  // A project-scoped gate with no project named has no answer to move — the picker is the way out, so the
  // buttons wait for it rather than acting on some default project nobody chose.
  const unanswered = perProject && projectId == null;

  return (
    <div className="pluggate">
      {lead}
      {perProject && (
        <select
          value={projectId ?? ""}
          onChange={(e) => onProject(e.target.value === "" ? null : Number(e.target.value))}
          style={{ fontSize: "var(--fs-xs)" }}
        >
          <option value="">{t("plugins.pickProject")}</option>
          {projects.map((p) => (
            <option key={p.id} value={p.id}>{p.name}</option>
          ))}
        </select>
      )}
      {!unanswered && (
        <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>
          {tf(enabled ? "plugins.enabledAt" : "plugins.disabledAt", { where })}
        </span>
      )}
      {enabled ? (
        <button className="feed__action" disabled={busy} onClick={() => void move(false)}>
          {t("plugins.disable")}
        </button>
      ) : (
        <button
          className="btn"
          disabled={busy || unanswered || !install.compatible}
          onClick={() => (install.consented ? void move(true) : setAsking(true))}
        >
          {t("plugins.enable")}
        </button>
      )}
      {/* Not a warning about this machine but about the plugin: an open gate on a build amenbo cannot
          speak to fires nothing, so saying why beats a switch that appears to work. */}
      {!install.compatible && (
        <div className="pluggate__note">
          {install.incompatibleReason ?? t("plugins.incompatible")}
        </div>
      )}
      {unanswered && <div className="pluggate__note faint">{t("plugins.pickProjectNote")}</div>}
      {/* The one thing a disable does that cannot be undone: those events are not delivered late, and
          re-enabling starts from now (`AMB-D-399`). The CLI has always said it; this is the same line. */}
      {dropped > 0 && (
        <div className="pluggate__note">{tf("plugins.droppedQueued", { count: dropped })}</div>
      )}
      {asking && (
        <div className="pluggate__consent">
          <div>{tf("plugins.consentAsk", { name: install.name })}</div>
          <div className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("plugins.consentOnce")}</div>
          <div className="pluggate">
            <button
              className="btn"
              disabled={busy}
              onClick={() => { setAsking(false); void move(true); }}
            >
              {t("plugins.consentAgree")}
            </button>
            <button className="feed__action" disabled={busy} onClick={() => setAsking(false)}>
              {t("plugins.consentCancel")}
            </button>
          </div>
        </div>
      )}
      {error && <div className="pluggate__note">{error}</div>}
    </div>
  );
}
