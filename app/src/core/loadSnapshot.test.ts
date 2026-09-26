// `loadSnapshot` never runs two reads at once. A call made while one is in flight waits for a single read
// queued behind it, which every later call joins, so the last caller always sees a store read after it
// asked — whatever order core's replies come back in. The Tauri host is stubbed (`invoke`), and each
// `snapshot` reply is held until the test lets it go.
import { beforeEach, describe, it, expect, vi } from "vitest";

const invoke = vi.fn();
vi.mock("./ipc", () => ({
  invoke: (cmd: string, args?: unknown) => invoke(cmd, args),
  applyPerfConfig: () => {},
}));

import { getInboxDataGeneration, getSnapshot, loadSnapshot } from "./snapshot";

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
  signature: { file: "file-0", config: "config-0", version: "1" },
};

/** The `snapshot` replies not yet let go, oldest first. */
let held: { resolve: (language: string) => void; reject: (e: unknown) => void }[] = [];

const reads = () => invoke.mock.calls.filter(([cmd]) => cmd === "snapshot").length;

/** Let the microtasks run, so a read queued behind a finished one gets to invoke. */
const settle = () => new Promise((r) => setTimeout(r, 0));

beforeEach(() => {
  held = [];
  invoke.mockReset();
  invoke.mockImplementation((cmd: string) => {
    if (cmd !== "snapshot") return Promise.resolve(0);
    return new Promise((resolve, reject) => {
      held.push({ resolve: (language) => resolve({ ...SNAPSHOT, language }), reject });
    });
  });
});

describe("loadSnapshot — reads never overlap", () => {
  it("calls made while a read is in flight share one read, started after the first is done", async () => {
    const first = loadSnapshot();
    await settle();
    const second = loadSnapshot();
    const third = loadSnapshot();
    await settle();
    expect(reads()).toBe(1); // nothing new starts while the first is in flight.

    held[0].resolve("ja");
    await first;
    await settle();
    expect(reads()).toBe(2); // one read for both of the calls that waited.

    held[1].resolve("en");
    await Promise.all([second, third]);
    expect(getSnapshot().language).toBe("en");
    expect(reads()).toBe(2);
  });

  it("the last call wins, even when the read before it would have answered last", async () => {
    const first = loadSnapshot();
    await settle();
    const second = loadSnapshot();

    held[0].resolve("old");
    await first;
    await settle();
    held[1].resolve("new");
    await second;

    expect(getSnapshot().language).toBe("new");
  });

  it("a read that fails does not keep the one queued behind it from running", async () => {
    const first = loadSnapshot();
    await settle();
    const second = loadSnapshot();

    held[0].reject(new Error("store_busy"));
    await expect(first).rejects.toThrow("store_busy");
    await settle();
    held[1].resolve("after");
    await second;

    expect(getSnapshot().language).toBe("after");
  });

  it("a queued read counts as inbox-affecting when any call it stands for did", async () => {
    const first = loadSnapshot({ inboxAffected: false });
    await settle();
    const second = loadSnapshot({ inboxAffected: false });
    const third = loadSnapshot(); // inbox-affecting by default
    held[0].resolve("ja");
    await first;
    await settle();
    const before = getInboxDataGeneration();
    held[1].resolve("ja");
    await Promise.all([second, third]);

    expect(getInboxDataGeneration()).toBe(before + 1);
  });

  it("a call made once everything is done starts a read of its own", async () => {
    const first = loadSnapshot();
    await settle();
    held[0].resolve("ja");
    await first;

    const again = loadSnapshot();
    await settle();
    expect(reads()).toBe(2);
    held[1].resolve("ja");
    await again;
  });
});
