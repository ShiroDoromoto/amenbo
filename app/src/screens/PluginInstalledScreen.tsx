import { useState, useSyncExternalStore } from "react";
import { PluginGate } from "../components/PluginGate";
import { t, tf } from "../core/i18n";
import { usePluginInstalls, type PluginInstall } from "../core/pluginInstalls";
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
// consent question and the project a gate speaks for cannot mean two different things on two screens.

export function PluginInstalledScreen() {
  const projects = useSyncExternalStore(subscribe, () => getSnapshot().projects);
  // Which project a project-scoped gate speaks for (`AMB-D-379`). This screen is not opened inside a
  // project, so it has to be named — except on a store with exactly one project, where naming it would be
  // asking a question with a single answer.
  const [pickedProject, setPickedProject] = useState<number | null>(null);
  const gateProject = pickedProject ?? (projects.length === 1 ? projects[0].id : null);
  const { installs, loading, error } = usePluginInstalls(gateProject);

  return (
    <>
      <div className="filterbar">
        <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>🧩 {t("plugins.installed")}</span>
        <span className="topbar__spacer" style={{ flex: 1 }} />
        <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>
          {tf("plugins.installedCount", { count: installs.length })}
        </span>
      </div>

      <div style={{ padding: 12, overflowY: "auto" }}>
        {error != null && (
          <div style={{ color: "var(--c-warn)", padding: "var(--s-2) 0" }}>{t("plugins.installsError")}</div>
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
            projects={projects}
            projectId={gateProject}
            onProject={setPickedProject}
          />
        ))}
      </div>
    </>
  );
}

/**
 * One installed plugin: its name, which switch it has, and that switch.
 *
 * Installed and enabled are two facts (`AMB-D-351`), and a plugin that is here but fires nothing is the
 * ordinary state — so the row leads with the name and lets the gate say where it stands, rather than
 * badging "installed" on a screen where everything is.
 */
function InstalledRow({ install, projects, projectId, onProject }: {
  install: PluginInstall;
  projects: { id: number; name: string }[];
  projectId: number | null;
  onProject: (id: number | null) => void;
}) {
  return (
    <div className="feed__item">
      <div className="feed__body" style={{ minWidth: 0 }}>
        <div className="feed__line">
          <strong>{install.name}</strong>{" "}
          <span className="chip">
            {t(install.scope === "project" ? "plugins.gate.project" : "plugins.gate.machine")}
          </span>
          {install.enabled === true && (
            <>
              {" "}
              <span className="chip">{t("plugins.enabledChip")}</span>
            </>
          )}
        </div>
        <PluginGate
          install={install}
          projects={projects}
          projectId={projectId}
          onProject={onProject}
        />
      </div>
    </div>
  );
}
