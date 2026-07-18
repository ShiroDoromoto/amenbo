// Fault injection: fire `store-changed` with the change feed broken. The feed is bounded (it gets
// truncated), the whole file can be swapped out from under it, and a gap in the hook's allowlist can
// leave a mutation unrecorded. Every one of those looks exactly like "the feed is empty, so nothing
// changed" — and believing that leaves the screen adrift from the source of truth forever. One thing
// is pinned here: **when the feed cannot say what changed, we always fall to `reconcile("gap")`, a
// full re-read from the source of truth.**
//
// The Tauri host is stubbed (`listen` / `invoke`) so this runs on its own. The dataset-to-scope
// folding itself is covered by `changes.test.ts`.
import { beforeEach, describe, it, expect, vi } from "vitest";

const invoke = vi.fn();
vi.mock("./ipc", () => ({
  invoke: (cmd: string, args?: unknown) => invoke(cmd, args),
  applyPerfConfig: () => {},
}));

const invalidateScopes = vi.fn();
const invalidateAllQueries = vi.fn();
vi.mock("./query", () => ({
  invalidateScopes: (s: ReadonlySet<string>) => invalidateScopes(s),
  invalidateAllQueries: () => invalidateAllQueries(),
}));

// Intercept the `store-changed` subscription so a test can fire a wake-up whenever it likes.
let wake: (() => Promise<void>) | null = null;
vi.mock("@tauri-apps/api/event", () => ({
  listen: async (_name: string, handler: (e: unknown) => Promise<void>) => {
    wake = () => handler({ payload: null });
    return () => {};
  },
}));

import {
  getLastReconcile,
  loadSnapshot,
  reconcile,
  subscribeStoreChangeReflected,
  watchStore,
  type StoreChangeReflected,
} from "./snapshot";

// `inTauri()` is only true inside the webview; the test stubs the host so it believes it is there.
(globalThis as unknown as { window: unknown }).window = { __TAURI_INTERNALS__: {} };

const SNAPSHOT = {
  language: "ja",
  onboarded: true,
  roster: [],
  projects: [],
  activity: [],
  startupHealth: { issues: [] },
  versionStatus: { appVersion: "", updateAvailable: false, newerVersion: null },
  perfLog: null,
  updateCheck: true,
};

/** What core is made to answer. Each test rewrites these. */
let signature = "sig-0";
let head = 0; // `change_cursor` (the feed's current head)
let pages: unknown[] = []; // The replies to `changes_since` (consumed oldest first)

const row = (dataset: string) => ({ dataset, rowId: 1, op: "update" as const });

function feed(rows: ReturnType<typeof row>[], cursor: number, expired = false) {
  return { rows, cursor, more: false, expired };
}

beforeEach(() => {
  invoke.mockReset();
  invalidateScopes.mockReset();
  invalidateAllQueries.mockReset();
  signature = "sig-0";
  head = 0;
  pages = [];
  invoke.mockImplementation((cmd: string) => {
    switch (cmd) {
      case "snapshot": return Promise.resolve(SNAPSHOT);
      case "store_signature": return Promise.resolve(signature);
      case "change_cursor": return Promise.resolve(head);
      case "changes_since": return Promise.resolve(pages.shift() ?? feed([], 0));
      default: return Promise.resolve(null);
    }
  });
});

/** The startup sequence itself (read the store through, noting the cursor, then subscribe). Returns the wake-up trigger. */
async function boot(): Promise<() => Promise<void>> {
  await loadSnapshot(); // The change_cursor is noted here, before the store is read through.
  await watchStore();
  invalidateScopes.mockClear();
  invalidateAllQueries.mockClear();
  invoke.mockClear();
  if (!wake) throw new Error("watchStore did not set up a subscription");
  return wake;
}

/** Catch the notifications that an external change was reflected. */
function captureReflected(): { seen: StoreChangeReflected[]; stop: () => void } {
  const seen: StoreChangeReflected[] = [];
  const stop = subscribeStoreChangeReflected((r) => seen.push(r));
  return { seen, stop };
}

