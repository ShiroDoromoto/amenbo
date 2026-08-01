// The GUI's own query layer.
//
// A domain read is expressed as a key plus a fetcher, and every subscriber of the same key shares one
// result. No HTTP-oriented query library: this app's source of truth is a local SQLite behind a Tauri
// command — there is no URL and no HTTP cache, so such a library's assumptions do not hold.
//
// The cache is bounded (mounted entries plus a small LRU):
//   - A subscribed key (listeners non-empty = on screen) stays resident, and every subscriber shares the
//     same state object.
//   - When the last subscriber unmounts, listeners goes empty and the key falls back to a small
//     LRU (QUERY_LRU_MAX). A remount paints the cached result immediately and revalidates behind it
//     (no blank flash).
//   - Past the LRU cap, the oldest entry is dropped (nothing stays resident without bound).
//
// **Liveness is listeners and nothing else** — never keep a second tally such as a refcount. Two tallies
// drift, and the moment they do you get an entry that is on screen yet outside every invalidation path
// (that query is never refetched again). Making the subscriber set itself the sole liveness condition
// means "has subscribers but is not live" cannot be expressed. Eviction likewise happens only when
// listeners is empty, which closes the other path too: a subscribed entry being replaced by a different
// object, leaving its listeners orphaned.
//
// Invalidation: a write refetches just the matching keys via `invalidateQueries`, from its ack (affected
// ids + scope). An external write (store-changed) folds the rows the **change feed** names into scopes and
// refetches only the keys that touch them via `invalidateScopes` (`core/changes`). Only read-state changes,
// and the case where the feed cannot say what changed (gap → reconcile), reach for the coarse
// `invalidateAllQueries`. This layer never subscribes to the snapshot itself.
//
// Key convention (QueryKey = an array of primitives; the first element is the namespace, and targeted
// invalidation narrows on that prefix):
//   ["taskPage", projectId, filter, sort, limit, offset]  one page of a list/board
//   ["smartView", viewId, page, pageSize, me]             one page of a smart view
//   ["task", id]                                          a single task
//   ["decisions", projectId]                              decision records
import { useEffect, useReducer, useRef } from "react";

export type QueryKey = ReadonlyArray<string | number | boolean | null | undefined>;

export interface QueryState<T> {
  data: T | undefined;
  loading: boolean;
  error: unknown;
}

// Stable reference before the first load (the initial state of every entry, and what useQuery returns pre-mount).
const INITIAL: QueryState<unknown> = { data: undefined, loading: true, error: undefined };

interface Entry {
  state: QueryState<unknown>;       // Stable reference. Swapped for a new object only when the contents change.
  listeners: Set<() => void>;       // Re-render triggers of the subscribed hooks. **Non-empty = live** (the sole liveness condition).
  version: number;                  // Fetch generation. Discards stale (out-of-order) resolutions.
  fetcher: () => Promise<unknown>;  // The latest fetcher (it closes over props, so it is refreshed on every mount).
}

/** live = it has subscribers. What gets invalidated, and what may be evicted, both follow from this one predicate. */
function isLive(e: Entry): boolean {
  return e.listeners.size > 0;
}

// How many entries we keep after unmount. Each entry is one page or one item of bounded data. Set it small
// enough to overflow while merely moving between projects, views and the task detail pane, and every remount
// pays a blank flash followed by a full refetch behind it. Only subscribed entries are resident; this is the
// "recently seen" holding area, so it can afford to be generous (the cap is what stops unbounded residency).
const QUERY_LRU_MAX = 128;

const cache = new Map<string, Entry>();
/** Keys held with zero subscribers (oldest first: the head is the next eviction candidate). */
const lru: string[] = [];

function serializeKey(key: QueryKey): string {
  return JSON.stringify(key);
}

/** Refetch the live entries (those with subscribers) whose key matches the predicate (targeted invalidation from a write ack). */
export function invalidateQueries(pred: (key: QueryKey) => boolean): void {
  for (const [keyStr, e] of cache) {
    if (isLive(e) && pred(JSON.parse(keyStr) as QueryKey)) revalidate(keyStr);
  }
}

/** Refetch every live entry (coarse invalidation for changes we cannot target, such as read-state). */
export function invalidateAllQueries(): void {
  for (const [keyStr, e] of cache) {
    if (isLive(e)) revalidate(keyStr);
  }
}

/**
 * Targeted invalidation for external writes (store-changed). Refetches only the live queries that touch
 * `scopes`, folded from **the rows the change feed named**. The scope vocabulary has two suppliers —
 * `DATASET_SCOPES` in `core/changes` (the feed side) and `WriteAck.scopes` (our own writes) — and this is
 * its single consumer. It is not called when the feed cannot say what changed; that case falls through to
 * the full refetch of `reconcile("gap")`. The key→scope mapping errs on the safe side: an archived-projects
 * listing goes stale both on project-row changes and on changes to the task counts it shows, so it watches
 * both; and attachments cannot be narrowed from the key down to a row id, so any external write in the
 * attachment scope (CLI `attach` and the like) refetches every open viewer.
 */
