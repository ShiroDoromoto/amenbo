import { useEffect, useMemo, useState } from "react";
import { Pager, usePager } from "../components/Pager";
import { errText, t, tn, tf } from "../core/i18n";
import {
  addCatalogSource, filterPlugins, pluginCategories, pluginLayer, pluginLayerLabel, probeCatalogSource,
  removeCatalogSource, sortPlugins, unreachableSources, usePluginCatalog,
  type PluginCatalog, type PluginCatalogProbe, type PluginEntry, type PluginLayer, type PluginSort,
} from "../core/pluginCatalog";
import { firesAnywhere, installOf, usePluginInstalls, type PluginInstall } from "../core/pluginInstalls";
import { pluginDesc } from "../core/pluginText";
import { refreshPluginUpdates } from "../core/pluginUpdates";
import { PluginDetail } from "./PluginDetail";
import { asTyped } from "../core/keys";

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

/**
 * The source filter holds one string: "" for any, a layer's name, or a registered catalog's URL
 * (`AMB-D-389`). One control rather than two, because they are one question — "where is this from?" —
 * asked at two grains, and a reader narrowing to one shelf is not also thinking about layers.
 */
function isLayerChoice(v: string): v is PluginLayer {
  return (LAYER_CHOICES as string[]).includes(v);
}

/** The orderings on offer. "Popular" is not among them: stars are fetched for one opened entry, never for a list. */
const SORT_CHOICES: PluginSort[] = ["featured", "new", "name"];

