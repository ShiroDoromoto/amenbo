// Fault injection: fire `store-changed` with the change feed broken. The feed is bounded (it gets
// truncated), the whole file can be swapped out from under it, and a gap in the hook's allowlist can
// leave a mutation unrecorded. Every one of those looks exactly like "the feed is empty, so nothing
// changed" — and believing that leaves the screen adrift from the source of truth forever. One thing
// is pinned here: **when the feed cannot say what changed, we always fall to `reconcile("gap")`, a
// full re-read from the source of truth.**
//
// The other half is which leg of the signature moved (`AMB-D-856`). A file swapped out and a
// `config.json` written are the two the feed structurally cannot speak for, and they are read off the
// signature rather than guessed at from an empty page — which is what lets an empty page mean what it
// says: a commit with no row on the feed, and nothing to re-read.
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
const recheckSetupAgain = vi.fn();
vi.mock("./query", () => ({
  invalidateScopes: (s: ReadonlySet<string>) => invalidateScopes(s),
  invalidateAllQueries: () => invalidateAllQueries(),
  recheckSetupAgain: () => recheckSetupAgain(),
}));

// Intercept the subscriptions so a test can fire a wake-up whenever it likes. `wake` is the last
// `store-changed` handler subscribed; `handed` is the talk window's handler for what the board hands over.
let wake: (() => Promise<void>) | null = null;
let handed: ((payload: unknown) => void) | null = null;
const emit = vi.fn();
vi.mock("@tauri-apps/api/event", () => ({
  listen: async (name: string, handler: (e: { payload: unknown }) => unknown) => {
    if (name === "store-changed") wake = async () => { await handler({ payload: null }); };
    else handed = (payload) => { handler({ payload }); };
    return () => {};
  },
  emit: (name: string, payload: unknown) => { emit(name, payload); return Promise.resolve(); },
}));

// Whether the board window is open, as the talk window asks it.
let boardOpen = true;
vi.mock("@tauri-apps/api/window", () => ({
  Window: { getByLabel: async (label: string) => (label === "main" && boardOpen ? {} : null) },
}));

import {
  followStore,
  getLastReconcile,
  getSnapshot,
  loadSnapshot,
  subscribe,
  reconcile,
  subscribeStoreChangeReflected,
  watchStore,
  type StoreChangeReflected,
} from "./snapshot";

// `inTauri()` is only true inside the webview; the test stubs the host so it believes it is there.
(globalThis as unknown as { window: unknown }).window = { __TAURI_INTERNALS__: {} };

const SNAPSHOT = {
  language: "ja",
  roster: [],
  projects: [],
  activity: [],
  startupHealth: { issues: [] },
  versionStatus: { appVersion: "", updateAvailable: false, newerVersion: null },
  perfLog: null,
  updateCheck: true,
  autostart: false,
  defaultView: "board",
};

/** What core is made to answer. Each test rewrites these. */
let signature = { file: "file-0", config: "config-0", version: "1" };
let head = 0; // `change_cursor` (the feed's current head)
let pages: unknown[] = []; // The replies to `changes_since` (consumed oldest first)

const row = (dataset: string) => ({ dataset, rowId: 1, op: "update" as const });

/** Somebody committed: `data_version` moves and the two file legs stand still. */
function committed(): void {
  signature = { ...signature, version: String(Number(signature.version) + 1) };
}

/** The store file was swapped out from under us (`fold`, `stage_and_swap`, a restore). */
function fileSwapped(): void {
  signature = { file: `${signature.file}-swapped`, config: signature.config, version: "1" };
}

/** `config.json` was written — a file no row of the feed will ever speak for. */
function configWritten(): void {
  signature = { ...signature, config: `${signature.config}-again` };
}

function feed(rows: ReturnType<typeof row>[], cursor: number, expired = false) {
  return { rows, cursor, more: false, expired };
}

