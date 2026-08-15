import { useEffect, useState, useSyncExternalStore } from "react";
import { PluginCrossings } from "../components/PluginCrossings";
import { confirmDialog } from "../core/dialog";
import { errText, t, tf } from "../core/i18n";
import {
  firesAnywhere,
  uninstallPlugin,
  usePluginInstalls,
  type PluginInstall,
  type PluginRemoved,
} from "../core/pluginInstalls";
import {
  applyPluginUpdate,
  catalogReadLine,
  clearDismissedPluginUpdates,
  refreshPluginUpdates,
  usePluginUpdates,
  type PluginUpdate,
} from "../core/pluginUpdates";
import { pluginDesc } from "../core/pluginText";
import { getSnapshot, subscribe } from "../core/snapshot";
import { Icon } from "../components/Icon";

// What this machine holds — the "manage what you have" half of the plugin section (`AMB-D-356`), beside
// the market's "find one".
//
// **It never reads the catalog.** An install is a directory on disk plus this store, so this screen
// answers offline and keeps answering when the catalog cannot be reached: the plugins you have are yours
// whether or not the index that offered them is up. The cost is that nothing here shows what the catalog
// knows (a description, an author) — those belong to the market's copy of the entry, not to the install.
//
// Everything a plugin can *do* here is done at a row (`PluginCrossings`, `AMB-D-447`): one per project,
// carrying that project's switch and that project's settings — or, for a plugin its author declared the
// machine's, the one row the device holds (`AMB-D-601`). **The screen holds no project of its own**
// (`AMB-D-412`): the rows name their own, so no single choice up here can decide what a row is allowed
// to say — and it is where a device-wide plugin is managed, since a project's own settings have no row
// for something no project crosses.

export function PluginInstalledScreen() {
  const projects = useSyncExternalStore(subscribe, () => getSnapshot().projects);
  const { installs, loading, error } = usePluginInstalls();
  // Opening this screen is one of the update triggers (`AMB-D-359`) — core answers from the catalog's
  // freshness window, so arriving here inside the hour costs nothing. The offer itself is the shell's banner;
  // what this screen reads it for is what the banner has no reason to say: the "nothing is waiting", and
  // the catalog that answer was measured against.
  const { updates, catalog, loading: checking } = usePluginUpdates();
  const [checked, setChecked] = useState(false);
  // What the count below was measured against, said whenever there is a count to frame. Nothing installed
  // is the one state that reads no catalog, and the empty screen already says the whole of it.
  const framing = checking ? null : catalogReadLine(catalog);
  useEffect(() => { refreshPluginUpdates("incidental"); }, []);
  // What the last uninstall took, kept here because the row that did it is gone by the time it is drawn.
  const [removed, setRemoved] = useState<{ name: string; parts: string } | null>(null);

  return (
    <>
      <div className="filterbar">
        <span className="faint" style={{ fontSize: "var(--fs-xs)" }}><Icon name="puzzle" /> {t("plugins.installed")}</span>
        <span className="topbar__spacer" style={{ flex: 1 }} />
        {/* Ahead of the verdict, not after it: a reader who takes "up to date" at face value has already
            stopped reading by the time a footnote arrives. */}
        {framing && (
          <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>{framing}</span>
        )}
        {checked && !checking && updates.length === 0 && (
          <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("plugins.updates.none")}</span>
        )}
        {/* Asking in so many words goes to the catalog whatever the cache's age (`AMB-D-462`), and also
            un-dismisses: a build waved away earlier is what the asker wants told. */}
        <button
          className="feed__action"
          disabled={checking}
          onClick={() => { setChecked(true); clearDismissedPluginUpdates(); refreshPluginUpdates("now"); }}
        >
          {checking ? t("plugins.updates.checking") : t("plugins.updates.check")}
        </button>
        <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>
          {tf("plugins.installedCount", { count: installs.length })}
        </span>
      </div>

      <div style={{ padding: 12, overflowY: "auto" }}>
        {error != null && (
          <div style={{ color: "var(--c-heed)", padding: "var(--s-2) 0" }}>{t("plugins.installsError")}</div>
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
 * One installed plugin: its name, and the crossings it has with the projects on this device.
 *
 * Installed and enabled are two facts (`AMB-D-351`), and a plugin that is here but fires nothing is the
 * ordinary state — so the plugin's line leads with the name and lets the rows below say where it stands,
 * rather than badging "installed" on a screen where everything is.
 *
 * **An open gate is not the same as a plugin that fires** (`AMB-D-359`). A build this amenbo cannot speak
 * to — a payload contract that is not ours, a version floor above us — is handed no event, whatever its
 * switch says. So an incompatible plugin wears that instead of the plain "enabled", and the rows below it
 * carry core's own reason: an enabled plugin sitting silent is exactly the state a badge has to name.
 *
 * The settings are not a section of their own: a value is one project's (`AMB-D-434`), so it is filled in
 * inside that project's row, beside the switch that would be refused without it (`AMB-D-447`).
 *
 * **The build moves from here too** (`AMB-D-359`). The banner takes updates in bulk for whoever just wants
 * them; this row is the other half of the same offer, for choosing one plugin at a time — and it is where
 * an offer that needs a decision is actually resolvable, since the settings the new schema wants are one
 * button away. It only moves builds forward: the way back is the CLI's alone (`AMB-D-522`).
 */
function InstalledRow({ install, update, projects, onRemoved }: {
  install: PluginInstall;
  /** The build the catalog holds for this plugin, when it is not the one installed. */
  update?: PluginUpdate;
  projects: { id: number; name: string }[];
  /** Report what an uninstall took, for the screen to say once this row is gone. */
  onRemoved: (r: { name: string; parts: string }) => void;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // The badge is about the plugin, so it reads the whole list: on in some project, or on in none.
  const firing = firesAnywhere(install);
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
  // secrets, and retains the build being replaced — which is what the CLI's `plugin rollback` goes to.
  const onUpdate = () =>
    run(async () =>
      (await applyPluginUpdate(install.name)) ? tf("plugins.updates.applied", { count: 1 }) : null,
    );

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
          <strong>{install.name}</strong>
          {!install.compatible ? (
            <>
              {" "}
              <span className="chip chip--heed">
                {t(firing ? "plugins.notFiring" : "plugins.incompatibleChip")}
              </span>
            </>
          ) : firing ? (
            <>
              {" "}
              <span className="chip">{t("plugins.enabledChip")}</span>
            </>
          ) : null}
        </div>
        {/* The layer its author declared, said in words above the gate it is about (`AMB-D-601`). Nothing
            here is settable — the declaration is what makes an enable mean exactly one thing — so a
            device-wide plugin gets a sentence and not a second switch, and a project's plugin gets
            nothing, that being the ordinary case the rows below already read as. */}
        {install.scope === "machine" && (
          <div className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("plugins.scope.machine")}</div>
        )}
        <PluginCrossings install={install} projects={projects} />
        {(update || moved) && (
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
                  <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>{pluginDesc(update)}</span>
                  <button className="btn" disabled={busy} onClick={() => void onUpdate()}>
                    {busy ? t("plugins.updates.applying") : t("plugins.updates.apply")}
                  </button>
                </>
              )
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
