import { useEffect, useState, useSyncExternalStore } from "react";
import { PluginConfigForm } from "../components/PluginConfigForm";
import { PluginGate } from "../components/PluginGate";
import { confirmDialog } from "../core/dialog";
import { errText, t, tn, tf } from "../core/i18n";
import {
  uninstallPlugin,
  usePluginInstalls,
  type PluginInstall,
  type PluginRemoved,
} from "../core/pluginInstalls";
import {
  applyPluginUpdate,
  clearDismissedPluginUpdates,
  refreshPluginUpdates,
  rollbackPlugin,
  usePluginUpdates,
  type PluginUpdate,
} from "../core/pluginUpdates";
import { getSnapshot, subscribe } from "../core/snapshot";

// What this machine holds — the "manage what you have" half of the plugin section (`AMB-D-356`), beside
// the market's "find one".
//
// **It never reads the catalog.** An install is a directory on disk plus this store, so this screen
// answers offline and keeps answering when the catalog cannot be reached: the plugins you have are yours
// whether or not the index that offered them is up. The cost is that nothing here shows what the catalog
// knows (a description, an author) — those belong to the market's copy of the entry, not to the install.
//
// Everything a row can *do* is one control (`PluginGate`), the same one the market's detail draws, so the
// project a gate speaks for cannot mean two different things on two screens.

export function PluginInstalledScreen() {
  const projects = useSyncExternalStore(subscribe, () => getSnapshot().projects);
  // Which project a gate speaks for (`AMB-D-434`). This screen is not opened inside a
  // project, so it has to be named — except on a store with exactly one project, where naming it would be
  // asking a question with a single answer.
  const [pickedProject, setPickedProject] = useState<number | null>(null);
  const gateProject = pickedProject ?? (projects.length === 1 ? projects[0].id : null);
  const { installs, loading, error } = usePluginInstalls(gateProject);
  // Opening this screen is one of the update triggers (`AMB-D-359`) — core answers from the catalog's
  // freshness window, so arriving here inside the hour costs nothing. The offer itself is the shell's banner;
  // what this screen reads it for is the "nothing is waiting" the banner has no reason to say.
  const { updates, loading: checking } = usePluginUpdates();
  const [checked, setChecked] = useState(false);
  useEffect(() => { refreshPluginUpdates(); }, []);
  // What the last uninstall took, kept here because the row that did it is gone by the time it is drawn.
  const [removed, setRemoved] = useState<{ name: string; parts: string } | null>(null);

  return (
    <>
      <div className="filterbar">
        <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>🧩 {t("plugins.installed")}</span>
        <span className="topbar__spacer" style={{ flex: 1 }} />
        {checked && !checking && updates.length === 0 && (
          <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("plugins.updates.none")}</span>
        )}
        {/* Asking in so many words also un-dismisses: a build waved away earlier is what the asker wants told. */}
        <button
          className="feed__action"
          disabled={checking}
          onClick={() => { setChecked(true); clearDismissedPluginUpdates(); refreshPluginUpdates(); }}
        >
          {checking ? t("plugins.updates.checking") : t("plugins.updates.check")}
        </button>
        <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>
          {tf("plugins.installedCount", { count: installs.length })}
        </span>
      </div>

      <div style={{ padding: 12, overflowY: "auto" }}>
        {error != null && (
          <div style={{ color: "var(--c-warn)", padding: "var(--s-2) 0" }}>{t("plugins.installsError")}</div>
        )}
        {/* The receipt outlives the row it is about: what an uninstall took can only be said once the
            plugin is gone from the list, so it is said here rather than where the row was. */}
        {removed && (
          <div className="faint" style={{ fontSize: "var(--fs-xs)", padding: "var(--s-2) 0" }}>
            {removed.parts
              ? tf("plugins.removed", { name: removed.name, what: removed.parts })
              : tf("plugins.removedNothing", { name: removed.name })}
          </div>
        )}
        {loading && installs.length === 0 && (
          <div style={{ color: "var(--c-muted)", padding: 16 }}>{t("app.loading")}</div>
        )}
        {!loading && installs.length === 0 && (
          <div style={{ color: "var(--c-muted)", padding: 16 }}>
            <div>{t("plugins.emptyInstalled")}</div>
            <div className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("plugins.emptyInstalledNote")}</div>
          </div>
        )}
        {installs.map((install) => (
          <InstalledRow
            key={install.name}
            install={install}
            update={updates.find((u) => u.name === install.name)}
            projects={projects}
            projectId={gateProject}
            onProject={setPickedProject}
            onRemoved={setRemoved}
          />
        ))}
      </div>
    </>
  );
}

