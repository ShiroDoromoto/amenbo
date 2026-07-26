// **Which installed plugins the catalog has moved past** — the detection seam behind the update banner
// (`AMB-D-359`).
//
// Detection is core's and nothing is duplicated here: `plugin_updates` compares what this machine holds
// against the catalog it already fetches, through that catalog's freshness boundary. So the face is free to
// re-ask at the moments that matter — a focus return, opening a plugin screen, an explicit "check now" — and
// amenbo still holds no timer and opens no connection it would not have opened anyway (`AMB-D-331`'s posture,
// applied to plugins).
//
// **An offer, never an application.** Nothing here updates anything on its own: `applyPluginUpdate` runs only
// when a button was pressed, and the gates it passes (signature, checksum, compatibility, the `required`
// re-check) are all core's.
//
// The dismissal is keyed by the **build** offered, not by the plugin, which is what makes a quiet banner stay
// quiet without going silent: dismissing the offer of one build says nothing about the next one, and a plugin
// whose catalog entry moves again surfaces on its own.
import { invoke } from "./ipc";
import { inTauri } from "./snapshot";
import { invalidateQueries, useQuery } from "./query";
import type { PluginUpdateDto, PluginUpdateOutcomeDto } from "../bindings/bindings";

/** One installed plugin the catalog holds a different build of (generated DTO). */
export type PluginUpdate = PluginUpdateDto;
/** How one plugin fared in an "update all" (generated DTO). */
export type PluginUpdateOutcome = PluginUpdateOutcomeDto;

const NONE: PluginUpdate[] = [];
const KEY = "amenbo.pluginUpdatesDismissed";

/**
 * Ask core which installed plugins have a newer build waiting (Tauri: `plugin_updates`), with the "needs a
 * decision first" gates judged wherever the plugin is enabled — no project is passed, because an update
 * replaces the build for every project at once (`AMB-D-379`), and this is asked from screens that are in no
 * project.
 *
 * Outside Tauri — `npm run dev` in a browser — there is no plugins directory and no catalog cache, so the
 * mock says nothing is waiting rather than inventing an offer.
 */
export async function fetchPluginUpdates(): Promise<PluginUpdate[]> {
  if (!inTauri()) return NONE;
  return invoke<PluginUpdate[]>("plugin_updates", {});
}

/**
 * The updates waiting, for the banner. One live query for the whole app: the banner is mounted once, and
 * every trigger is an invalidation of this key rather than a second reader.
 */
export function usePluginUpdates(): {
  updates: PluginUpdate[];
  loading: boolean;
  error: unknown;
} {
  const { data, loading, error } = useQuery<PluginUpdate[]>(["plugin-updates"], fetchPluginUpdates);
  return { updates: data ?? NONE, loading, error };
}

/** Re-ask core — the one call behind every trigger (focus return, a plugin screen opening, "check now"). */
export function refreshPluginUpdates(): void {
  invalidateQueries((key) => key[0] === "plugin-updates");
}

/** Refetch what an applied update changed: the offer itself, and the installs whose build just moved. */
function reloadAfterApply(): void {
  invalidateQueries((key) => key[0] === "plugin-updates" || key[0] === "plugin-installs");
}

/**
 * Apply one plugin's update (Tauri: `plugin_update_apply`). Every gate is core's — the asset is re-verified
 * against amenbo's catalog key and its checksum, the previous build is retained as a `.bak`, and the gate,
 * settings and secrets are carried over — so a refusal here is a message to show, not a state to guess at.
 *
 * `false` means there was nothing to apply: the catalog publishes the build already installed.
 */
export async function applyPluginUpdate(name: string): Promise<boolean> {
  if (!inTauri()) return false;
  const applied = await invoke<boolean>("plugin_update_apply", { name });
  reloadAfterApply();
  return applied;
}

/**
 * Apply every waiting update (Tauri: `plugin_update_apply_all`). Best-effort across plugins: one that fails
 * is left exactly as it was and comes back as a row saying why, so a mixed run reports both halves.
 */
export async function applyAllPluginUpdates(): Promise<PluginUpdateOutcome[]> {
  if (!inTauri()) return [];
  const outcomes = await invoke<PluginUpdateOutcome[]>("plugin_update_apply_all", {});
  reloadAfterApply();
  return outcomes;
}

/**
 * The identity a dismissal is keyed by: the plugin **and** the build offered for it. The digest is what
 * detection itself compared, so a catalog that moves the entry again mints a new id and the offer returns.
 */
export function updateId(u: PluginUpdate): string {
  return `${u.name}@${u.availableChecksum ?? ""}`;
}

// The dismissal, read once and then held: it is device-local state (localStorage, like the theme), and the
// banner subscribes to it, so writing it has to reach a component that is already mounted. Cached both to
// keep the snapshot reference stable for `useSyncExternalStore` and to keep a render off the disk.
let dismissedCache: string[] | null = null;
const dismissListeners = new Set<() => void>();

function writeDismissed(ids: string[]): void {
  dismissedCache = ids;
  try {
    localStorage.setItem(KEY, JSON.stringify(ids));
  } catch {
    /* unavailable: the dismissal still holds for this session, through the cache above */
  }
  for (const l of dismissListeners) l();
}

/** The builds whose offer was dismissed, or an empty list where localStorage is unavailable. */
export function getDismissedPluginUpdates(): string[] {
  if (dismissedCache) return dismissedCache;
  try {
    const raw = localStorage.getItem(KEY);
    const parsed: unknown = raw ? JSON.parse(raw) : null;
    dismissedCache = Array.isArray(parsed)
      ? parsed.filter((v): v is string => typeof v === "string")
      : [];
  } catch {
    dismissedCache = []; // unavailable or corrupt: nothing was remembered, so nothing is dismissed
  }
  return dismissedCache;
}

/** Subscribes to the dismissal moving — what lets a banner obey a dismissal made outside it. */
export function subscribeDismissedPluginUpdates(fn: () => void): () => void {
  dismissListeners.add(fn);
  return () => dismissListeners.delete(fn);
}

/**
 * Remember the offer of exactly these builds as dismissed. It **replaces** the record rather than adding to
 * it, which is what keeps the list from growing without bound: a build no longer offered — applied, or
 * uninstalled — is not in what we write, so its id goes.
 */
export function dismissPluginUpdates(updates: PluginUpdate[]): void {
  writeDismissed(updates.map(updateId));
}

/**
 * Forget every dismissal, so nothing offered stays hidden. The explicit "check for updates" calls it: asking
 * in so many words should surface what is waiting, even a build the user waved away earlier.
 */
export function clearDismissedPluginUpdates(): void {
  writeDismissed([]);
}

/** The offers still worth showing: those whose build has not been dismissed. */
export function pendingPluginUpdates(
  updates: PluginUpdate[],
  dismissed: readonly string[],
): PluginUpdate[] {
  return updates.filter((u) => !dismissed.includes(updateId(u)));
}
