// @vitest-environment jsdom
// The plugin update banner is the quiet half of `AMB-D-359`: it offers the take-it button for what needs no
// decision, names what does and only then points at a screen, and stays dismissed per **build** rather than
// per plugin. These tests hold it to exactly that — the seam to core is stubbed, so what is under test is the
// judgement about what to show and what to call.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PluginUpdate } from "../core/pluginUpdates";

const hoisted = vi.hoisted(() => ({
  updates: [] as PluginUpdate[],
  applied: [] as string[],
  appliedAll: 0,
  refreshed: 0,
}));

vi.mock("../core/pluginUpdates", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/pluginUpdates")>();
  return {
    ...orig,
    usePluginUpdates: () => ({ updates: hoisted.updates, loading: false, error: undefined }),
    refreshPluginUpdates: () => { hoisted.refreshed++; },
    applyPluginUpdate: (name: string) => { hoisted.applied.push(name); return Promise.resolve(true); },
    applyAllPluginUpdates: () => {
      hoisted.appliedAll++;
      return Promise.resolve(hoisted.updates.map((u) => ({ name: u.name, applied: true })));
    },
  };
});

import { PluginUpdateBanner } from "./PluginUpdateBanner";
import { clearDismissedPluginUpdates } from "../core/pluginUpdates";
import { t, tn, tf } from "../core/i18n";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
let opened = 0;

const offer = (over: Partial<PluginUpdate> & { name: string }): PluginUpdate => ({
  desc: "does a thing",
  availableChecksum: "sum-1",
  missing: [],
  ...over,
});

// Exact, minus the ✕ the close button leads with: "update" is a prefix of "update all", and a loose match
// would have the two tests about them pass on each other's button.
const button = (label: string) =>
  Array.from(container.querySelectorAll("button")).find(
    (b) => (b.textContent ?? "").replace("✕", "").trim() === label,
  );

beforeEach(() => {
  hoisted.updates = [];
  hoisted.applied = [];
  hoisted.appliedAll = 0;
  hoisted.refreshed = 0;
  opened = 0;
  localStorage.clear();
  clearDismissedPluginUpdates();
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

const render = () =>
  act(() =>
    root.render(
      createElement(PluginUpdateBanner, { onOpenInstalled: () => { opened++; } }),
    ),
  );

describe("offering what is waiting", () => {
  // Nothing waiting is nothing said: the banner is the standing offer, not a status line.
  it("draws nothing when no update is waiting", () => {
    render();
    expect(container.textContent).toBe("");
  });

  it("takes one update from the banner itself, with no screen in between", async () => {
    hoisted.updates = [offer({ name: "worktree" })];
    render();

    expect(container.textContent).toContain(tn("plugins.updates.title", 1));
    expect(container.textContent).toContain("worktree");
    await act(async () => { button(t("plugins.updates.apply"))!.click(); });
    expect(hoisted.applied).toEqual(["worktree"]);
    expect(container.textContent).toContain(tf("plugins.updates.applied", { count: 1 }));
  });

  it("offers one button for several", async () => {
    hoisted.updates = [offer({ name: "worktree" }), offer({ name: "notify" })];
    render();

    expect(button(t("plugins.updates.apply"))).toBeUndefined();
    await act(async () => { button(t("plugins.updates.applyAll"))!.click(); });
    expect(hoisted.appliedAll).toBe(1);
    expect(container.textContent).toContain(tf("plugins.updates.applied", { count: 2 }));
  });
});

describe("what needs a decision", () => {
  // The whole point of the split (`AMB-D-359`): a screen is offered only when there is something to resolve
  // there, and an update that cannot simply be taken is never dressed up as one that can.
  it("names an unsatisfied setting and points at the screen instead of an apply button", () => {
    hoisted.updates = [offer({ name: "notify", hold: "settings", missing: ["token"] })];
    render();

    expect(container.textContent).toContain(
      tf("plugins.updates.holdSettings", { name: "notify", keys: "token" }),
    );
    expect(button(t("plugins.updates.apply"))).toBeUndefined();
    act(() => { button(t("plugins.updates.open"))!.click(); });
    expect(opened).toBe(1);
  });

  it("says an incompatible build cannot run here", () => {
    hoisted.updates = [offer({ name: "notify", hold: "incompatible" })];
    render();

    expect(container.textContent).toContain(
      tf("plugins.updates.holdIncompatible", { name: "notify" }),
    );
    expect(button(t("plugins.updates.applyAll"))).toBeUndefined();
  });

  // A held one must not silence the rest: what can be taken is still offered beside it.
  it("still offers the ones that can just be taken", () => {
    hoisted.updates = [
      offer({ name: "notify", hold: "settings", missing: ["token"] }),
      offer({ name: "worktree" }),
    ];
    render();

    expect(button(t("plugins.updates.apply"))).toBeDefined();
    expect(button(t("plugins.updates.open"))).toBeDefined();
  });
});

describe("dismissing an offer", () => {
  it("stays quiet for the build dismissed, and returns for the next one", () => {
    hoisted.updates = [offer({ name: "worktree", availableChecksum: "sum-1" })];
    render();
    act(() => { button(t("health.dismiss"))!.click(); });
    expect(container.textContent).toBe("");

    // The same plugin, a build further on: a dismissal is about the build offered, not about the plugin.
    hoisted.updates = [offer({ name: "worktree", availableChecksum: "sum-2" })];
    render();
    expect(container.textContent).toContain("worktree");
  });

  it("obeys a dismissal cleared from elsewhere (the explicit check)", () => {
    hoisted.updates = [offer({ name: "worktree" })];
    render();
    act(() => { button(t("health.dismiss"))!.click(); });
    expect(container.textContent).toBe("");

    act(() => clearDismissedPluginUpdates());
    expect(container.textContent).toContain("worktree");
  });
});
