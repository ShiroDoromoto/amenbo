// The plugin market's data seam: one fetch of the merged catalog, then everything else locally.
//
// `AMB-D-347` fixes the shape of discovery — the catalog is a static file the app pulls **once**, and
// searching, filtering and paging happen over the copy in hand. Nothing here goes back to the
// network per keystroke, and nothing asks GitHub about an entry that is merely listed (stars,
// README and download counts are the detail view's, lazily, for the one entry a user opened).
// So the filtering below is deliberately plain client-side work over an array, not a query.
import { invoke } from "./ipc";
import { inTauri } from "./snapshot";
import { invalidateQueries, useQuery } from "./query";
import type { PluginCatalogDto, PluginEntryDto, PluginRepoFactsDto } from "../bindings/bindings";

/** One catalog entry as the market list draws it (generated DTO). */
export type PluginEntry = PluginEntryDto;
/** The merged catalog: the entries, plus which catalogs answered (generated DTO). */
export type PluginCatalog = PluginCatalogDto;

const EMPTY_CATALOG: PluginCatalog = { entries: [], sources: [], dropped: 0 };

/**
 * Fetch the merged catalog (Tauri: `plugin_catalog_browse`). Outside Tauri — `npm run dev` in a
 * browser — there is nothing to merge: the catalogs are read through core, from this machine's
 * registry cache, so the browser mock has no honest answer and returns an empty one. The same call
 * the command reference makes.
 */
export async function fetchPluginCatalog(): Promise<PluginCatalog> {
  if (inTauri()) return invoke<PluginCatalog>("plugin_catalog_browse");
  return EMPTY_CATALOG;
}

/**
 * Read the merged catalog for the market screen. Core answers a re-open inside the freshness window
 * from its cache, so remounting the screen is not another fetch.
 */
export function usePluginCatalog(): { catalog: PluginCatalog; loading: boolean; error: unknown } {
  const { data, loading, error } = useQuery<PluginCatalog>(["plugin-catalog"], fetchPluginCatalog);
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

/** What the market list is narrowed by. Every field is optional; an unset one narrows nothing. */
export interface PluginFilter {
  /** Free text, matched case-insensitively against the name, the description and the author. */
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
    if (f.category && e.category !== f.category) return false;
    if (f.os && !e.os.includes(f.os)) return false;
    if (!q) return true;
    return (
      e.name.toLowerCase().includes(q) ||
      e.desc.toLowerCase().includes(q) ||
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

/**
 * Register a third-party catalog. `false` means it was already registered (idempotent, not a
 * failure); a URL core refuses — not `http(s)`, or the official catalog's own — throws.
 *
 * Registering only widens what discovery *shows*. Installing still accepts nothing a third-party
 * catalog signed (`AMB-D-371`), so this is a browsing choice, not a trust one.
 */
export async function addCatalogSource(url: string): Promise<boolean> {
  if (!inTauri()) return false;
  const added = await invoke<boolean>("plugin_catalog_add_source", { url });
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
 */
export async function fetchPluginRepoFacts(repo: string): Promise<PluginRepoFacts> {
  if (inTauri()) return invoke<PluginRepoFacts>("plugin_repo_facts", { repo });
  return { rateLimited: false };
}

/**
 * The opened entry's GitHub figures. Call it only from a detail that is actually open: a hook fetches
 * when it mounts, so what keeps this request tied to opening one entry is that nothing on the list
 * side mounts it.
 */
export function usePluginRepoFacts(
  repo: string,
): { facts: PluginRepoFacts | undefined; loading: boolean; error: unknown } {
  const { data, loading, error } = useQuery<PluginRepoFacts>(["plugin-repo-facts", repo], () =>
    fetchPluginRepoFacts(repo),
  );
  return { facts: data, loading, error };
}
