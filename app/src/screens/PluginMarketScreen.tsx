import { useMemo, useState } from "react";
import { Pager, usePager } from "../components/Pager";
import { t, tf } from "../core/i18n";
import {
  filterPlugins, pluginCategories, unreachableSources, usePluginCatalog, type PluginEntry,
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

export function PluginMarketScreen() {
  const { catalog, loading, error } = usePluginCatalog();
  const [search, setSearch] = useState("");
  const [category, setCategory] = useState("");
  const [os, setOs] = useState("");
  const [officialOnly, setOfficialOnly] = useState(false);

  const categories = useMemo(() => pluginCategories(catalog.entries), [catalog.entries]);
  const shown = useMemo(
    () => filterPlugins(catalog.entries, { q: search, category, os, officialOnly }),
    [catalog.entries, search, category, os, officialOnly],
  );
  // Narrowing returns to the first page — page 7 of the old result set says nothing about the new one.
  const pager = usePager(shown, `${search}|${category}|${os}|${officialOnly}`);
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
          <input
            type="checkbox"
            checked={officialOnly}
            onChange={(e) => setOfficialOnly(e.target.checked)}
          />{" "}
          {t("plugins.officialOnly")}
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
  return (
    <div className="feed__item">
      <div className="feed__body" style={{ minWidth: 0 }}>
        <div className="feed__line">
          <strong>{entry.name}</strong>{" "}
          {entry.official && <span className="chip">{t("plugins.official")}</span>}
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