beforeEach(() => {
  invoke.mockReset();
  invalidateScopes.mockReset();
  invalidateAllQueries.mockReset();
  recheckSetupAgain.mockReset();
  emit.mockReset();
  boardOpen = true;
  signature = { file: "file-0", config: "config-0", version: "1" };
  head = 0;
  pages = [];
  invoke.mockImplementation((cmd: string) => {
    switch (cmd) {
      case "snapshot": return Promise.resolve({ ...SNAPSHOT, signature }); // core reads it before the rows
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
  recheckSetupAgain.mockClear();
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
    committed(); // Another process wrote.
    pages = [feed([row("task"), row("task_comment")], 12)];
    const { seen, stop } = captureReflected();

    await fire();

    expect(invalidateScopes).toHaveBeenCalledWith(new Set(["tasks"]));
    expect(invalidateAllQueries).not.toHaveBeenCalled();
    expect(seen.map((r) => r.reason)).toEqual(["live"]);
    stop();
  });

  it("a device-local table is a scope like any other — the folders bound to a project fold to `projects`", async () => {
    head = 10;
    const fire = await boot();
    committed();
    pages = [feed([row("binding_project_dir")], 11)];
    const { seen, stop } = captureReflected();

    await fire();

    expect(invalidateScopes).toHaveBeenCalledWith(new Set(["projects"]));
    expect(invalidateAllQueries).not.toHaveBeenCalled();
    expect(seen.map((r) => r.reason)).toEqual(["live"]);
    stop();
  });

  it("a commit with no row on the feed reads nothing and announces nothing — and the next wake is still targeted", async () => {
    head = 10;
    const fire = await boot();
    committed(); // The dispatcher drained its outbox: a real commit, and no dataset the feed collects.
    pages = [feed([], 10)];
    const { seen, stop } = captureReflected();

    await fire();

    expect(reloaded()).toBe(0);
    expect(invalidateScopes).not.toHaveBeenCalled();
    expect(invalidateAllQueries).not.toHaveBeenCalled();
    expect(seen).toEqual([]);
    // …except whether to ask for setup again: the build a send brings back is written off the feed, so
    // an empty page is the only wake that can carry it. That one query, and nothing else.
    expect(recheckSetupAgain).toHaveBeenCalledTimes(1);

    // The signature was carried forward, so the write that follows is compared against what is on
    // disk now — not against a store two commits old, which would read as a swap and re-read everything.
    committed();
    pages = [feed([row("task")], 11)];

    await fire();

    expect(invalidateScopes).toHaveBeenCalledWith(new Set(["tasks"]));
    expect(invalidateAllQueries).not.toHaveBeenCalled();
    stop();
  });

  it("thins out our own (GUI) writes — already reflected via ack, and nothing left unread on the feed", async () => {
    head = 10;
    const fire = await boot();
    pages = [feed([], 10)]; // Our rows were read off the feed already: nothing past the cursor.
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
    committed();
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
    committed();
    pages = [feed([row("decision_comment")], 901)];
    invalidateAllQueries.mockClear();

    await fire();

    expect(invalidateScopes).toHaveBeenCalledWith(new Set(["decisions"]));
    expect(invalidateAllQueries).not.toHaveBeenCalled();
    stop();
  });

  it("whole-file replacement (fold / stage_and_swap / restore) → the feed is not continuous → gap", async () => {
    head = 5;
    const fire = await boot();
    fileSwapped(); // A different file is in place now.
    pages = [feed([], 5)]; // The swapped-in file's feed knows nothing of our changes.
    const { seen, stop } = captureReflected();

    await fire();

    expect(getLastReconcile()?.reason).toBe("gap");
    expect(invalidateAllQueries).toHaveBeenCalledTimes(1);
    expect(seen.map((r) => r.reason)).toEqual(["gap"]);
    stop();
  });

  it("`config.json` written from the CLI → no row will ever speak for it → gap", async () => {
    head = 5;
    const fire = await boot();
    configWritten(); // A language, a theme, a default view: written straight to disk, not to the database.
    pages = [feed([], 5)];
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

    committed(); // By the time of the re-read, the disk has moved.
    await reconcile("focus");

    expect(reloaded()).toBeGreaterThan(0);
    expect(invalidateAllQueries).toHaveBeenCalled();
  });
});

describe("watchStore — an outside write that lands while the store is being re-read is not dropped", () => {
  it("a write landing between the drain and the snapshot read is read off the feed on its own wake", async () => {
    head = 10;
    const fire = await boot();
    committed(); // Write A from outside.
    head = 11;
    // Right after the feed hands back A, write B lands — before the snapshot reads the store, so the
    // snapshot's signature already counts B, and B's wake will find nothing moved.
    let slipIn = true;
    invoke.mockImplementation((cmd: string) => {
      switch (cmd) {
        case "snapshot": return Promise.resolve({ ...SNAPSHOT, signature });
        case "store_signature": return Promise.resolve(signature);
        case "change_cursor": return Promise.resolve(head);
        case "changes_since": {
          const page = pages.shift() ?? feed([], head);
          if (slipIn) {
            slipIn = false;
            committed(); // Write B from outside.
            head = 12;
            pages.push(feed([row("decision_comment")], 12));
          }
          return Promise.resolve(page);
        }
        default: return Promise.resolve(null);
      }
    });
    pages = [feed([row("task")], 11)];

    await fire(); // A's wake.

    expect(invalidateScopes).toHaveBeenCalledWith(new Set(["tasks"]));
    invalidateScopes.mockClear();
    invoke.mockClear();

    await fire(); // B's wake: the signature matches the snapshot just read, yet B's row is still unread.

    expect(invalidateScopes).toHaveBeenCalledWith(new Set(["decisions"]));
    expect(invalidateAllQueries).not.toHaveBeenCalled();
    expect(reloaded()).toBe(0); // The snapshot already counts B; only the queries B touched are owed.
  });
});

describe("watchStore — the signature it compares against comes with the snapshot, not after it", () => {
  it("a write from outside that lands after the rows were read is not taken for our own", async () => {
    head = 5;
    // Core read the signature, then the rows; somebody committed before a separate signature read
    // could have been made. The snapshot's own signature is the older one.
    invoke.mockImplementation((cmd: string) => {
      switch (cmd) {
        case "snapshot": {
          const before = signature;
          committed();
          return Promise.resolve({ ...SNAPSHOT, signature: before });
        }
        case "store_signature": return Promise.resolve(signature);
        case "change_cursor": return Promise.resolve(head);
        case "changes_since": return Promise.resolve(pages.shift() ?? feed([], 0));
        default: return Promise.resolve(null);
      }
    });
    const fire = await boot();
    pages = [feed([row("task")], 6)];

    await fire();

    expect(invalidateScopes).toHaveBeenCalledWith(new Set(["tasks"]));
  });
});

describe("one outside write is one read of the snapshot, however many windows are open", () => {
  it("the board reads, and hands what it read to the talk window", async () => {
    head = 10;
    const fire = await boot();
    committed();
    pages = [feed([row("task")], 11)];

    await fire();

    expect(reloaded()).toBe(1);
    expect(emit).toHaveBeenCalledTimes(1);
    const [name, payload] = emit.mock.calls[0];
    expect(name).toBe("store-reflected");
    expect(payload).toMatchObject({
      snapshot: { signature },
      scopes: ["tasks"],
      inbox: true,
      recheckSetup: false,
      reflected: { reason: "live" },
    });
  });

  it("the talk window reads nothing while the board is open, and lays down what the board handed over", async () => {
    await boot();
    await followStore();
    invoke.mockClear();
    const { seen, stop } = captureReflected();
    let redrawn = 0;
    const unsub = subscribe(() => redrawn++);

    await wake!(); // The talk window's own wake: the board is open, so it is the board's to read.

    expect(invoke).not.toHaveBeenCalled();

    const snapshot = { ...SNAPSHOT, tasks: [], decisions: [], projects: [{ id: 1 }], signature };
    handed!({ snapshot, scopes: ["projects"], inbox: false, recheckSetup: false, reflected: { reason: "live", at: 1 } });

    expect(getSnapshot()).toBe(snapshot);
    expect(redrawn).toBe(1);
    expect(invalidateScopes).toHaveBeenCalledWith(new Set(["projects"]));
    expect(seen).toEqual([{ reason: "live", at: 1 }]);
    expect(invoke).not.toHaveBeenCalled();
    unsub();
    stop();
  });

  it("with the board closed, the talk window reads for itself", async () => {
    head = 10;
    await boot();
    await followStore();
    invoke.mockClear();
    boardOpen = false;
    committed();
    pages = [feed([row("project")], 11)];

    await wake!();

    expect(reloaded()).toBe(1);
    expect(invalidateScopes).toHaveBeenCalledWith(new Set(["projects"]));
    expect(emit).not.toHaveBeenCalled(); // Nobody to hand it to.
  });
});