/**
 * What an uninstall took, as a list a person can read — empty when the name held nothing on this machine.
 *
 * The settings are one item however many projects held them, and the binary is not the headline: what
 * makes `AMB-D-357` worth saying out loud is that the settings and the secrets went with it.
 */
function removedParts(r: PluginRemoved): string {
  const parts = [
    r.directory && t("plugins.removedPart.binary"),
    r.projectValues > 0 && t("plugins.removedPart.settings"),
    r.secrets && t("plugins.removedPart.secrets"),
    r.runsLog && t("plugins.removedPart.runs"),
  ].filter((p): p is string => typeof p === "string");
  return parts.join(t("common.listSeparator"));
}

/**
 * How many settings the author marked `required` this project holds no value for — the count an enable
 * is refused over (`AMB-D-356`). There is one value per setting per project (`AMB-D-434`), so a field is
 * held or it is not.
 */
function requiredUnset(install: PluginInstall): number {
  return install.config.filter(
    (f) => f.required && (f.secret ? !f.secretSet : f.value == null),
  ).length;
}

/**
 * One installed plugin: its name, which switch it has, and that switch.
 *
 * Installed and enabled are two facts (`AMB-D-351`), and a plugin that is here but fires nothing is the
 * ordinary state — so the row leads with the name and lets the gate say where it stands, rather than
 * badging "installed" on a screen where everything is.
 *
 * **An open gate is not the same as a plugin that fires** (`AMB-D-359`). A build this amenbo cannot speak
 * to — a payload contract that is not ours, a version floor above us — is handed no event, whatever its
 * switch says. So an incompatible row wears that instead of the plain "enabled", and the gate below it
 * carries core's own reason: an enabled plugin sitting silent is exactly the state a badge has to name.
 *
 * The settings sit under the switch, and only for a plugin whose author declared any: a `required`
 * setting with no value is what an enable is refused for (`AMB-D-356`), so the way to fill it in belongs
 * beside the switch that will say no.
 *
 * **The build moves from here too** (`AMB-D-359`). The banner takes updates in bulk for whoever just wants
 * them; this row is the other half of the same offer, for choosing one plugin at a time — and it is where
 * an offer that needs a decision is actually resolvable, since the settings the new schema wants are one
 * button away. The way back is here for the same reason: this face applies updates, so it owes the undo.
 */
