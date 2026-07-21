// The in-app self-update: `installUpdate` drives the Tauri updater — check the manifest, then download +
// verify + install a newer signed build — and reports progress the banner draws. Pinned here: it returns false (so the
// banner falls back to opening the installer) when the manifest offers nothing, and on an offer it walks the phases
// checking → downloading (with bytes) → installing → ready and returns true. The plugin is stubbed; the real download
// and minisign verification are the plugin's, not ours.
import { beforeEach, describe, it, expect, vi } from "vitest";

// `inTauri()` is only true inside the webview; the test stubs the host so mutations believes it is there.
(globalThis as unknown as { window: unknown }).window = { __TAURI_INTERNALS__: {} };

/** What `check()` resolves to. Each test rewrites this: null = no update, or a fake Update whose downloadAndInstall
 *  replays a fixed event stream. */
let update: unknown = null;
const check = vi.fn(() => Promise.resolve(update));
vi.mock("@tauri-apps/plugin-updater", () => ({ check: () => check() }));

import { installUpdate, type UpdateProgress } from "./mutations";

/** A fake Update that replays `events` through the caller's progress callback, as the plugin would. */
function fakeUpdate(events: unknown[]) {
  return {
    downloadAndInstall: async (onEvent: (e: unknown) => void) => {
      for (const e of events) onEvent(e);
    },
  };
}

beforeEach(() => {
  check.mockClear();
  update = null;
});

describe("installUpdate", () => {
  it("returns false when the updater manifest offers nothing newer", async () => {
    update = null;
    const seen: UpdateProgress[] = [];
    const applied = await installUpdate((p) => seen.push(p));
    expect(applied).toBe(false);
    expect(check).toHaveBeenCalledOnce();
    expect(seen).toEqual([{ phase: "checking" }]); // it checks, then stops — nothing to download.
  });

  it("walks the phases and returns true on an offer, carrying bytes while it downloads", async () => {
    update = fakeUpdate([
      { event: "Started", data: { contentLength: 200 } },
      { event: "Progress", data: { chunkLength: 50 } },
      { event: "Progress", data: { chunkLength: 150 } },
      { event: "Finished" },
    ]);
    const seen: UpdateProgress[] = [];
    const applied = await installUpdate((p) => seen.push(p));
    expect(applied).toBe(true);
    expect(seen).toEqual([
      { phase: "checking" },
      { phase: "downloading", downloaded: 0, total: 200 },
      { phase: "downloading", downloaded: 50, total: 200 },
      { phase: "downloading", downloaded: 200, total: 200 },
      { phase: "installing" },
      { phase: "ready" },
    ]);
  });

  it("reports an indeterminate download when the manifest carried no length", async () => {
    update = fakeUpdate([
      { event: "Started", data: {} }, // no contentLength.
      { event: "Progress", data: { chunkLength: 10 } },
      { event: "Finished" },
    ]);
    const seen: UpdateProgress[] = [];
    await installUpdate((p) => seen.push(p));
    expect(seen).toContainEqual({ phase: "downloading", downloaded: 0, total: null });
    expect(seen).toContainEqual({ phase: "downloading", downloaded: 10, total: null });
  });
});
