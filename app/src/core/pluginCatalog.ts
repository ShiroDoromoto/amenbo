// The plugin market's data seam: one fetch of the merged catalog, then everything else locally.
//
// `AMB-D-347` fixes the shape of discovery — the catalog is a static file the app pulls **once**, and
// searching, filtering and paging happen over the copy in hand. Nothing here goes back to the
// network per keystroke, and nothing asks GitHub about an entry that is merely listed (stars,
// README and download counts are the detail view's, lazily, for the one entry a user opened).
// So the filtering below is deliberately plain client-side work over an array, not a query.
import { useSyncExternalStore } from "react";
import { currentLang, t } from "./i18n";
import { invoke } from "./ipc";
import { inTauri, subscribe } from "./snapshot";
import { pluginDesc } from "./pluginText";
import { invalidateQueries, useQuery } from "./query";
import type {
  PluginCatalogDto,
  PluginCatalogProbeDto,
  PluginDetailDto,
  PluginEntryDto,
  PluginRepoFactsDto,
} from "../bindings/bindings";

/** One catalog entry as the market list draws it (generated DTO). */
export type PluginEntry = PluginEntryDto;
/** What the catalog's detail document says about one plugin (generated DTO). */
export type PluginDetail = PluginDetailDto;
/** The merged catalog: the entries, plus which catalogs answered (generated DTO). */
export type PluginCatalog = PluginCatalogDto;

const EMPTY_CATALOG: PluginCatalog = { entries: [], sources: [], dropped: 0 };

/**
 * Fetch the merged catalog in one language (Tauri: `plugin_catalog_browse`). The language travels with
 * the ask because the translated lines are a document per language (`AMB-D-622`), and what comes back
 * carries both halves for the front end to pick from (`AMB-D-623`).
 *
 * Outside Tauri — `npm run dev` in a browser — there is nothing to merge: the catalogs are read through
 * core, from this machine's registry cache, so the browser mock has no honest answer and returns an
 * empty one. The same call the command reference makes.
 */
export async function fetchPluginCatalog(lang: string): Promise<PluginCatalog> {
  if (inTauri()) return invoke<PluginCatalog>("plugin_catalog_browse", { lang });
  return EMPTY_CATALOG;
}

/**
 * Read the merged catalog for the market screen. Core answers a re-open inside the freshness window
 * from its cache, so remounting the screen is not another fetch.
 *
 * **The language is part of the key**, so changing it re-reads the list in the new one rather than
 * leaving the rows in the language they were drawn in. Core reads each language's document the same
 * incidental way, so going back to one already looked at costs no request.
 */
export function usePluginCatalog(): { catalog: PluginCatalog; loading: boolean; error: unknown } {
  const lang = useSyncExternalStore(subscribe, currentLang);
  const { data, loading, error } = useQuery<PluginCatalog>(["plugin-catalog", lang], () =>
    fetchPluginCatalog(lang),
  );
  return { catalog: data ?? EMPTY_CATALOG, loading, error };
}

/**
 * Which trust layer an entry sits in (`AMB-D-347`). Two independent facts fold into one ladder here,
 * because they nest: official (the amenbo team wrote it) is always listed as well, and an entry a
 * third-party catalog offers is neither.
 */
export type PluginLayer = "official" | "listed" | "third-party";

/** The layer one entry belongs to — the badge it wears, and what the layer filter matches. */
export function pluginLayer(e: PluginEntry): PluginLayer {
  if (e.official) return "official";
  return e.listed ? "listed" : "third-party";
}

/**
 * What the badge on a row says. The two reviewed layers are named by the layer itself; the free layer is
 * named by **the catalog that served it** (`AMB-D-389`).
 *
 * A registered catalog is a trust root the user chose and named — what installs from it is verified on
 * its key — so the shelf's name is the most informative thing a row can carry there, and "third-party"
 * is left to the filter, where a generic choice is what the vocabulary needs.
 */