function InstalledRow({ install, update, projects, projectId, onProject, onRemoved }: {
  install: PluginInstall;
  /** The build the catalog holds for this plugin, when it is not the one installed. */
  update?: PluginUpdate;
  projects: { id: number; name: string }[];
  projectId: number | null;
  onProject: (id: number | null) => void;
  /** Report what an uninstall took, for the screen to say once this row is gone. */
  onRemoved: (r: { name: string; parts: string }) => void;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [settings, setSettings] = useState(false);
  // What the last build move did, said on the row it was about — the offer is gone by the time it is drawn.
  const [moved, setMoved] = useState<string | null>(null);

  const run = async (op: () => Promise<string | null>) => {
    setBusy(true);
    setError(null);
    setMoved(null);
    try {
      setMoved(await op());
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  };

  // Taking an update needs no question: core re-verifies the asset, keeps the gate, the settings and the
  // secrets, and retains the build being replaced — which is what the roll-back beside it goes to.
  const onUpdate = () =>
    run(async () =>
      (await applyPluginUpdate(install.name)) ? tf("plugins.updates.applied", { count: 1 }) : null,
    );

  // Going back does need one: the retained build is the only one there is, and this consumes it.
  const onRollback = async () => {
    if (!(await confirmDialog(tf("plugins.updates.rollbackConfirm", { name: install.name })))) return;
    await run(async () => {
      const restored = await rollbackPlugin(install.name);
      return restored == null ? null : tf("plugins.updates.rolledBack", { desc: restored });
    });
  };

  // The question names what goes beyond the binary (`AMB-D-357`): the settings in every project, the
  // secrets are the part nobody pictures, and they do not come back with a re-install.
  const onRemove = async () => {
    if (!(await confirmDialog(tf("plugins.removeConfirm", { name: install.name })))) return;
    setBusy(true);
    setError(null);
    try {
      const removed = await uninstallPlugin(install.name);
      if (removed) onRemoved({ name: install.name, parts: removedParts(removed) });
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="feed__item">
      <div className="feed__body" style={{ minWidth: 0 }}>
        <div className="feed__line">
          <strong>{install.name}</strong>{" "}
          <span className="chip">{t("plugins.gate.project")}</span>
          {!install.compatible ? (
            <>
              {" "}
              <span className="chip chip--warn">
                {t(install.enabled === true ? "plugins.notFiring" : "plugins.incompatibleChip")}
              </span>
            </>
          ) : install.enabled === true ? (
            <>
              {" "}
              <span className="chip">{t("plugins.enabledChip")}</span>
            </>
          ) : null}
        </div>
        <PluginGate
          install={install}
          projects={projects}
          projectId={projectId}
          onProject={onProject}
        />
        {install.config.length > 0 && (
          <div className="pluggate">
            <button className="feed__action" onClick={() => setSettings((s) => !s)}>
              {settings ? t("plugins.cfg.hide") : t("plugins.cfg.open")}
            </button>
            {/* Said on the closed row too: this is why an enable is refused, and the row is where
                anyone looking at that refusal is standing. */}
            {requiredUnset(install) > 0 && (
              <span className="chip chip--warn">
                {tn("plugins.cfg.requiredUnset", requiredUnset(install))}
              </span>
            )}
          </div>
        )}
        {settings && (
          <PluginConfigForm
            install={install}
            projects={projects}
            projectId={projectId}
            onProject={onProject}
          />
        )}
        {(update || install.rollback || moved) && (
          <div className="pluggate">
            {/* An offer that needs a decision is named instead of offered as a button that would only be
                refused — and the settings it is short of are opened from this same row. */}
            {update?.hold ? (
              <span className="pluggate__note">
                {update.hold === "incompatible"
                  ? tf("plugins.updates.holdIncompatible", { name: update.name })
                  : tf("plugins.updates.holdSettings", {
                      name: update.name,
                      keys: update.missing.join(t("common.listSeparator")),
                    })}
              </span>
            ) : (
              update && (
                <>
                  <span className="chip">{t("plugins.updates.waiting")}</span>
                  <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>{update.desc}</span>
                  <button className="btn" disabled={busy} onClick={() => void onUpdate()}>
                    {busy ? t("plugins.updates.applying") : t("plugins.updates.apply")}
                  </button>
                </>
              )
            )}
            {install.rollback && (
              <button className="feed__action" disabled={busy} onClick={() => void onRollback()}>
                {t("plugins.updates.rollback")}
              </button>
            )}
            {moved && !busy && (
              <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>{moved}</span>
            )}
          </div>
        )}
        {/* Set apart from the gate: disabling is a switch that can be flicked back, this is not
            (`AMB-D-357`). */}
        <div className="pluggate">
          <button className="feed__action" disabled={busy} onClick={() => void onRemove()}>
            {busy ? t("plugins.removing") : t("plugins.remove")}
          </button>
          {error && <div className="pluggate__note">{error}</div>}
        </div>
      </div>
    </div>
  );
}
