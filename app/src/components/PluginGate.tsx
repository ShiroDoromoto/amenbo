import { useState, type ReactNode } from "react";
import { errText, t, tn } from "../core/i18n";
import { setPluginEnabled, type PluginInstall } from "../core/pluginInstalls";

/**
 * One installed plugin's switch, wherever a face draws it — the market's detail and the installed
 * screen both hand the same control to the same seam (`AMB-D-351`/`AMB-D-434`).
 *
 * **It names the projects the plugin is on in** (`AMB-D-412`), rather than answering for whichever one
 * a screen happens to be looking through. Nothing named means off everywhere, and that is an answer:
 * a plugin still firing in a project nobody is looking at is exactly what a single truth value hides.
 *
 * Only the projects it is on in are listed, and another is added from the picker beside them — so a
 * store with fifty projects draws the same short row as a store with two. Picking one **is** the
 * enable: turning a plugin on is itself the permission to run its code, so there is no second question
 * to ask. Everything that can refuse (a build this amenbo cannot speak to, a `required` setting with no
 * value in that project) is core's judgement, shown here as the reason it gave.
 */
export function PluginGate({ install, projects, lead }: {
  install: PluginInstall;
  /** The projects the gate can be moved in — the store's, for the list and the picker. */
  projects: { id: number; name: string }[];
  /** Drawn first in the row — what the surrounding face wants said beside the switch. */
  lead?: ReactNode;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // What the last disable threw away, until the switch is moved again. Zero is not a state worth a line:
  // an empty queue is the ordinary case, and saying so every time would train the eye past the one time
  // it matters — the same silence the CLI keeps.
  const [dropped, setDropped] = useState(0);

  const move = async (project: number, next: boolean) => {
    setBusy(true);
    setError(null);
    setDropped(0);
    try {
      const moved = await setPluginEnabled(install.name, project, next);
      setDropped(moved.droppedQueued);
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  };

  // In the store's own order, so the same two projects do not swap places between two rows.
  const on = projects.filter((p) => install.enabledProjects.includes(p.id));
  const off = projects.filter((p) => !install.enabledProjects.includes(p.id));

  return (
    <div className="pluggate">
      {lead}
      {on.length === 0 ? (
        <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>
          {t("plugins.gate.offEverywhere")}
        </span>
      ) : (
        <>
          <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>
            {t("plugins.gate.onIn")}
          </span>
          {on.map((p) => (
            <span key={p.id} className="pluggate__on">
              <span className="chip">{p.name}</span>
              <button
                className="feed__action"
                disabled={busy}
                onClick={() => void move(p.id, false)}
              >
                {t("plugins.disable")}
              </button>
            </span>
          ))}
        </>
      )}
      {/* The picker is the enable, and it carries only what the row does not already say. A plugin on
          in every project there is has nothing left to add, so it is not drawn. */}
      {off.length > 0 && (
        <select
          value=""
          disabled={busy || !install.compatible}
          onChange={(e) => void move(Number(e.target.value), true)}
          style={{ fontSize: "var(--fs-xs)" }}
        >
          <option value="">{t("plugins.gate.addProject")}</option>
          {off.map((p) => (
            <option key={p.id} value={p.id}>{p.name}</option>
          ))}
        </select>
      )}
      {/* Not a warning about this machine but about the plugin: an open gate on a build amenbo cannot
          speak to fires nothing, so saying why beats a switch that appears to work. */}
      {!install.compatible && (
        <div className="pluggate__note">
          {install.incompatibleReason ?? t("plugins.incompatible")}
        </div>
      )}
      {/* The one thing a disable does that cannot be undone: those events are not delivered late, and
          re-enabling starts from now (`AMB-D-399`). The CLI has always said it; this is the same line. */}
      {dropped > 0 && (
        <div className="pluggate__note">{tn("plugins.droppedQueued", dropped)}</div>
      )}
      {error && <div className="pluggate__note">{error}</div>}
    </div>
  );
}
