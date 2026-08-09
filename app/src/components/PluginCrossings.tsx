import { useState } from "react";
import { t } from "../core/i18n";
import { type PluginInstall } from "../core/pluginInstalls";
import { PluginCrossingRow } from "./PluginCrossingRow";

/**
 * One installed plugin's crossings, drawn from the plugin's side (`AMB-D-447`) — a row per project,
 * each carrying that project's switch and that project's settings.
 *
 * **What is listed is what there is something to say about**: the projects the plugin fires in
 * (`AMB-D-412`) and the projects that filled it in without turning it on (`AMB-D-434`). Nothing listed
 * means off everywhere and filled in nowhere, which is an answer — a plugin still firing in a project
 * nobody is looking at is exactly what a single truth value hides.
 *
 * Another project is **added** from the picker rather than enabled by it. A crossing has to exist before
 * its `required` settings can be filled in, and the mark that says an enable would be refused is only
 * worth having if it is readable before the switch is pressed — so picking draws the row, and the row's
 * own switch is the permission to run somebody else's code (`AMB-D-351`).
 *
 * **A plugin its author declared the machine's gets one row instead of that list** (`AMB-D-601`). Its
 * gate, its settings and its secrets are the device's, so there is no project to cross and nothing for a
 * picker to add: a list drawn for it would be projects whose switches move a gate none of them owns. The
 * one row is the whole of what there is to read and to move, and pressing its switch is the consent to
 * let the plugin read every project on the machine.
 */
export function PluginCrossings({ install, projects }: {
  install: PluginInstall;
  /** The projects a crossing can exist in — the store's, for the rows and the picker. */
  projects: { id: number; name: string }[];
}) {
  // Projects opened from the picker, which the install says nothing about yet. Kept until the row is
  // navigated away from, so turning a plugin off does not make the row someone is working in vanish.
  const [added, setAdded] = useState<number[]>([]);

  // In the store's own order, so the same two projects do not swap places between two rows.
  const shown = projects.filter(
    (p) => added.includes(p.id) || install.projects.some((row) => row.project === p.id),
  );
  const rest = projects.filter((p) => !shown.includes(p));

  // The device layer is not a shorter project list — it is a different row, with nothing to add beside
  // it. The one warning that still belongs under it is the build's: an open gate on a plugin this amenbo
  // cannot speak to fires nothing, wherever that gate sits.
  if (install.device) {
    return (
      <>
        <PluginCrossingRow install={install} layer={null} name={t("plugins.gate.device")} />
        {!install.compatible && (
          <div className="pluggate">
            <div className="pluggate__note">
              {install.incompatibleReason ?? t("plugins.incompatible")}
            </div>
          </div>
        )}
      </>
    );
  }

  return (
    <>
      {shown.length === 0 && (
        <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>
          {t("plugins.gate.offEverywhere")}
        </span>
      )}
      {shown.map((p) => (
        <PluginCrossingRow key={p.id} install={install} layer={p.id} name={p.name} />
      ))}
      {/* A plugin with a row in every project there is has nothing left to add, and a compatible one
          has nothing to warn about — so the line under the rows is drawn only when it carries something. */}
      {(rest.length > 0 || !install.compatible) && (
        <div className="pluggate">
          {rest.length > 0 && (
            <select
              value=""
              onChange={(e) => setAdded((a) => [...a, Number(e.target.value)])}
              style={{ fontSize: "var(--fs-xs)" }}
            >
              <option value="">{t("plugins.gate.addProject")}</option>
              {rest.map((p) => (
                <option key={p.id} value={p.id}>{p.name}</option>
              ))}
            </select>
          )}
          {/* Not a warning about this machine but about the plugin: an open gate on a build amenbo
              cannot speak to fires nothing, so saying why beats a switch that appears to work. */}
          {!install.compatible && (
            <div className="pluggate__note">
              {install.incompatibleReason ?? t("plugins.incompatible")}
            </div>
          )}
        </div>
      )}
    </>
  );
}
