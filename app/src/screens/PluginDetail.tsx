import { useEffect } from "react";
import { Markdown } from "../components/Markdown";
import { t, tf } from "../core/i18n";
import { openExternalUrl } from "../core/mutations";
import { pluginLayer, usePluginRepoFacts, type PluginEntry } from "../core/pluginCatalog";

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

export function PluginDetail({ entry, onClose }: { entry: PluginEntry; onClose: () => void }) {
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

        <div className="plugdet__readme">
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