export function PluginMarketScreen({ onOpenInstalled }: {
  /** Go to the installed screen — where a plugin that just landed is turned on (`AMB-D-412`). */
  onOpenInstalled: () => void;
}) {
  const { catalog, loading, error } = usePluginCatalog();
  const [search, setSearch] = useState("");
  const [category, setCategory] = useState("");
  const [os, setOs] = useState("");
  // Either a layer or one catalog — see `isLayerChoice`.
  const [origin, setOrigin] = useState("");
  // Recommended first, which is the view `AMB-D-347` leads discovery with. On a catalog that has
  // curated nothing it is exactly the "new" ordering, so the default costs nothing before there is
  // anything to recommend.
  const [sort, setSort] = useState<PluginSort>("featured");
  const [sourcesOpen, setSourcesOpen] = useState(false);
  // The opened entry is held by name, not as a row: the catalog can be refetched underneath, and a
  // detail must then show what the catalog now holds rather than a copy frozen at the click.
  const [openName, setOpenName] = useState<string | null>(null);
  // What this machine holds, drawn over the catalog by name. A separate, local read: the catalog says what
  // exists, this says what is here, and an unreachable catalog must not hide an installed plugin.
  const { installs } = usePluginInstalls();
  // Opening a plugin screen is one of the update triggers (`AMB-D-359`). The offer is the shell's banner, so
  // nothing is drawn here for it — this only asks. Free inside the catalog's freshness window.
  useEffect(() => { refreshPluginUpdates("incidental"); }, []);

  const categories = useMemo(() => pluginCategories(catalog.entries), [catalog.entries]);
  // The catalogs the filter can name one by one: the registered ones, in registration order. The
  // official catalog is not among them — it is the `official` / `listed` choices already.
  const registered = catalog.sources.filter((s) => !s.official);
  const shown = useMemo(
    () => sortPlugins(
      filterPlugins(catalog.entries, {
        q: search,
        category,
        os,
        layer: isLayerChoice(origin) ? origin : "",
        source: isLayerChoice(origin) ? "" : origin,
      }),
      sort,
    ),
    [catalog.entries, search, category, os, origin, sort],
  );
  // Narrowing or reordering returns to the first page — page 7 of the old result set says nothing
  // about the new one.
  const pager = usePager(shown, `${search}|${category}|${os}|${origin}|${sort}`);
  const unreachable = unreachableSources(catalog);
  // An entry that left the catalog while its detail was open closes it, rather than drawing a plugin
  // the merge no longer offers.
  const open = catalog.entries.find((e) => e.name === openName) ?? null;

  return (
    <>
      <div className="filterbar">
        <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>🧩 {t("plugins.market")}</span>
        <input
          {...asTyped}
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
          <select value={origin} onChange={(e) => setOrigin(e.target.value)}>
            <option value="">{t("plugins.anyLayer")}</option>
            {LAYER_CHOICES.map((l) => (
              <option key={l} value={l}>{t(`plugins.layer.${l}`)}</option>
            ))}
            {/* "Only the in-house catalog" in one click (`AMB-D-389`). The list itself stays mixed —
                splitting it is the reader's move to make here, not the screen's default. */}
            {registered.map((s) => (
              <option key={s.url} value={s.url}>{s.name}</option>
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
            {tn("plugins.unreachable", unreachable.length)}
            <div className="faint" style={{ fontSize: "var(--fs-xs)" }}>{unreachable.join(" / ")}</div>
          </div>
        )}
        {/* A catalog that sheds entries must not do it silently: what the door refused is missing from
            the list, and only saying so tells a short list from a complete one. */}
        {catalog.dropped > 0 && (
          <div className="faint" style={{ fontSize: "var(--fs-xs)", padding: "var(--s-2) 0" }}>
            {tn("plugins.dropped", catalog.dropped)}
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
          onOpenInstalled={onOpenInstalled}
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
 * **Registering is a trust decision, not a bookmark** (`AMB-D-389`): a plugin installed from a
 * registered catalog is verified against *that* catalog's key, so adding one adds a trust root. The
 * URL is therefore not registered on the button — it is probed first, and what the probe found (the
 * fingerprint that would be pinned, under the name it will be called) is put in front of the user to
 * agree to. The official catalog is always merged first and cannot be removed, which is why its row
 * has no button.
 */
function CatalogSources({ catalog }: { catalog: PluginCatalog }) {
  const [url, setUrl] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // What registering the typed URL would mean, once asked — the material on screen while the user
  // decides. Null is "nothing has been asked yet"; nothing is written until this is agreed to.
  const [probe, setProbe] = useState<PluginCatalogProbe | null>(null);
  const [name, setName] = useState("");

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

  // Step one: ask what this URL would mean. A refusal (not http(s), the official catalog's own URL, a
  // key that changed since it was pinned) lands here, before anything is registered.
  const check = () => {
    const target = url.trim();
    if (!target) return;
    void run(async () => {
      const found = await probeCatalogSource(target);
      setProbe(found);
      setName(found?.suggestedName ?? "");
    });
  };

  // Step two: register what was shown, on the fingerprint that was shown. The door refuses anything
  // else, so a key that moved between the two calls stops here rather than being pinned unseen.
  const confirm = () => {
    if (!probe) return;
    void run(async () => {
      await addCatalogSource(probe.url, { name, agreedFingerprint: probe.fingerprint ?? undefined });
      setProbe(null);
      setUrl("");
      setName("");
    });
  };

  const cancel = () => {
    setProbe(null);
    setError(null);
  };

  return (
    <div className="catsrc">
      {catalog.sources.map((s) => (
        <div className="catsrc__row" key={s.url}>
          <span className="chip">{t(s.official ? "plugins.layer.official" : "plugins.layer.third-party")}</span>
          <span className="catsrc__name">{s.name}</span>
          <span className="catsrc__url">{s.url}</span>
          {/* The key its plugins are trusted on (`AMB-D-389`), on every row — the one with nothing to
              show is the one worth noticing, because nothing from it can be installed. */}
          <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>
            {s.fingerprint ? tf("plugins.sourceKey", { fp: s.fingerprint }) : t("plugins.sourceNoKey")}
          </span>
          <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>
            {s.reachable ? tn("plugins.offered", s.offered) : t("plugins.sourceDown")}
          </span>
          {!s.official && (
            <button className="feed__action" disabled={busy} onClick={() => void run(() => removeCatalogSource(s.url))}>
              {t("plugins.removeSource")}
            </button>
          )}
        </div>
      ))}
      {probe ? (
        <SourceConsent
          probe={probe}
          name={name}
          onName={setName}
          busy={busy}
          onConfirm={confirm}
          onCancel={cancel}
        />
      ) : (
        <div className="catsrc__row">
          <input
            {...asTyped}
            className="board__search"
            type="url"
            placeholder={t("plugins.sourcePh")}
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter") check(); }}
            style={{ fontSize: "var(--fs-xs)", flex: 1, minWidth: 0 }}
          />
          <button className="btn" disabled={busy || !url.trim()} onClick={check}>
            {busy ? t("plugins.sourceChecking") : t("plugins.addSource")}
          </button>
        </div>
      )}
      {error && <div style={{ color: "var(--c-warn)", fontSize: "var(--fs-xs)" }}>{error}</div>}
      <div className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("plugins.sourcesNote")}</div>
    </div>
  );
}

/**
 * What the user agrees to when they register a catalog (`AMB-D-389`): the fingerprint of the key its
 * plugins will be verified against, and the name it will be called by.
 *
 * The fingerprint is the whole point of asking — trust-on-first-use rests on the person having seen
 * the key they are trusting, so it is shown in full rather than summarised, and the button says what
 * pressing it does. A catalog that publishes no key asks for no trust: it can be browsed, nothing on
 * it installs, and the panel says so instead of pretending there is something to compare.
 */
function SourceConsent({ probe, name, onName, busy, onConfirm, onCancel }: {
  probe: PluginCatalogProbe;
  name: string;
  onName: (v: string) => void;
  busy: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  return (
    <div className="catsrc__consent">
      <div style={{ fontSize: "var(--fs-sm)" }}>{tf("plugins.trustTitle", { url: probe.url })}</div>
      {probe.fingerprint ? (
        <>
          <div className="catsrc__fp">
            <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("plugins.fingerprint")}</span>{" "}
            <strong>{probe.fingerprint}</strong>
          </div>
          <div className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("plugins.trustNote")}</div>
          <div className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("plugins.keyChangeNote")}</div>
        </>
      ) : (
        <div className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("plugins.noKeyNote")}</div>
      )}
      {probe.registered && (
        <div className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("plugins.alreadyRegistered")}</div>
      )}
      <div className="catsrc__row">
        <label style={{ fontSize: "var(--fs-xs)", flex: 1, minWidth: 0, display: "flex", gap: "var(--s-2)", alignItems: "center" }}>
          {t("plugins.sourceName")}
          <input
            {...asTyped}
            className="board__search"
            type="text"
            value={name}
            placeholder={probe.suggestedName}
            onChange={(e) => onName(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter") onConfirm(); }}
            style={{ fontSize: "var(--fs-xs)", flex: 1, minWidth: 0 }}
          />
        </label>
        <button className="feed__action" disabled={busy} onClick={onCancel}>{t("plugins.sourceCancel")}</button>
        <button className="btn" disabled={busy} onClick={onConfirm}>
          {t(probe.fingerprint ? "plugins.trustAndAdd" : "plugins.addSource")}
        </button>
      </div>
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
  // who reviewed it. On the free layer it is the serving catalog's name (`AMB-D-389`).
  //
  // The recommendation badge below sits beside it because it is not on that ladder at all: it says the
  // index recommends the plugin, which a listed third-party plugin can be and an official one can lack.
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
            {pluginLayerLabel(entry)}
          </span>
          {/* No star: a star is the popularity figure this list deliberately never asks GitHub for
              (`AMB-D-347`), and wearing one here would read as exactly that number. */}
          {entry.featured && (
            <>
              {" "}
              <span className="chip chip--featured">{t("plugins.featured")}</span>
            </>
          )}
          {/* Installed and enabled are two facts (`AMB-D-351`), and the row says which one it is: a plugin
              that is here but fires nothing is the ordinary state, not a half-finished install. The card
              is one line about the plugin, so "enabled" here means it fires in some project
              (`AMB-D-412`) — which ones is the detail's to name. */}
          {install && (
            <>
              {" "}
              <span className="chip">
                {firesAnywhere(install) ? t("plugins.enabledChip") : t("plugins.installed")}
              </span>
            </>
          )}
          {entry.addedAt && (
            <span className="faint" style={{ fontSize: "var(--fs-xs)" }}> {tf("plugins.added", { date: entry.addedAt.slice(0, 10) })}</span>
          )}
        </div>
        {/* The author's line in the reader's language where the catalog published one (`AMB-D-623`),
            and the author's own where it did not — with nothing on the row to say which it is. */}
        <div className="muted" style={{ fontSize: "var(--fs-sm)" }}>{pluginDesc(entry)}</div>
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