export function pluginLayerLabel(e: PluginEntry): string {
  const layer = pluginLayer(e);
  return layer === "third-party" ? e.sourceName : t(`plugins.layer.${layer}`);
}

/** What the market list is narrowed by. Every field is optional; an unset one narrows nothing. */
export interface PluginFilter {
  /**
   * Free text, matched case-insensitively against the name, the author and **both** descriptions — the
   * line the reader is being shown and the one the author wrote (`AMB-D-623`). Searching only the shown
   * one would lose an English word someone remembers from the repository; searching only the base one
   * would lose the words actually on their screen.
   */
  q?: string;
  /** Exactly one category, or "" for every category. */
  category?: string;
  /** Only entries that support this OS (`macos` / `windows` / `linux`), or "" for any. */
  os?: string;
  /**
   * Only entries at or above this layer, or "" for every layer: `official` is the amenbo team's own,
   * `listed` also admits what review put on the official index, `third-party` narrows to what only a
   * registered third-party catalog offers.
   */
  layer?: PluginLayer | "";
  /**
   * Only entries one named catalog served, by its URL, or "" for every catalog (`AMB-D-389`). The URL
   * and not the name, because the name is the user's and two catalogs may carry the same one.
   *
   * The narrower half of `layer`: that says which shelf a plugin sits on, this says which shelf.
   */
  source?: string;
}

/**
 * Narrow the entries to what the filter names. Pure, and over the whole list: the screen pages the
 * result, so filtering has to see every entry, not just the page on screen.
 */
export function filterPlugins(entries: PluginEntry[], f: PluginFilter): PluginEntry[] {
  const q = (f.q ?? "").trim().toLowerCase();
  return entries.filter((e) => {
    // "listed" is a floor, not an exact match: official entries are listed too, and hiding them from a
    // reader who asked for reviewed plugins would be a lie about the catalog.
    if (f.layer === "official" && !e.official) return false;
    if (f.layer === "listed" && !e.listed) return false;
    if (f.layer === "third-party" && e.listed) return false;
    if (f.source && e.source !== f.source) return false;
    if (f.category && e.category !== f.category) return false;
    if (f.os && !e.os.includes(f.os)) return false;
    if (!q) return true;
    return (
      e.name.toLowerCase().includes(q) ||
      e.desc.toLowerCase().includes(q) ||
      pluginDesc(e).toLowerCase().includes(q) ||
      e.author.toLowerCase().includes(q)
    );
  });
}

/** How the market list is ordered. */
export type PluginSort = "featured" | "new" | "name";

/** The "new" ordering, which "featured" falls back to within each half. */
function byNewest(a: PluginEntry, b: PluginEntry): number {
  if (!a.addedAt && !b.addedAt) return 0;
  if (!a.addedAt) return 1;
  if (!b.addedAt) return -1;
  return b.addedAt.localeCompare(a.addedAt);
}

/**
 * Order the entries. Sorting is the client's, over the list already in hand — the same reason the
 * filtering is (`AMB-D-347`).
 *
 * "New" reads `addedAt`, which only the catalog's CI can know (a client holds no git history of the
 * catalog repository). An entry without one is not old, it is unknown, so it sinks below the dated
 * ones rather than sorting as the epoch. Ties keep catalog order, which puts the official catalog
 * first.
 *
 * "Featured" reads the official index's hand curation, which is a flag and not a rank: it says which
 * plugins are recommended, never in what order among themselves. So it lifts them as a block and
 * orders within it by the same "new" rule, rather than inventing a ranking the catalog never made.
 * The rest of the list follows unfiltered — a recommendation is a way in, not a way of hiding the
 * plugins nobody got round to recommending.
 */
export function sortPlugins(entries: PluginEntry[], sort: PluginSort): PluginEntry[] {
  const out = [...entries];
  if (sort === "name") {
    out.sort((a, b) => a.name.localeCompare(b.name));
    return out;
  }
  if (sort === "featured") {
    out.sort((a, b) => Number(b.featured) - Number(a.featured) || byNewest(a, b));
    return out;
  }
  out.sort(byNewest);
  return out;
}

