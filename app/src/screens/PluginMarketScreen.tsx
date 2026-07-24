import { useMemo, useState, useSyncExternalStore } from "react";
import { Pager, usePager } from "../components/Pager";
import { errText, t, tf } from "../core/i18n";
import {
  addCatalogSource, filterPlugins, pluginCategories, pluginLayer, removeCatalogSource, sortPlugins,
  unreachableSources, usePluginCatalog,
  type PluginCatalog, type PluginEntry, type PluginLayer, type PluginSort,
} from "../core/pluginCatalog";
import { installOf, usePluginInstalls, type PluginInstall } from "../core/pluginInstalls";
import { getSnapshot, subscribe } from "../core/snapshot";
import { PluginDetail } from "./PluginDetail";

// The plugin market — the "find one" half of the plugin section (`AMB-D-356`); managing what is
// installed is its own surface. The catalog arrives once as a merged list (`AMB-D-347`) and
// everything on this screen happens over that copy: the search, the three narrowing controls and
// the pager never go back to the network, and the list never asks GitHub about an entry it is
// merely showing. Only one page is drawn, so a catalog that grows to thousands of entries costs the
// same DOM as an empty one.
//
// Browsing only. Installing is a separate act with its own consent (`AMB-D-351`), and stars, the
// README and download counts belong to the detail of the one entry a user opens.

/** The OS filter's choices — a closed vocabulary (core's `Os`), unlike the catalog-curated categories. */
const OS_CHOICES = ["macos", "windows", "linux"] as const;

/** The trust layers, widest first, as the filter offers them. */
const LAYER_CHOICES: PluginLayer[] = ["listed", "official", "third-party"];

/** The orderings on offer. "Popular" is not among them: stars are fetched for one opened entry, never for a list. */
const SORT_CHOICES: PluginSort[] = ["new", "name"];

