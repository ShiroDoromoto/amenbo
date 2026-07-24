// @vitest-environment jsdom
// The installed screen answers "what does this machine hold, and is it firing" (`AMB-D-351`) without ever
// reading the catalog. These tests hold it to that: every install is listed with the switch its author
// declared, the switch moves from here under the same consent the market asks, and a project-scoped gate
// still waits to be told which project it speaks for (`AMB-D-379`).
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PluginInstall } from "../core/pluginInstalls";

const hoisted = vi.hoisted(() => ({
  installs: [] as PluginInstall[],
  loading: false,
  error: undefined as unknown,
  gated: [] as { name: string; projectId: number | null; enabled: boolean }[],
  projects: [] as { id: number; name: string }[],
}));

// Only the seam that talks to core is replaced: which rows exist, and what the write was called with.
vi.mock("../core/pluginInstalls", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/pluginInstalls")>();
  return {
    ...orig,
    usePluginInstalls: () => ({
      installs: hoisted.installs,
      loading: hoisted.loading,
      error: hoisted.error,
    }),
    setPluginEnabled: (name: string, projectId: number | null, enabled: boolean) => {
      hoisted.gated.push({ name, projectId, enabled });
      return Promise.resolve(enabled);
    },
  };
});

vi.mock("../core/snapshot", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/snapshot")>();
  return {
    ...orig,
    subscribe: () => () => {},
    getSnapshot: () => ({ ...orig.getSnapshot(), projects: hoisted.projects }),
  };
});

import { PluginInstalledScreen } from "./PluginInstalledScreen";
import { t, tf } from "../core/i18n";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

const row = (over: Partial<PluginInstall> & { name: string }): PluginInstall => ({
  scope: "machine",
  consented: false,
  compatible: true,
  ...over,
});

const button = (label: string) =>
  Array.from(container.querySelectorAll("button")).find((b) => b.textContent === label);
const rows = () => Array.from(container.querySelectorAll(".feed__item"));

beforeEach(() => {
  hoisted.installs = [];
  hoisted.loading = false;
  hoisted.error = undefined;
  hoisted.gated = [];
  hoisted.projects = [];
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

const render = () => act(() => root.render(createElement(PluginInstalledScreen)));

describe("what this machine holds", () => {
  it("lists every install, and says which switch each one has", () => {
    hoisted.projects = [{ id: 1, name: "alpha" }];
    hoisted.installs = [
      row({ name: "notify" }),
      row({ name: "worktree", scope: "project", consented: true, enabled: true }),
    ];
    render();

    expect(rows()).toHaveLength(2);
    expect(container.textContent).toContain("notify");
    expect(container.textContent).toContain("worktree");
    expect(container.textContent).toContain(t("plugins.gate.machine"));
    expect(container.textContent).toContain(t("plugins.gate.project"));
    expect(container.textContent).toContain(tf("plugins.installedCount", { count: 2 }));
    // Enabled is the fact worth badging; installed is what every row on this screen already is.
    expect(container.textContent).toContain(t("plugins.enabledChip"));
  });

  // Nothing installed is not an error, and the way out of it is the other tab.
  it("points at the market when there is nothing here", () => {
    render();
    expect(container.textContent).toContain(t("plugins.emptyInstalled"));
    expect(container.textContent).toContain(t("plugins.emptyInstalledNote"));
    expect(rows()).toHaveLength(0);
  });

  it("says so when the installs could not be read", () => {
    hoisted.error = new Error("nope");
    render();
    expect(container.textContent).toContain(t("plugins.installsError"));
  });
});

describe("moving a gate from the list", () => {
  it("asks for the consent the first time, and moves the gate on the answer", async () => {
    hoisted.installs = [row({ name: "notify" })];
    render();

    act(() => { button(t("plugins.enable"))!.click(); });
    expect(container.textContent).toContain(tf("plugins.consentAsk", { name: "notify" }));
    expect(hoisted.gated).toEqual([]);

    await act(async () => { button(t("plugins.consentAgree"))!.click(); });
    expect(hoisted.gated).toEqual([{ name: "notify", projectId: null, enabled: true }]);
  });

  it("disables without a question", async () => {
    hoisted.installs = [row({ name: "notify", consented: true, enabled: true })];
    render();
    await act(async () => { button(t("plugins.disable"))!.click(); });
    expect(hoisted.gated).toEqual([{ name: "notify", projectId: null, enabled: false }]);
  });

  // A project-scoped gate has no device-wide answer to fall back on, so this screen names the project too.
  it("waits for a project before it will move a project-scoped gate", async () => {
    hoisted.projects = [{ id: 1, name: "alpha" }, { id: 2, name: "beta" }];
    hoisted.installs = [row({ name: "worktree", scope: "project", consented: true })];
    render();

    expect(container.textContent).toContain(t("plugins.pickProjectNote"));
    expect(button(t("plugins.enable"))!.disabled).toBe(true);

    const picker = container.querySelector("select") as HTMLSelectElement;
    act(() => {
      picker.value = "2";
      picker.dispatchEvent(new Event("change", { bubbles: true }));
    });
    await act(async () => { button(t("plugins.enable"))!.click(); });
    expect(hoisted.gated).toEqual([{ name: "worktree", projectId: 2, enabled: true }]);
  });

  // An open gate on a build amenbo cannot speak to fires nothing, so the switch says why instead of moving.
  it("refuses to enable a plugin this build cannot speak to, and names the mismatch", () => {
    hoisted.installs = [
      row({ name: "notify", consented: true, compatible: false, incompatibleReason: "needs amenbo 9.0" }),
    ];
    render();
    expect(container.textContent).toContain("needs amenbo 9.0");
    expect(button(t("plugins.enable"))!.disabled).toBe(true);
  });
});
