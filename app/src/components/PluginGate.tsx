import { useState, type ReactNode } from "react";
import { errText, t, tn, tf } from "../core/i18n";
import { setPluginEnabled, type PluginInstall } from "../core/pluginInstalls";

/**
 * One installed plugin's switch, wherever a face draws it — the market's detail and the installed
 * screen both hand the same control to the same seam (`AMB-D-351`/`AMB-D-434`).
 *
 * **One switch, and it is a project's** (`AMB-D-434`). So a project has to be named before the switch can
 * be moved, and the picker is that, not a choice of level. Turning a plugin on is itself the permission to
 * run its code, so there is no second question to ask. Everything that can refuse (a build this amenbo
 * cannot speak to, a `required` setting with no value) is core's judgement, shown here as the reason it
 * gave.
 */
export function PluginGate({ install, projects, projectId, onProject, lead }: {
  install: PluginInstall;
  /** The projects the gate can be moved in — the store's, for the picker below. */
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

  const enabled = install.enabled === true;
  const where = t("plugins.gate.project");
  // A gate with no project named has no answer to move — the picker is the way out, so the buttons wait
  // for it rather than acting on some default project nobody chose.
  const unanswered = projectId == null;

  return (
    <div className="pluggate">
      {lead}
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
          onClick={() => void move(true)}
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
        <div className="pluggate__note">{tn("plugins.droppedQueued", dropped)}</div>
      )}
      {error && <div className="pluggate__note">{error}</div>}
    </div>
  );
}