/** Whether the source of truth was re-read — that is, whether `snapshot` was fetched again. */
const reloaded = () => invoke.mock.calls.filter(([cmd]) => cmd === "snapshot").length;

describe("watchStore — while the feed can speak, refetch only the surfaces that changed", () => {
  it("external write → invalidates only the scope folded from the feed's dataset (no full re-read)", async () => {
    head = 10;
    const fire = await boot();
    signature = "sig-1"; // Another process wrote.
    pages = [feed([row("task"), row("task_comment")], 12)];
    const { seen, stop } = captureReflected();

    await fire();

    expect(invalidateScopes).toHaveBeenCalledWith(new Set(["tasks"]));
    expect(invalidateAllQueries).not.toHaveBeenCalled();
    expect(seen.map((r) => r.reason)).toEqual(["live"]);
    stop();
  });

  it("thins out our own (GUI) writes — already reflected via ack (do not fire twice from the feed)", async () => {
    head = 10;
    const fire = await boot();
    const { seen, stop } = captureReflected(); // The signature is unchanged, so the write was ours.

    await fire();

    expect(invalidateScopes).not.toHaveBeenCalled();
    expect(invalidateAllQueries).not.toHaveBeenCalled();
    expect(reloaded()).toBe(0);
    expect(seen).toEqual([]);
    stop();
  });
});

describe("watchStore — when the feed cannot speak, always re-read from the source of truth (DoD)", () => {
  it("cursor expiry (truncation dropped an unread row) → full re-read via gap; afterward it resumes from the feed's head", async () => {
    head = 1;
    const fire = await boot();
    signature = "sig-1";
    pages = [feed([], 900, true)]; // expired: an empty reply must not be read as "nothing changed".
    const { seen, stop } = captureReflected();

    await fire();

    expect(getLastReconcile()?.reason).toBe("gap");
    expect(invalidateAllQueries).toHaveBeenCalledTimes(1); // The screen re-reads until it matches the source of truth.
    expect(invalidateScopes).not.toHaveBeenCalled();
    expect(reloaded()).toBe(1);
    expect(seen.map((r) => r.reason)).toEqual(["gap"]);

    // Recovery: the re-read notes the new cursor (the feed's current head), so the next wake-up is
    // targeted again — we do not stay stuck in gap.
    head = 900;
    signature = "sig-2";
    pages = [feed([row("decision")], 901)];
    invalidateAllQueries.mockClear();

    await fire();

    expect(invalidateScopes).toHaveBeenCalledWith(new Set(["decisions"]));
    expect(invalidateAllQueries).not.toHaveBeenCalled();
    stop();
  });

  it("whole-file replacement (fold / stage_and_swap / restore) → the feed is not continuous → gap", async () => {
    head = 5;
    const fire = await boot();
    signature = "sig-swapped"; // A different file is in place now.
    pages = [feed([], 5)]; // The swapped-in file's feed knows nothing of our changes.
    const { seen, stop } = captureReflected();

    await fire();

    expect(getLastReconcile()?.reason).toBe("gap");
    expect(invalidateAllQueries).toHaveBeenCalledTimes(1);
    expect(seen.map((r) => r.reason)).toEqual(["gap"]);
    stop();
  });

  it("even with a missing feed row (a mutation that never landed on the feed), the focus-return reconcile pulls back to the source of truth (defense in depth)", async () => {
    head = 5;
    const fire = await boot();
    // The worst case: a mutation happened, yet the signature did not move, so the watcher reads it as
    // our own. The wake-up path does nothing, and if that were the end of it the screen would freeze
    // on stale data.
    await fire();
    expect(reloaded()).toBe(0);

    signature = "sig-late"; // By the time of the re-read, the disk has moved.
    await reconcile("focus");

    expect(reloaded()).toBeGreaterThan(0);
    expect(invalidateAllQueries).toHaveBeenCalled();
  });
});
