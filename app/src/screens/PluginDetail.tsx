import { useEffect, useState } from "react";
import { Markdown } from "../components/Markdown";
import { PluginGate } from "../components/PluginGate";
import { errText, t, tf } from "../core/i18n";
import { openExternalUrl } from "../core/mutations";
import { pluginLayer, repoLinkBase, repoUrl, usePluginRepoFacts, type PluginEntry } from "../core/pluginCatalog";
import { installPlugin, type PluginInstall } from "../core/pluginInstalls";

// The one plugin a user opened (`AMB-D-347`).
//
// The market list is drawn from the catalog alone, and everything that costs a request lives here
// instead: the stars, the current release's downloads and the README are per-repository, so they are
// fetched when this opens and for this entry only. That asymmetry is the whole discovery design —
// browsing a catalog of thousands stays one static file, and GitHub is asked about a plugin only when
// someone actually wants to look at it.
//
// The figures never gate anything. What may be installed is decided by the asset's signature against
// amenbo's own key (`AMB-D-371`); a star count is a display figure, and a download count includes
// whatever else pulls an asset, so both are read as a sense of scale and nothing more.

export function PluginDetail({ entry, install, projects, projectId, onProject, onClose }: {
  entry: PluginEntry;
  /** This machine's row for this entry, or `undefined` when it is not installed. */
  install?: PluginInstall;
  /** The projects a project-scoped gate can be moved in — the store's, for the picker below. */
  projects: { id: number; name: string }[];
  /** Which project the gate below speaks for (`null` = none chosen yet). */
  projectId: number | null;
  onProject: (id: number | null) => void;
  onClose: () => void;
}) {
  const { facts, loading, error } = usePluginRepoFacts(entry.repo);
  const layer = pluginLayer(entry);

  // Escape closes it, like every other modal here — the detail is a look, and looking must be cheap to
  // back out of.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div className="setup__overlay" onClick={onClose}>
      <div className="plugdet" onClick={(e) => e.stopPropagation()}>
        <div className="plugdet__head">
          <strong className="plugdet__name">{entry.name}</strong>
          <span className={`chip ${layer === "official" ? "chip--official" : ""}`}>
            {t(`plugins.layer.${layer}`)}
          </span>
          {/* The same pair the row wore, so opening an entry does not quietly drop a claim the list made. */}
          {entry.featured && <span className="chip chip--featured">{t("plugins.featured")}</span>}
          <span className="topbar__spacer" style={{ flex: 1 }} />
          <button className="btn" onClick={onClose}>{t("plugins.close")}</button>
        </div>

        <PluginActions
          entry={entry}
          install={install}
          projects={projects}
          projectId={projectId}
          onProject={onProject}
        />

        <div className="plugdet__desc">{entry.desc}</div>
        <div className="plugdet__meta faint">
          <span>{entry.author}</span>
          <span>·</span>
          <span>{entry.category}</span>
          <span>·</span>
          <span>{entry.os.map((o) => t(`plugins.os.${o}`)).join(" / ")}</span>
          {entry.addedAt && (
            <>
              <span>·</span>
              <span>{tf("plugins.added", { date: entry.addedAt.slice(0, 10) })}</span>
            </>
          )}
        </div>

        {/* Everything below this line came from GitHub, not from the catalog. */}
        <div className="plugdet__figures">
          <button className="feed__action" onClick={() => void openExternalUrl(repoUrl(entry.repo))}>
            {tf("plugins.openRepo", { repo: entry.repo })}
          </button>
          {loading && <span className="faint">{t("plugins.factsLoading")}</span>}
          {facts?.stars != null && <span>★ {facts.stars.toLocaleString()}</span>}
          {facts?.downloads != null && (
            <span>{tf("plugins.downloads", { count: facts.downloads.toLocaleString() })}</span>
          )}
        </div>
        {/* Three different silences, and they are not the same news: too many requests means wait, a
            failure means the figures are missing but the entry is not, and neither is "this plugin has
            no stars". */}
        {facts?.rateLimited && <div className="plugdet__note">{t("plugins.rateLimited")}</div>}
        {error != null && !facts && <div className="plugdet__note">{t("plugins.factsError")}</div>}

        {/* The README is the one body here that came from somewhere: its relative paths name files in
            the repository it was read from, so that repository is what they are resolved against. */}
        <div className="plugdet__readme markdown">
          {facts?.readme ? (
            <Markdown linkBase={repoLinkBase(entry.repo)}>{facts.readme}</Markdown>
          ) : (
            !loading && <span className="faint">{t("plugins.noReadme")}</span>
          )}
        </div>

        <div className="plugdet__foot faint">{t("plugins.factsNote")}</div>
      </div>
    </div>
  );
}

/**
 * The two acts this screen can perform on a plugin, in the order they exist in: **install**, then
 * **enable** (`AMB-D-351`). They are drawn as two separate steps because they are two separate things —
 * installing writes a binary that runs nothing, and only enabling opens the gate it fires through.
 *
 * Only the install half lives here: the switch is `PluginGate`, the one control the installed screen
 * draws too, so the consent question and the project a gate speaks for cannot drift between the two faces.
 */
function PluginActions({ entry, install, projects, projectId, onProject }: {
  entry: PluginEntry;
  install?: PluginInstall;
  projects: { id: number; name: string }[];
  projectId: number | null;
  onProject: (id: number | null) => void;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  if (install) {
    return (
      <PluginGate
        install={install}
        projects={projects}
        projectId={projectId}
        onProject={onProject}
        lead={<span className="chip">{t("plugins.installed")}</span>}
      />
    );
  }

  const runInstall = async () => {
    setBusy(true);
    setError(null);
    try {
      await installPlugin(entry.name, projectId);
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="plugdet__actions">
      <button className="btn" disabled={busy} onClick={() => void runInstall()}>
        {busy ? t("plugins.installing") : t("plugins.install")}
      </button>
      <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("plugins.installNote")}</span>
      {error && <div className="plugdet__note">{error}</div>}
    </div>
  );
}
