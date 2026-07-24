import { useEffect, useState } from "react";
import { Markdown } from "../components/Markdown";
import { errText, t, tf } from "../core/i18n";
import { openExternalUrl } from "../core/mutations";
import { pluginLayer, usePluginRepoFacts, type PluginEntry } from "../core/pluginCatalog";
import { installPlugin, setPluginEnabled, type PluginInstall } from "../core/pluginInstalls";

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
          <button className="feed__action" onClick={() => void openExternalUrl(`https://github.com/${entry.repo}`)}>
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

        <div className="plugdet__readme markdown">
          {facts?.readme ? (
            <Markdown>{facts.readme}</Markdown>
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
 * **The consent is asked here, once per device.** Enabling means running somebody else's code on this
 * machine, so the first enable stops and asks; core records the answer, and `consented` on the row is what
 * keeps every later enable from asking again (a disable does not take it back — `disable ≠ uninstall`).
 *
 * **One switch, and the author says which** (`AMB-D-379`). A `machine` plugin has the device's, a
 * `project` plugin has one project's — so the second needs a project named before it can be moved, and the
 * picker below is that, not a choice of level. Everything else that can refuse (a build this amenbo cannot
 * speak to, a `required` setting with no value) is core's judgement, shown here as the reason it gave.
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
  // Open only while the consent question is on screen. Not a stored answer: what is remembered lives on
  // the device (`consented`), and this is just the asking.
  const [asking, setAsking] = useState(false);

  const run = async (op: () => Promise<unknown>) => {
    setBusy(true);
    setError(null);
    try {
      await op();
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  };

  const failure = error && (
    <div className="plugdet__note" style={{ color: "var(--c-warn)" }}>{error}</div>
  );

  if (!install) {
    return (
      <div className="plugdet__actions">
        <button
          className="btn"
          disabled={busy}
          onClick={() => void run(() => installPlugin(entry.name, projectId))}
        >
          {busy ? t("plugins.installing") : t("plugins.install")}
        </button>
        <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("plugins.installNote")}</span>
        {failure}
      </div>
    );
  }

  const perProject = install.scope === "project";
  const enabled = install.enabled === true;
  const where = t(perProject ? "plugins.gate.project" : "plugins.gate.machine");
  // A project-scoped gate with no project named has no answer to move — the picker is the way out, so the
  // buttons wait for it rather than acting on some default project nobody chose.
  const unanswered = perProject && projectId == null;
  const move = (next: boolean) => run(() => setPluginEnabled(entry.name, projectId, next));

  return (
    <div className="plugdet__actions">
      <span className="chip">{t("plugins.installed")}</span>
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
        <div className="plugdet__note" style={{ color: "var(--c-warn)" }}>
          {install.incompatibleReason ?? t("plugins.incompatible")}
        </div>
      )}
      {unanswered && <div className="plugdet__note faint">{t("plugins.pickProjectNote")}</div>}
      {asking && (
        <div className="plugdet__consent">
          <div>{tf("plugins.consentAsk", { name: entry.name })}</div>
          <div className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("plugins.consentOnce")}</div>
          <div className="plugdet__actions">
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
      {failure}
    </div>
  );
}
