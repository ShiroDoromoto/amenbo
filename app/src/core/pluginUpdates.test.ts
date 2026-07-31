// How far a plugin-update check goes for its catalog, and who decides (`AMB-D-462`). The trigger decides, but
// it cannot hand its decision to the read directly: the query layer refetches through a fetcher it captured at
// mount, so `refreshPluginUpdates` latches the reach and the fetch it starts spends it. That indirection is
// what these pin — a press that reaches the catalog, an automatic trigger that does not, and the two crossing.
import { beforeEach, describe, expect, it, vi } from "vitest";

// `inTauri()` is only true inside the webview; without this the fetch short-circuits to "nothing waiting".
(globalThis as unknown as { window: unknown }).window = { __TAURI_INTERNALS__: {} };

/** The arguments each `plugin_updates` invocation was made with, newest last. */
const invoked: unknown[] = [];
vi.mock("./ipc", () => ({
  invoke: (_cmd: string, args: unknown) => {
    invoked.push(args);
    return Promise.resolve([]);
  },
}));

// The query layer, stubbed down to "an invalidation was asked for": what it does with one — refetch through
// the fetcher it holds — is what each test then does by hand, since that call is the thing under test.
const invalidated = vi.fn();
vi.mock("./query", () => ({
  invalidateQueries: (pred: unknown) => invalidated(pred),
  useQuery: () => ({ data: undefined, loading: false, error: undefined }),
}));

import { fetchPluginUpdates, refreshPluginUpdates } from "./pluginUpdates";

beforeEach(() => {
  invoked.length = 0;
  invalidated.mockClear();
});

describe("refreshPluginUpdates", () => {
  it("asks for a refetch, and lets an automatic trigger be answered from the cache", async () => {
    refreshPluginUpdates("incidental");
    expect(invalidated).toHaveBeenCalledOnce();
    await fetchPluginUpdates();
    expect(invoked).toEqual([{ reach: "incidental" }]);
  });

  it("sends what a person pressed to the catalog", async () => {
    refreshPluginUpdates("now");
    await fetchPluginUpdates();
    expect(invoked).toEqual([{ reach: "now" }]);
  });

  // Otherwise one press would reach past the window forever: every later focus return would go to the network
  // too, which is the cost the freshness boundary exists to avoid.
  it("spends the press on one read, and is back to the cheap one after it", async () => {
    refreshPluginUpdates("now");
    await fetchPluginUpdates();
    await fetchPluginUpdates();
    expect(invoked).toEqual([{ reach: "now" }, { reach: "incidental" }]);
  });

  // A focus return can land between the press and the read it started. It must not take the reach with it:
  // the person would be answered from the cache, which is exactly the silence the press is meant to break.
  it("keeps the press when an automatic trigger arrives before the read", async () => {
    refreshPluginUpdates("now");
    refreshPluginUpdates("incidental");
    await fetchPluginUpdates();
    expect(invoked).toEqual([{ reach: "now" }]);
  });
});