/**
 * The categories present in the catalog, sorted, for the category selector. The vocabulary is the
 * catalog's — a manifest's `category` is a free label (`AMB-D-347`), so the choices are read off the
 * entries rather than declared here.
 */
export function pluginCategories(entries: PluginEntry[]): string[] {
  return [...new Set(entries.map((e) => e.category).filter(Boolean))].sort();
}

/**
 * The catalogs that could not be reached — neither the network nor a cache answered, so they
 * contributed nothing to the list. Shown so a short list is not mistaken for the whole catalog.
 */
export function unreachableSources(catalog: PluginCatalog): string[] {
  return catalog.sources.filter((s) => !s.reachable).map((s) => s.url);
}

/** Refetch the merged catalog — after registering or unregistering a source changed what it holds. */
function reloadCatalog(): void {
  invalidateQueries((key) => key[0] === "plugin-catalog");
}

/** What registering a catalog would mean, before anything is written (generated DTO). */
export type PluginCatalogProbe = PluginCatalogProbeDto;

/**
 * Ask what registering `url` would mean, without registering it — the fingerprint the consent screen
 * shows, the name it suggests, and whether going ahead pins a key (`AMB-D-389`). A URL core refuses —
 * not `http(s)`, the official catalog's own, or one whose key document is not a key — throws, as does
 * a catalog that now publishes a different key than the one already pinned.
 *
 * Outside Tauri there is no registry to probe, so the browser mock has no honest answer.
 */
export async function probeCatalogSource(url: string): Promise<PluginCatalogProbe | null> {
  if (!inTauri()) return null;
  return invoke<PluginCatalogProbe>("plugin_catalog_probe_source", { url });
}

/**
 * Register a third-party catalog under `name`, pinning the key whose fingerprint the user agreed to.
 * `false` means it was already registered exactly like this (idempotent, not a failure).
 *
 * Registering is a trust decision, not a bookmark (`AMB-D-389`): a plugin installed from this catalog
 * is verified against **its** key rather than the one amenbo ships. So `agreedFingerprint` is what the
 * consent screen showed, and the door refuses to pin anything else — including the case where the
 * catalog started publishing a different key between the screen and the button.
 */
export async function addCatalogSource(
  url: string,
  opts: { name?: string; agreedFingerprint?: string } = {},
): Promise<boolean> {
  if (!inTauri()) return false;
  const added = await invoke<boolean>("plugin_catalog_add_source", {
    url,
    name: opts.name ?? null,
    agreedFingerprint: opts.agreedFingerprint ?? null,
  });
  reloadCatalog();
  return added;
}

/**
 * Unregister a third-party catalog and drop its cached copy. `false` means it was not registered.
 * A plugin installed from it stays installed: the catalog is where it was found, not what runs it.
 */
export async function removeCatalogSource(url: string): Promise<boolean> {
  if (!inTauri()) return false;
  const removed = await invoke<boolean>("plugin_catalog_remove_source", { url });
  reloadCatalog();
  return removed;
}

// ---- the one entry a user opened ----
//
// The other half of `AMB-D-347`'s discovery shape, and the **only** place the market talks to GitHub:
// stars, downloads and a README are per-repository, so asking for them anywhere but a detail would be
// exactly the "one request per plugin" the catalog exists to avoid. Everything here is keyed on the
// repository and fetched when a detail opens — never for a row that is merely listed.
//
// The detail file `plugins/<name>.json` (`AMB-D-385`) is fetched from this same seam when the install
// side lands: one opened entry, one place that goes and gets what only opening it justifies.

/** What GitHub could tell us about one plugin's repository (generated DTO). */
export type PluginRepoFacts = PluginRepoFactsDto;

/** Where a repository lives, from the `owner/name` a catalog entry carries. */
export function repoUrl(repo: string): string {
  return `https://github.com/${repo}`;
}

