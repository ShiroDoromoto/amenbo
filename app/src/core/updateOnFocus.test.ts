// The update check has to survive a session that only reads. Its trigger used to be the snapshot, which is only
// rebuilt when the store has moved — so someone who opens tasks and never writes was told about a new release at
// launch and never again. What is pinned here: **a focus return asks core for the update state even when the store
// signature has not moved**, and the answer reaches the snapshot the banner reads.
//
// The Tauri host is stubbed (`invoke`) so this runs on its own; the TTL that keeps the ask off the network lives in
// core, not here.
import { beforeEach, describe, it, expect, vi } from "vitest";

const invoke = vi.fn();
vi.mock("./ipc", () => ({
  invoke: (cmd: string, args?: unknown) => invoke(cmd, args),
  applyPerfConfig: () => {},
}));

vi.mock("./query", () => ({
  invalidateScopes: () => {},
  invalidateAllQueries: () => {},
}));

import { getSnapshot, loadSnapshot, reconcile, subscribe } from "./snapshot";

// `inTauri()` is only true inside the webview; the test stubs the host so it believes it is there.
(globalThis as unknown as { window: unknown }).window = { __TAURI_INTERNALS__: {} };

const SNAPSHOT = {
  language: "ja",
  onboarded: true,
  roster: [],
  projects: [],
  activity: [],
  startupHealth: { issues: [] },
  versionStatus: { appVersion: "1.3.0", updateAvailable: false, newerVersion: null },
  perfLog: null,
  updateCheck: true,
};

/** What core answers for `version_status`. Each test rewrites this. */
let upstream: unknown = { appVersion: "1.3.0", updateAvailable: false, newerVersion: null };

beforeEach(async () => {
  invoke.mockReset();
  upstream = { appVersion: "1.3.0", updateAvailable: false, newerVersion: null };
  invoke.mockImplementation((cmd: string) => {
    switch (cmd) {
      case "snapshot": return Promise.resolve(SNAPSHOT);
      case "store_signature": return Promise.resolve("sig-0"); // never moves: a read-only session.
      case "version_status": return upstream instanceof Error ? Promise.reject(upstream) : Promise.resolve(upstream);
      default: return Promise.resolve(null);
    }
  });
  await loadSnapshot(); // this is what notes the signature the focus path compares against.
  invoke.mockClear();
});

const asked = () => invoke.mock.calls.filter(([cmd]) => cmd === "version_status").length;
const reloaded = () => invoke.mock.calls.filter(([cmd]) => cmd === "snapshot").length;

describe("the update check on focus return", () => {
  it("asks core even though the store never moved (and re-reads no snapshot for it)", async () => {
    await reconcile("focus");

    expect(asked()).toBe(1);
    expect(reloaded()).toBe(0); // the whole point: the cheap ask, not a full re-read.
  });

  it("puts a newly offered version into the snapshot and tells subscribers", async () => {
    upstream = { appVersion: "1.3.0", updateAvailable: true, newerVersion: "1.4.0" };
    let notified = 0;
    const stop = subscribe(() => notified++);

    await reconcile("focus");

    expect(getSnapshot().versionStatus.newerVersion).toBe("1.4.0");
    expect(getSnapshot().versionStatus.updateAvailable).toBe(true);
    expect(notified).toBeGreaterThan(0);
    stop();
  });

  it("does not redraw when the answer is the one already on screen", async () => {
    let notified = 0;
    const stop = subscribe(() => notified++);

    await reconcile("focus");

    expect(asked()).toBe(1);
    expect(notified).toBe(0);
    stop();
  });

  it("stays silent when the ask fails — an update check never gets in the way", async () => {
    upstream = new Error("offline");

    await expect(reconcile("focus")).resolves.toBeUndefined();
    expect(getSnapshot().versionStatus.updateAvailable).toBe(false);
  });
});
