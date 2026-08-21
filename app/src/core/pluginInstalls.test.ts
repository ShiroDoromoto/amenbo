// What a press of the author's own operation leaves behind on screen (`AMB-D-664`). The values it asked for
// go nowhere, but the run itself may write — an operation saves through `plugin config set` (`AMB-D-406`),
// and the viewer's `setup` writes three settings back that way — so what the layer holds has to be read
// again afterwards, exactly as a write of our own is. These pin that refetch, including on the run that
// came back saying no: a plugin can write and then fail, and the form must not keep drawing "unset".
import { beforeEach, describe, expect, it, vi } from "vitest";

// `inTauri()` is only true inside the webview; without this every seam here short-circuits to its mock.
(globalThis as unknown as { window: unknown }).window = { __TAURI_INTERNALS__: {} };

/** What the boundary was asked, newest last, and what it is told to answer. */
const invoked: { cmd: string; args: unknown }[] = [];
let answer: unknown = { ok: true };
vi.mock("./ipc", () => ({
  invoke: (cmd: string, args: unknown) => {
    invoked.push({ cmd, args });
    return Promise.resolve(answer);
  },
}));

// The query layer, stubbed down to "an invalidation was asked for, on which keys" — what it does with one
// is its own tests'; which reads go stale after a press is what these are about.
const invalidated: string[][] = [];
vi.mock("./query", () => ({
  invalidateQueries: (pred: (key: string[]) => boolean) => {
    for (const key of [["plugin-config", "viewer", null], ["plugin-installs", "en"]]) {
      if (pred(key as string[])) invalidated.push(key as string[]);
    }
  },
  useQuery: () => ({ data: undefined, loading: false, error: undefined }),
}));

import { runPluginAction } from "./pluginInstalls";

beforeEach(() => {
  invoked.length = 0;
  invalidated.length = 0;
  answer = { ok: true };
});

describe("runPluginAction", () => {
  it("names the declaration and passes what the press asked for, at the layer it was pressed at", async () => {
    await runPluginAction("viewer", "config setup", { api_token: "t0ken" }, null);
    expect(invoked).toEqual([
      {
        cmd: "plugin_settings_action",
        args: { name: "viewer", cmd: "config setup", supplied: { api_token: "t0ken" }, projectId: null },
      },
    ]);
  });

  it("reads back what the layer holds, since the run may have written to it", async () => {
    await runPluginAction("viewer", "config setup", {}, null);
    expect(invalidated).toEqual([["plugin-config", "viewer", null]]);
  });

  it("reads it back even when the run answered no — it may have written before it failed", async () => {
    answer = { ok: false, message: "no worker" };
    const outcome = await runPluginAction("viewer", "config setup", {}, null);
    expect(outcome).toEqual({ ok: false, message: "no worker" });
    expect(invalidated).toEqual([["plugin-config", "viewer", null]]);
  });
});