/**
 * The place a README's relative paths are read against — its own repository, at the branch GitHub
 * serves it from. `HEAD` is GitHub's name for whatever that branch is called, so the base is known
 * from the coordinates alone and nothing has to be fetched to learn it.
 */
export function repoLinkBase(repo: string): string {
  return `${repoUrl(repo)}/blob/HEAD/`;
}

/**
 * Read one repository's figures (Tauri: `plugin_repo_facts`). Core caches them per repository well
 * past the hour, because GitHub's unauthenticated rate limit — not freshness — is what bounds this.
 * Outside Tauri there is no core to ask, so the browser mock reports nothing rather than inventing a
 * star count.
 *
 * `readme` is the caller saying whether it would draw one (`AMB-D-638`): a plugin that describes itself
 * costs a request fewer, and what is not asked for is absent from the answer.
 */
export async function fetchPluginRepoFacts(
  repo: string,
  readme: boolean,
): Promise<PluginRepoFacts> {
  if (inTauri()) return invoke<PluginRepoFacts>("plugin_repo_facts", { repo, readme });
  return { rateLimited: false };
}

/**
 * The opened entry's GitHub figures. Call it only from a detail that is actually open: a hook fetches
 * when it mounts, so what keeps this request tied to opening one entry is that nothing on the list
 * side mounts it.
 *
 * `readme` is `"unknown"` while the caller cannot say yet — the catalog's detail document is what
 * answers whether this plugin describes itself, and it arrives on its own schedule. Nothing is asked of
 * GitHub until it does, and the wait reads as loading rather than as an answer: a detail that drew "no
 * README" for the moment before would be reporting a fetch that had not happened.
 */
export function usePluginRepoFacts(
  repo: string,
  readme: boolean | "unknown",
): { facts: PluginRepoFacts | undefined; loading: boolean; error: unknown } {
  const waiting = readme === "unknown";
  const { data, loading, error } = useQuery<PluginRepoFacts>(
    ["plugin-repo-facts", repo, String(readme)],
    () => (waiting ? Promise.resolve({ rateLimited: false }) : fetchPluginRepoFacts(repo, readme)),
  );
  if (waiting) return { facts: undefined, loading: true, error: undefined };
  return { facts: data, loading, error };
}

/**
 * Read the catalog's detail document for one plugin (Tauri: `plugin_detail`, `AMB-D-385`). The list
 * carries what a row draws; this carries what installing one would mean — the switch it gets, what it
 * watches, what it will ask to be told, and whether this build can run it.
 *
 * It is read from whichever catalog served the row (`AMB-D-389`), so a registered catalog's plugin says
 * what installing it would mean before it is installed — the same merged view the list drew. `null` is an
 * answer, not a failure: a name no catalog carries has no detail to read. Outside Tauri there is no
 * catalog at all, so the browser mock says the same.
 *
 * `lang` is what the form labels come back in (`AMB-D-623`). It costs nothing to ask for: the detail
 * document carries every language at once (`AMB-D-622`), so the language is picked out of one fetch.
 */
export async function fetchPluginDetail(
  name: string,
  lang: string,
): Promise<PluginDetail | null> {
  if (inTauri()) return invoke<PluginDetail | null>("plugin_detail", { name, lang });
  return null;
}

/**
 * The opened entry's detail. Mounted by the detail view alone — the same rule the figures above follow,
 * and the whole reason browsing a catalog of thousands stays one static file. Keyed by language too, so a
 * reader who changes it while a detail is open sees the form it describes follow them.
 */
export function usePluginDetail(
  name: string,
): { detail: PluginDetail | null | undefined; loading: boolean; error: unknown } {
  const lang = useSyncExternalStore(subscribe, currentLang);
  const { data, loading, error } = useQuery<PluginDetail | null>(
    ["plugin-detail", name, lang],
    () => fetchPluginDetail(name, lang),
  );
  return { detail: data, loading, error };
}
