// The inbox's notification affordance. It does two things:
//  1. Supplies the nav's unread badge count reactively from the inbox set (`reads.loadInboxItems`).
//  2. Recomputes the inbox on every snapshot notification (a write command returning, a `store-changed`
//     event, a read-state change) and raises one aggregated OS notification (the `notify_os` command) for
//     the items **eligible on their source's gate and not yet notified** — source D's `unread` or source C's
//     `unseen` — recording them so they never fire again.
//
// What "not yet notified" rests on: a device-local persisted set (`mailboxNotified.ts` → the store's
// `mailbox_notified` table), loaded once at startup as the baseline. It is the persistent form of what used
// to be an in-memory "seen this run" set: because it survives restarts, the same judgment — unread ∧
// un-notified — serves both the live path and the startup catch-up. So an item that arrived while the app was
// closed is announced once on the next launch and never again. There is no first-load special case: a fresh
// store's notified set is empty, so the first launch simply announces what is unread and records it; a
// subsequent launch with nothing new stays silent because everything unread is already recorded.
//
// Properties to preserve:
//  - Fetching the inbox is async, so `running`/`queued` serialise it and fold repeated triggers into one.
//  - An item notifies only on its source's gate (D `unread`, C `unseen`); a read D item or a seen C item stays
//    silent, and an item already in the notified set never re-fires.
//  - The badge counts the whole inbox (C ∪ D), independent of read/seen state; only the notification is gated.
//  - The arrival sound belongs to the OS notification; the app has no beep of its own.
import { useSyncExternalStore } from "react";
import { getInboxDataGeneration, inTauri, subscribe } from "./snapshot";
import { loadInboxItems, type InboxItemBrief } from "./reads";
import { loadNotified, addNotified } from "./mailboxNotified";
import { invoke } from "./ipc";
import { t, tn } from "./i18n";
import { pushNotice } from "./notice";

/**
 * The arrival rule, isolated so it can be reasoned about on its own: of the inbox items, the ids that should be
 * announced now are the ones **eligible on their source's gate** — source D's `unread` or source C's `unseen` —
 * and **not already in the notified set**. Each source gates itself (a read D item, or a seen C item, never
 * announces); an item already announced (this run or a previous launch, since the set is persisted) never fires
 * again. The caller announces the returned ids once, aggregated, and adds them to the set.
 */
export function arrivalsToAnnounce(items: InboxItemBrief[], notified: Set<number>): number[] {
  return items.filter((it) => (it.unread || it.unseen) && !notified.has(it.id)).map((it) => it.id);
}

let count = 0;
let notified: Set<number> | null = null; // the persisted "already announced" set; null = not loaded yet
let started = false;
let running = false;
let queued = false;
/** The domain generation the inbox was last recomputed for (a read-state-only notification does not recompute). */
let seenGeneration = -1;
const listeners = new Set<() => void>();

function emit(): void {
  for (const l of listeners) l();
}

async function recompute(): Promise<void> {
  if (running) {
    queued = true; // Triggers fired while running fold into a single rerun
    return;
  }
  running = true;
  try {
    const items = await loadInboxItems();
    if (notified === null) notified = new Set(await loadNotified());
    // Announce the source-eligible items we have not announced before, aggregated into one toast, and record them so
    // the next recompute (this run or a later launch) does not announce them again. The in-memory set is
    // updated before the async persist, so a rapid re-trigger cannot double-announce the same id.
    const fresh = arrivalsToAnnounce(items, notified);
    if (fresh.length > 0) {
      for (const id of fresh) notified.add(id);
      void notifyArrival(fresh.length);
      void addNotified(fresh);
    }
    if (count !== items.length) {
      count = items.length;
      emit();
    }
  } finally {
    running = false;
    if (queued) {
      queued = false;
      void recompute();
    }
  }
}

/**
 * Start the inbox controller, subscribing exactly once for the life of the app. Idempotent. Of all the snapshot
 * notifications, only those that **advance a generation the inbox depends on** cause a recompute: real data
 * being refetched (a write ack, `store-changed`, focus catch-up) and archiving (`notifyInboxChanged`). A
 * read-receipt `notifyDataChanged` — clicking an item as read, say — cannot change who is in the inbox, so it
 * must not set off a recompute that costs several invokes.
 */
function start(): void {
  if (started) return;
  started = true;
  seenGeneration = getInboxDataGeneration();
  void recompute();
  subscribe(() => {
    const g = getInboxDataGeneration();
    if (g === seenGeneration) return;
    seenGeneration = g;
    void recompute();
  });
}

/** The nav's inbox badge count (the current size of C ∪ D). At 0 the caller shows no badge. */
export function useInboxCount(): number {
  start();
  return useSyncExternalStore(
    (fn) => {
      listeners.add(fn);
      return () => listeners.delete(fn);
    },
    () => count,
  );
}

// ───────────────────────── Announcing arrivals ─────────────────────────

// Whether the UI has already been told once that the OS notification path failed. A toast every time would be
// insufferable, so it is said once.
let notifyFailureHintShown = false;

/**
 * Announce an arrival through an OS notification. Delivery is concentrated in the native `notify_os` command
 * (UNUserNotificationCenter on macOS; the notification plugin on Windows and Linux; permission is settled at
 * startup, so JS needs no permission dance of its own). Both the sound and the toast belong to the OS — the app
 * makes no noise of its own. Failure is never swallowed: a denied permission, a missing plugin or an unregistered
 * delegate would kill notifications silently and nobody would ever notice, so it always reaches the log, and the
 * first time it also raises one toast in the UI — the way back to enabling permission.
 */
async function notifyArrival(n: number): Promise<void> {
  if (!inTauri()) return;
  try {
    await invoke("notify_os", { title: t("mailbox.notifyTitle"), body: tn("mailbox.notifyBody", n) });
  } catch (e) {
    console.error("[amenbo] OS notification (notify_os) failed:", e);
    if (!notifyFailureHintShown) {
      notifyFailureHintShown = true;
      pushNotice(t("mailbox.notifyFailed"));
    }
  }
}