export function PluginMarketScreen() {
  const { catalog, loading, error } = usePluginCatalog();
  const [search, setSearch] = useState("");
  const [category, setCategory] = useState("");
  const [os, setOs] = useState("");
  const [layer, setLayer] = useState<PluginLayer | "">("");
  const [sort, setSort] = useState<PluginSort>("new");
  const [sourcesOpen, setSourcesOpen] = useState(false);
  // The opened entry is held by name, not as a row: the catalog can be refetched underneath, and a
  // detail must then show what the catalog now holds rather than a copy frozen at the click.
  const [openName, setOpenName] = useState<string | null>(null);
  // Which project a project-scoped gate speaks for (`AMB-D-379`). The market is not opened inside a
  // project, so it has to be named — except on a store with exactly one project, where naming it would be
  // asking a question with a single answer.
  const [pickedProject, setPickedProject] = useState<number | null>(null);
  const projects = useSyncExternalStore(subscribe, () => getSnapshot().projects);
  const gateProject = pickedProject ?? (projects.length === 1 ? projects[0].id : null);
  // What this machine holds, drawn over the catalog by name. A separate, local read: the catalog says what
  // exists, this says what is here, and an unreachable catalog must not hide an installed plugin.
  const { installs } = usePluginInstalls(gateProject);

  const categories = useMemo(() => pluginCategories(catalog.entries), [catalog.entries]);
  const shown = useMemo(
    () => sortPlugins(filterPlugins(catalog.entries, { q: search, category, os, layer }), sort),
    [catalog.entries, search, category, os, layer, sort],
  );
  // Narrowing or reordering returns to the first page — page 7 of the old result set says nothing
  // about the new one.
  const pager = usePager(shown, `${search}|${category}|${os}|${layer}|${sort}`);
  const unreachable = unreachableSources(catalog);
  // An entry that left the catalog while its detail was open closes it, rather than drawing a plugin
  // the merge no longer offers.
  const open = catalog.entries.find((e) => e.name === openName) ?? null;

  return (
    <>
      <div className="filterbar">
        <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>🧩 {t("plugins.market")}</span>
        <input
          className="board__search"
          type="search"
          placeholder={t("plugins.searchPh")}
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          style={{ fontSize: "var(--fs-xs)", width: 180 }}
        />
        <label style={{ fontSize: "var(--fs-xs)" }}>
          {t("plugins.category")}{" "}
          <select value={category} onChange={(e) => setCategory(e.target.value)}>
            <option value="">{t("plugins.anyCategory")}</option>
            {categories.map((c) => (
              <option key={c} value={c}>{c}</option>
            ))}
          </select>
        </label>
        <label style={{ fontSize: "var(--fs-xs)" }}>
          {t("plugins.os")}{" "}
          <select value={os} onChange={(e) => setOs(e.target.value)}>
            <option value="">{t("plugins.anyOs")}</option>
            {OS_CHOICES.map((o) => (
              <option key={o} value={o}>{t(`plugins.os.${o}`)}</option>
            ))}
          </select>
        </label>
        <label style={{ fontSize: "var(--fs-xs)" }}>
          {t("plugins.layer")}{" "}
          <select value={layer} onChange={(e) => setLayer(e.target.value as PluginLayer | "")}>
            <option value="">{t("plugins.anyLayer")}</option>
            {LAYER_CHOICES.map((l) => (
              <option key={l} value={l}>{t(`plugins.layer.${l}`)}</option>
            ))}
          </select>
        </label>
        <label style={{ fontSize: "var(--fs-xs)" }}>
          {t("plugins.sort")}{" "}
          <select value={sort} onChange={(e) => setSort(e.target.value as PluginSort)}>
            {SORT_CHOICES.map((s) => (
              <option key={s} value={s}>{t(`plugins.sort.${s}`)}</option>
            ))}
          </select>
        </label>
        <span className="topbar__spacer" style={{ flex: 1 }} />
        <button className="feed__action" onClick={() => setSourcesOpen((v) => !v)}>
          {tf("plugins.sources", { count: catalog.sources.length })} {sourcesOpen ? "⌄" : "›"}
        </button>
        <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>
          {tf("plugins.count", { shown: shown.length, total: catalog.entries.length })}
        </span>
      </div>

      {sourcesOpen && <CatalogSources catalog={catalog} />}

      <div style={{ padding: 12, overflowY: "auto" }}>
        {error != null && (
          <div style={{ color: "var(--c-warn)", padding: "var(--s-2) 0" }}>{t("plugins.error")}</div>
        )}
        {unreachable.length > 0 && (
          <div style={{ color: "var(--c-warn)", padding: "var(--s-2) 0" }}>
            {tf("plugins.unreachable", { count: unreachable.length })}
            <div className="faint" style={{ fontSize: "var(--fs-xs)" }}>{unreachable.join(" / ")}</div>
          </div>
        )}
        {/* A catalog that sheds entries must not do it silently: what the door refused is missing from
            the list, and only saying so tells a short list from a complete one. */}
        {catalog.dropped > 0 && (
          <div className="faint" style={{ fontSize: "var(--fs-xs)", padding: "var(--s-2) 0" }}>
            {tf("plugins.dropped", { count: catalog.dropped })}
          </div>
        )}
        {loading && catalog.entries.length === 0 && (
          <div style={{ color: "var(--c-muted)", padding: 16 }}>{t("plugins.loading")}</div>
        )}
        {!loading && catalog.entries.length === 0 && (
          <div style={{ color: "var(--c-muted)", padding: 16 }}>{t("plugins.emptyCatalog")}</div>
        )}
        {catalog.entries.length > 0 && shown.length === 0 && (
          <div style={{ color: "var(--c-muted)", padding: 16 }}>{t("plugins.emptyFilter")}</div>
        )}
        {pager.pageItems.map((e) => (
          <PluginCard
            key={e.name}
            entry={e}
            install={installOf(installs, e.name)}
            onOpen={() => setOpenName(e.name)}
          />
        ))}
        <Pager
          page={pager.page}
          pageCount={pager.pageCount}
          total={pager.total}
          start={pager.start}
          pageSize={pager.pageSize}
          onPage={pager.setPage}
        />
      </div>

      {open && (
        <PluginDetail
          entry={open}
          install={installOf(installs, open.name)}
          projects={projects}
          projectId={gateProject}
          onProject={setPickedProject}
          onClose={() => setOpenName(null)}
        />
      )}
    </>
  );
}

