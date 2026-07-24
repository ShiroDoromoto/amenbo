import { useMemo, useState } from "react";
import { Pager, usePager } from "../components/Pager";
import { t, tf } from "../core/i18n";
import {
  filterPlugins, pluginCategories, pluginLayer, sortPlugins, unreachableSources, usePluginCatalog,
  type PluginEntry, type PluginLayer, type PluginSort,
} from "../core/pluginCatalog";

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

  const categories = useMemo(() => pluginCategories(catalog.entries), [catalog.entries]);
  const shown = useMemo(
    () => sortPlugins(filterPlugins(catalog.entries, { q: search, category, os, layer }), sort),
    [catalog.entries, search, category, os, layer, sort],
  );
  // Narrowing or reordering returns to the first page — page 7 of the old result set says nothing
  // about the new one.
  const pager = usePager(shown, `${search}|${category}|${os}|${layer}|${sort}`);
  const unreachable = unreachableSources(catalog);

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
        <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>
          {tf("plugins.count", { shown: shown.length, total: catalog.entries.length })}
        </span>
      </div>

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
          <PluginCard key={e.name} entry={e} />
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
    </>
  );
}

/** One entry, drawn from the catalog alone — every field here is in the list document (`AMB-D-385`). */
function PluginCard({ entry }: { entry: PluginEntry }) {
  // One badge, not two: the layers nest, so an official plugin wearing both would only invite the
  // reading that "official" and "listed" are a scale of the same thing rather than who wrote it and
  // who reviewed it.
  const layer = pluginLayer(entry);
  return (
    <div className="feed__item">
      <div className="feed__body" style={{ minWidth: 0 }}>
        <div className="feed__line">
          <strong>{entry.name}</strong>{" "}
          <span className={`chip ${layer === "official" ? "chip--official" : ""}`}>
            {t(`plugins.layer.${layer}`)}
          </span>
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
