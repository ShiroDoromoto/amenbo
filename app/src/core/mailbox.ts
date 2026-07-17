// The inbox's notification affordance. It does two things:
//  1. Supplies the nav's unread badge count reactively from the inbox set (`reads.loadInboxTaskIds`).
//  2. Recomputes the inbox set on every snapshot notification (a write command returning, a `store-changed`
//     event, a read-state change) and, when an id appears that was not there last time, raises an OS
//     notification (the `notify_os` command).
//
// Properties to preserve:
//  - Fetching the set is async, so `running`/`queued` serialise it and fold repeated triggers into one.
//  - The first load (`known === null`) stays silent — nothing pent up at startup fires a notification.
//  - A set that merely shrinks (marked read, or done) contains no new id, so nothing is raised.
//  - The arrival sound belongs to the OS notification; the app has no beep of its own.
import { useSyncExternalStore } from "react";
import { getInboxDataGeneration, inTauri, subscribe } from "./snapshot";
import { loadInboxTaskIds } from "./reads";
import { invoke } from "./ipc";
import { tf, t } from "./i18n";
import { pushNotice } from "./notice";

let count = 0;
let known: Set<number> | null = null; // null = before the first load (which never notifies)
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
    const ids = await loadInboxTaskIds();
    const next = new Set(ids);
    if (known) {
      const fresh = ids.filter((id) => !known!.has(id));
      if (fresh.length > 0) void notifyArrival(fresh.length);
    }
    known = next;
    if (count !== next.size) {
      count = next.size;
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
    await invoke("notify_os", { title: t("mailbox.notifyTitle"), body: tf("mailbox.notifyBody", { n }) });
  } catch (e) {
    console.error("[amenbo] OS notification (notify_os) failed:", e);
    if (!notifyFailureHintShown) {
      notifyFailureHintShown = true;
      pushNotice(t("mailbox.notifyFailed"));
    }
  }
}