/**
 * The catalogs the list is merged from, and the face for adding or removing one — the CLI's
 * `plugin catalog add/remove/list` in the GUI.
 *
 * Registering a catalog widens what a user *sees*, and nothing else: an asset is still trusted only
 * by amenbo's own catalog key (`AMB-D-371`), so nothing here loosens what may be installed. The
 * official catalog is always merged first and cannot be removed, which is why its row has no button.
 */
function CatalogSources({ catalog }: { catalog: PluginCatalog }) {
  const [url, setUrl] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const run = async (op: () => Promise<boolean>) => {
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

  const add = () => {
    const target = url.trim();
    if (!target) return;
    void run(() => addCatalogSource(target).then((added) => { setUrl(""); return added; }));
  };

  return (
    <div className="catsrc">
      {catalog.sources.map((s) => (
        <div className="catsrc__row" key={s.url}>
          <span className="chip">{t(s.official ? "plugins.layer.official" : "plugins.layer.third-party")}</span>
          <span className="catsrc__url">{s.url}</span>
          <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>
            {s.reachable ? tf("plugins.offered", { count: s.offered }) : t("plugins.sourceDown")}
          </span>
          {!s.official && (
            <button className="feed__action" disabled={busy} onClick={() => void run(() => removeCatalogSource(s.url))}>
              {t("plugins.removeSource")}
            </button>
          )}
        </div>
      ))}
      <div className="catsrc__row">
        <input
          className="board__search"
          type="url"
          placeholder={t("plugins.sourcePh")}
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          onKeyDown={(e) => { if (e.key === "Enter") add(); }}
          style={{ fontSize: "var(--fs-xs)", flex: 1, minWidth: 0 }}
        />
        <button className="btn" disabled={busy || !url.trim()} onClick={add}>{t("plugins.addSource")}</button>
      </div>
      {error && <div style={{ color: "var(--c-warn)", fontSize: "var(--fs-xs)" }}>{error}</div>}
      <div className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("plugins.sourcesNote")}</div>
    </div>
  );
}

/**
 * One entry, drawn from the catalog alone — every field here is in the list document (`AMB-D-385`).
 *
 * Opening it is what costs a request: the detail asks GitHub about this one repository (`AMB-D-347`),
 * which is why the row itself is a button rather than something that loads on sight.
 */
function PluginCard({ entry, install, onOpen }: {
  entry: PluginEntry;
  /** This machine's row for it, when it holds one — what turns a catalog row into a state. */
  install?: PluginInstall;
  onOpen: () => void;
}) {
  // One badge, not two: the layers nest, so an official plugin wearing both would only invite the
  // reading that "official" and "listed" are a scale of the same thing rather than who wrote it and
  // who reviewed it.
  const layer = pluginLayer(entry);
  return (
    <div
      className="feed__item plugcard"
      role="button"
      tabIndex={0}
      onClick={onOpen}
      onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); onOpen(); } }}
    >
      <div className="feed__body" style={{ minWidth: 0 }}>
        <div className="feed__line">
          <strong>{entry.name}</strong>{" "}
          <span className={`chip ${layer === "official" ? "chip--official" : ""}`}>
            {t(`plugins.layer.${layer}`)}
          </span>
          {/* Installed and enabled are two facts (`AMB-D-351`), and the row says which one it is: a plugin
              that is here but fires nothing is the ordinary state, not a half-finished install. */}
          {install && (
            <>
              {" "}
              <span className="chip">
                {install.enabled ? t("plugins.enabledChip") : t("plugins.installed")}
              </span>
            </>
          )}
          {entry.addedAt && (
            <span className="faint" style={{ fontSize: "var(--fs-xs)" }}> {tf("plugins.added", { date: entry.addedAt.slice(0, 10) })}</span>
          )}
        </div>
        <div className="muted" style={{ fontSize: "var(--fs-sm)" }}>{entry.desc}</div>
        <div className="faint" style={{ fontSize: "var(--fs-xs)", display: "flex", gap: "var(--s-2)", flexWrap: "wrap", marginTop: 2 }}>
          <span>{entry.author}</span>
          <span>·</span>
          <span>{entry.category}</span>
          <span>·</span>
          <span>{entry.os.map((o) => t(`plugins.os.${o}`)).join(" / ")}</span>
          <span>·</span>
          <span>{entry.repo}</span>
        </div>
      </div>
    </div>
  );
}
