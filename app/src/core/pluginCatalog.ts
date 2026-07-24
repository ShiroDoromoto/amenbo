// The plugin market's data seam: one fetch of the merged catalog, then everything else locally.
//
// `AMB-D-347` fixes the shape of discovery — the catalog is a static file the app pulls **once**, and
// searching, filtering and paging happen over the copy in hand. Nothing here goes back to the
// network per keystroke, and nothing asks GitHub about an entry that is merely listed (stars,
// README and download counts are the detail view's, lazily, for the one entry a user opened).
// So the filtering below is deliberately plain client-side work over an array, not a query.
import { invoke } from "./ipc";
import { inTauri } from "./snapshot";
import { useQuery } from "./query";
import type { PluginCatalogDto, PluginEntryDto } from "../bindings/bindings";

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

/** What the market list is narrowed by. Every field is optional; an unset one narrows nothing. */
export interface PluginFilter {
  /** Free text, matched case-insensitively against the name, the description and the author. */
  q?: string;
  /** Exactly one category, or "" for every category. */
  category?: string;
  /** Only entries that support this OS (`macos` / `windows` / `linux`), or "" for any. */
  os?: string;
  /** Only entries the catalog marks official (`AMB-D-347` — the author is the amenbo team). */
  officialOnly?: boolean;
}

/**
 * Narrow the entries to what the filter names. Pure, and over the whole list: the screen pages the
 * result, so filtering has to see every entry, not just the page on screen.
 */
export function filterPlugins(entries: PluginEntry[], f: PluginFilter): PluginEntry[] {
  const q = (f.q ?? "").trim().toLowerCase();
  return entries.filter((e) => {
    if (f.officialOnly && !e.official) return false;
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