export function invalidateScopes(scopes: ReadonlySet<string>): void {
  const touchesScope = (ns: string) => scopes.has(ns);
  invalidateQueries((key: QueryKey) => {
    switch (key[0]) {
      case "taskPage": return touchesScope("tasks");
      case "smartView": return touchesScope("tasks");
      case "task": return touchesScope("tasks");
      case "archivedProjects": return touchesScope("projects") || touchesScope("tasks");
      case "decisions": return touchesScope("decisions");
      case "decision": return touchesScope("decisions");
      case "decisionComments": return touchesScope("decisions");
      case "attachments": return touchesScope("attachments");
      // A page of hits is drawn from every face at once — a task's words, a decision's, a comment on
      // either, an axis label, an attachment's name — so it goes stale on any of the scopes those sit in.
      case "search": return touchesScope("tasks") || touchesScope("decisions") || touchesScope("attachments");
      // The installed rows carry the gate the change feed just moved; the plugins themselves are files on
      // disk, so nothing else on that screen goes stale with them.
      case "plugin-installs": return touchesScope("plugins");
      default: return false;
    }
  });
}

function setState(e: Entry, next: QueryState<unknown>): void {
  e.state = next;
  for (const l of e.listeners) l();
}

/** Subscribe, creating the entry if it is absent. The caller adds its listener to the entry we return — that is where it becomes live. */
function subscribe(keyStr: string, listener: () => void, fetcher: () => Promise<unknown>): Entry {
  let e = cache.get(keyStr);
  if (!e) {
    e = { state: INITIAL, listeners: new Set(), version: 0, fetcher };
    cache.set(keyStr, e);
  }
  e.fetcher = fetcher;
  e.listeners.add(listener);
  const i = lru.indexOf(keyStr);
  if (i !== -1) lru.splice(i, 1); // A remount takes it off the eviction list (the cached result gets reused).
  return e;
}

/**
 * Unsubscribe. **It removes from the very entry it added to** (never looking the key up again), so one
 * subscriber's bookkeeping cannot take another's down with it: no double release, and no release ordering,
 * can kill a live entry. Only a key whose last subscriber left falls back to the LRU.
 */
function unsubscribe(keyStr: string, e: Entry, listener: () => void): void {
  e.listeners.delete(listener);
  const cur = cache.get(keyStr);
  if (!cur || isLive(cur)) return; // Someone is still watching it (or it is already gone): do not evict.
  if (!lru.includes(keyStr)) lru.push(keyStr);
  while (lru.length > QUERY_LRU_MAX) {
    const old = lru.shift();
    if (old === undefined) break;
    const oe = cache.get(old);
    if (oe && !isLive(oe)) cache.delete(old); // Evict only with zero subscribers, so a live entry is never swapped out.
  }
}

function revalidate(keyStr: string): void {
  const e = cache.get(keyStr);
  if (!e) return;
  const v = ++e.version;
  if (!e.state.loading) setState(e, { ...e.state, loading: true }); // Keep the cached data, mark it as refetching.
  e.fetcher()
    .then((data) => { if (e.version === v) setState(e, { data, loading: false, error: undefined }); })
    .catch((error) => { if (e.version === v) setState(e, { ...e.state, loading: false, error }); });
}

/**
 * Subscribe to a domain read as a key plus a fetcher. Subscribers of the same key share one result, and the
 * cache is bounded (mounted entries plus a small LRU). A key change, a mount, or a coarse invalidation
 * triggers a refetch.
 * **As long as it renders, the subscription is re-established** (self-healing). If a query that is on screen
 * drops out of the live set, no invalidation ever reaches it again and that view freezes stale. This layer
 * **always repairs "on screen but not live" on the very next render** — without having to know where it came
 * undone.
 */
export function useQuery<T>(key: QueryKey, fetcher: () => Promise<T>): QueryState<T> {
  const keyStr = serializeKey(key);
  const fetcherRef = useRef(fetcher);
  fetcherRef.current = fetcher; // Refresh on every render (the fetcher closes over its arguments = props).
  const [, force] = useReducer((c: number) => c + 1, 0);

  useEffect(() => {
    const e = subscribe(keyStr, force, () => fetcherRef.current());
    revalidate(keyStr); // Refetch on mount / key change (on an LRU hit, show the old value and update behind it).
    return () => unsubscribe(keyStr, e, force);
  }, [keyStr]);

  // Self-healing (no deps = every render). If our listener is not on the cached entry, invalidation will never
  // reach this query again = it stays on screen and permanently stale. Re-subscribe and refetch. In the healthy
  // case this costs one Map lookup plus one Set membership check (a constant per render).
  useEffect(() => {
    const cur = cache.get(keyStr);
    if (cur && cur.listeners.has(force)) return;
    // That this happened at all is the anomaly, so leave a trace (next time we hit it, we know which key came undone).
    console.warn(`[query] resubscribed a displayed query that had lost its subscription: ${keyStr}`);
    subscribe(keyStr, force, () => fetcherRef.current());
    revalidate(keyStr);
  });

  // Before mount (effects have not run) we paint an LRU-retained entry if there is one, else INITIAL. State
  // changes land via setState→listeners→force, and an entry's state reference is swapped only when it actually
  // changes (stable rendering).
  const e = cache.get(keyStr);
  return (e ? e.state : INITIAL) as QueryState<T>;
}

/**
 * The raw cache, for diagnostics and regression tests. It is the only way to inspect liveness (each entry's
 * subscriber count) on the spot when a view freezes, and the way a test can manufacture the "on screen but
 * unsubscribed" state. Do not touch it from product code.
 */
export const __queryCache: ReadonlyMap<string, { listeners: Set<() => void> }> = cache;
