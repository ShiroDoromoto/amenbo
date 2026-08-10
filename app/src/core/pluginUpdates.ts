// **Which installed plugins the catalog has moved past** — the detection seam behind the update banner
// (`AMB-D-359`).
//
// Detection is core's and nothing is duplicated here: `plugin_updates` compares what this machine holds
// against the catalog it already fetches. **How far it goes is the trigger's to say** (`AMB-D-462`). The
// automatic ones — a focus return, opening a plugin screen — read through the catalog's freshness boundary, so
// the face can re-ask at every moment that matters while amenbo still holds no timer and opens no connection
// it would not have opened anyway (`AMB-D-331`'s posture, applied to plugins). An explicit "check now" goes to
// the catalog instead: what the boundary saves is the cost of the automatic triggers, and a press is not one
// of them.
//
// **An offer, never an application.** Nothing here updates anything on its own: `applyPluginUpdate` runs only
// when a button was pressed, and the gates it passes (signature, checksum, compatibility, the `required`
// re-check) are all core's.
//
// The dismissal is keyed by the **build** offered, not by the plugin, which is what makes a quiet banner stay
// quiet without going silent: dismissing the offer of one build says nothing about the next one, and a plugin
// whose catalog entry moves again surfaces on its own.
import { useSyncExternalStore } from "react";
import { agoSecondsLabel, currentLang, t, tf } from "./i18n";
import { invoke } from "./ipc";
import { inTauri, subscribe } from "./snapshot";
import { invalidateQueries, useQuery } from "./query";
import type {
  PluginCatalogReadDto,
  PluginUpdateDto,
  PluginUpdateOutcomeDto,
  PluginUpdateReachDto,
  PluginUpdatesDto,
} from "../bindings/bindings";

/** One installed plugin the catalog holds a different build of (generated DTO). */
export type PluginUpdate = PluginUpdateDto;
/** How one plugin fared in an "update all" (generated DTO). */
export type PluginUpdateOutcome = PluginUpdateOutcomeDto;
/** How far a check goes for its catalog (generated DTO). */
export type PluginUpdateReach = PluginUpdateReachDto;
/** What a check was measured against — how current its verdict is (generated DTO). */
export type PluginCatalogRead = PluginCatalogReadDto;
/** A check's whole answer: what has moved, and what that was measured against (generated DTO). */
export type PluginUpdatesAnswer = PluginUpdatesDto;

// The empty answer, held so the reference is stable across renders. `notNeeded` is the honest arm for it:
// where this stands in — outside Tauri, and before the first read lands — no catalog was read at all.
const NONE: PluginUpdatesAnswer = { updates: [], catalog: { read: "notNeeded" } };
const KEY = "amenbo.pluginUpdatesDismissed";

// The reach the next read will use. It is latched here rather than passed, because the query layer refetches
// through a fetcher it captured at mount: a trigger has no way to hand its own intent to the read it starts.
// `now` is sticky until a read spends it — an automatic trigger landing in between must not spend what a
// person asked for — and the read that spends it leaves the cheap default behind.
let reachNow = false;

/**
 * Ask core which installed plugins have a newer build waiting (Tauri: `plugin_updates`), with the "needs a
 * decision first" gates judged wherever the plugin is enabled — no project is passed, because an update
 * replaces the build for every project at once (`AMB-D-434`), and this is asked from screens that are in no
 * project.
 *
 * How far it goes is whatever the trigger that started it declared (`refreshPluginUpdates`), spent here so
 * the next read is back to the cheap one. **What comes back carries what it was measured against**, off the
 * same read: an empty list is the ordinary answer and it means two different things, so the frame travels
 * with it rather than being asked for separately.
 *
 * `lang` is what each offer's one line comes back in (`AMB-D-623`), off the documents the offer was read
 * from — so it adds no request of its own.
 *
 * Outside Tauri — `npm run dev` in a browser — there is no plugins directory and no catalog cache, so the
 * mock says nothing is waiting rather than inventing an offer.
 */
export async function fetchPluginUpdates(lang: string): Promise<PluginUpdatesAnswer> {
  const reach: PluginUpdateReach = reachNow ? "now" : "incidental";
  reachNow = false;
  if (!inTauri()) return NONE;
  return invoke<PluginUpdatesAnswer>("plugin_updates", { reach, lang });
}

/**
 * The updates waiting and how current they are. One live query for the whole app: the banner is mounted
 * once, and every trigger is an invalidation of this key rather than a second reader — the language being
 * the one part of the key, so an offer left on screen is re-read rather than left in the old language.
 */
export function usePluginUpdates(): {
  updates: PluginUpdate[];
  catalog: PluginCatalogRead;
  loading: boolean;
  error: unknown;
} {
  const lang = useSyncExternalStore(subscribe, currentLang);
  const { data, loading, error } = useQuery<PluginUpdatesAnswer>(["plugin-updates", lang], () =>
    fetchPluginUpdates(lang),
  );
  const answer = data ?? NONE;
  return { updates: answer.updates, catalog: answer.catalog, loading, error };
}

/**
 * Re-ask core — the one call behind every trigger (focus return, a plugin screen opening, "check now"), with
 * the trigger saying how far its answer should go (`AMB-D-462`).
 *
 * `incidental` is what an automatic trigger asks for: the freshness window answers it from the cache, which
 * is what lets there be several of them without a resident timer. `now` is for a button somebody pressed —
 * an answer off the cache would say "no updates" while meaning "none an hour ago", and the press would look
 * like it did nothing.
 */
export function refreshPluginUpdates(reach: PluginUpdateReach): void {
  if (reach === "now") reachNow = true;
  invalidateQueries((key) => key[0] === "plugin-updates");
}

/**
 * The one line saying how current the verdict beside it is, or `null` where there is nothing to frame.
 *
 * `notNeeded` is the empty one: nothing is installed, so no catalog was read and none is missing. A note
 * about a catalog nobody needed would only suggest something went wrong.
 */
export function catalogReadLine(catalog: PluginCatalogRead): string | null {
  const ago = () => agoSecondsLabel(catalog.ageSeconds ?? 0);
  switch (catalog.read) {
    case "fetched": return t("plugins.updates.catalog.fetched");
    case "cached": return tf("plugins.updates.catalog.cached", { ago: ago() });
    case "offline": return tf("plugins.updates.catalog.offline", { ago: ago() });
    case "unavailable": return t("plugins.updates.catalog.unavailable");
    case "notNeeded": return null;
  }
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
 * The identity a dismissal is keyed by: the plugin **and** the entry offered for it. The digest is the one
 * detection itself compared, so a catalog that moves the entry again mints a new id and the offer returns —
 * including when the executable did not move, which is a real update and must not stay buried under the
 * dismissal of an earlier one.
 */
export function updateId(u: PluginUpdate): string {
  return `${u.name}@${u.availableDetailSum ?? ""}`;
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
