// @vitest-environment jsdom
// Install and enable are two acts, and the second one asks first (`AMB-D-351`). These tests hold the market
// to that: installing writes a plugin that fires nothing, the first enable stops at the consent question and
// only the answer moves the gate, and a plugin whose switch is one project's cannot be moved until a project
// is named (`AMB-D-379`).
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PluginCatalog, PluginDetail } from "../core/pluginCatalog";
import type { PluginInstall } from "../core/pluginInstalls";

const hoisted = vi.hoisted(() => ({
  catalog: { entries: [], sources: [], dropped: 0 } as PluginCatalog,
  /** The catalog's detail document for the opened entry — `null` where the official index has none. */
  detail: null as PluginDetail | null,
  installs: [] as PluginInstall[],
  installed: [] as string[],
  gated: [] as { name: string; projectId: number | null; enabled: boolean }[],
  enableFails: null as string | null,
  projects: [] as { id: number; name: string }[],
}));

vi.mock("../core/pluginCatalog", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/pluginCatalog")>();
  return {
    ...orig,
    usePluginCatalog: () => ({ catalog: hoisted.catalog, loading: false, error: undefined }),
    usePluginRepoFacts: () => ({ facts: undefined, loading: false, error: undefined }),
    usePluginDetail: () => ({ detail: hoisted.detail, loading: false, error: undefined }),
  };
});

// Only the seam that talks to core is replaced: which rows exist, and what the two writes were called with.
vi.mock("../core/pluginInstalls", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/pluginInstalls")>();
  return {
    ...orig,
    usePluginInstalls: () => ({ installs: hoisted.installs, loading: false, error: undefined }),
    installPlugin: (name: string) => {
      hoisted.installed.push(name);
      return Promise.resolve(null);
    },
    setPluginEnabled: (name: string, projectId: number | null, enabled: boolean) => {
      if (hoisted.enableFails) return Promise.reject(hoisted.enableFails);
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

import { PluginMarketScreen } from "./PluginMarketScreen";
import { t, tf } from "../core/i18n";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

const entry = (name: string) => ({
  name,
  desc: `${name} does a thing`,
  author: "someone",
  repo: `owner/${name}`,
  os: ["macos"],
  category: "workflow",
  official: false,
  listed: true,
  featured: false,
});

const row = (over: Partial<PluginInstall> & { name: string }): PluginInstall => ({
  scope: "machine",
  consented: false,
  compatible: true,
  config: [],
  rollback: false,
  ...over,
});

const buttons = () => Array.from(container.querySelectorAll("button"));
const button = (label: string) => buttons().find((b) => b.textContent === label);
const detail = () => container.querySelector(".plugdet");
const open = (i: number) =>
  act(() => (Array.from(container.querySelectorAll(".feed__item"))[i] as HTMLElement).click());

beforeEach(() => {
  hoisted.catalog = { entries: [entry("notify")], sources: [], dropped: 0 };
  hoisted.detail = null;
  hoisted.installs = [];
  hoisted.installed = [];
  hoisted.gated = [];
  hoisted.enableFails = null;
  hoisted.projects = [];
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

const render = () => act(() => root.render(createElement(PluginMarketScreen)));

describe("installing from the market", () => {
  it("offers an install for an entry this machine does not hold, and says it will not run", async () => {
    render();
    open(0);
    expect(detail()!.textContent).toContain(t("plugins.installNote"));

    await act(async () => { button(t("plugins.install"))!.click(); });
    expect(hoisted.installed).toEqual(["notify"]);
  });

  // An installed plugin that fires nothing is the ordinary state, so the row says which of the two it is.
  it("badges an installed row, and an enabled one differently", () => {
    hoisted.installs = [row({ name: "notify" })];
    render();
    expect(container.textContent).toContain(t("plugins.installed"));
    expect(container.textContent).not.toContain(t("plugins.enabledChip"));

    hoisted.installs = [row({ name: "notify", enabled: true, consented: true })];
    render();
    expect(container.textContent).toContain(t("plugins.enabledChip"));
  });

  it("shows what core refused with, rather than swallowing it", async () => {
    hoisted.installs = [row({ name: "notify", consented: true })];
    hoisted.enableFails = "plugin_signature_invalid";
    render();
    open(0);
    await act(async () => { button(t("plugins.enable"))!.click(); });
    expect(detail()!.textContent).toContain("plugin_signature_invalid");
  });
});

describe("the consent the first enable asks for", () => {
  it("asks before the first enable, and moves the gate only on the answer", async () => {
    hoisted.installs = [row({ name: "notify", enabled: false })];
    render();
    open(0);

    act(() => { button(t("plugins.enable"))!.click(); });
    // The question is on screen and nothing has been enabled yet.
    expect(detail()!.textContent).toContain(tf("plugins.consentAsk", { name: "notify" }));
    expect(hoisted.gated).toEqual([]);

    await act(async () => { button(t("plugins.consentAgree"))!.click(); });
    expect(hoisted.gated).toEqual([{ name: "notify", projectId: null, enabled: true }]);
  });

  it("backing out of the question enables nothing", () => {
    hoisted.installs = [row({ name: "notify" })];
    render();
    open(0);
    act(() => { button(t("plugins.enable"))!.click(); });
    act(() => { button(t("plugins.consentCancel"))!.click(); });
    expect(hoisted.gated).toEqual([]);
    expect(detail()!.textContent).not.toContain(tf("plugins.consentAsk", { name: "notify" }));
  });

  // Asked once per device (`AMB-D-351`): a plugin this machine already answered for goes straight through,
  // and disabling never asks at all.
  it("does not ask again once this device has consented", async () => {
    hoisted.installs = [row({ name: "notify", consented: true })];
    render();
    open(0);
    await act(async () => { button(t("plugins.enable"))!.click(); });
    expect(hoisted.gated).toEqual([{ name: "notify", projectId: null, enabled: true }]);
  });

  it("disables without a question", async () => {
    hoisted.installs = [row({ name: "notify", enabled: true, consented: true })];
    render();
    open(0);
    await act(async () => { button(t("plugins.disable"))!.click(); });
    expect(hoisted.gated).toEqual([{ name: "notify", projectId: null, enabled: false }]);
  });
});

describe("the one switch, at the level its author declared", () => {
  // A project-scoped gate has no device-wide answer to fall back on, so the market names the project first.
  it("waits for a project before it will move a project-scoped gate", async () => {
    hoisted.projects = [{ id: 1, name: "alpha" }, { id: 2, name: "beta" }];
    hoisted.installs = [row({ name: "notify", scope: "project", consented: true })];
    render();
    open(0);

    expect(detail()!.textContent).toContain(t("plugins.pickProjectNote"));
    expect(button(t("plugins.enable"))!.disabled).toBe(true);

    const picker = detail()!.querySelector("select") as HTMLSelectElement;
    act(() => {
      picker.value = "2";
      picker.dispatchEvent(new Event("change", { bubbles: true }));
    });
    await act(async () => { button(t("plugins.enable"))!.click(); });
    expect(hoisted.gated).toEqual([{ name: "notify", projectId: 2, enabled: true }]);
  });

  // One project is not a question worth asking: there is exactly one answer, so the gate is movable at once.
  it("takes the only project there is without asking", async () => {
    hoisted.projects = [{ id: 7, name: "solo" }];
    hoisted.installs = [row({ name: "notify", scope: "project", consented: true, enabled: false })];
    render();
    open(0);
    expect(detail()!.textContent).not.toContain(t("plugins.pickProjectNote"));
    await act(async () => { button(t("plugins.enable"))!.click(); });
    expect(hoisted.gated).toEqual([{ name: "notify", projectId: 7, enabled: true }]);
  });

  // An open gate on a build amenbo cannot speak to fires nothing, so the switch says why instead of moving.
  it("refuses to enable a plugin this build cannot speak to, and names the mismatch", () => {
    hoisted.installs = [
      row({ name: "notify", consented: true, compatible: false, incompatibleReason: "needs amenbo 9.0" }),
    ];
    render();
    open(0);
    expect(detail()!.textContent).toContain("needs amenbo 9.0");
    expect(button(t("plugins.enable"))!.disabled).toBe(true);
  });
});

// The catalog's second half (`AMB-D-385`): the list carries what a row draws, and what installing one
// would actually mean is fetched for the one entry someone opened. It is read before an install, which is
// the point — a plugin that will want a credential, or that this build cannot run, says so first.
describe("what the opened entry says installing it would mean", () => {
  const doc = (over: Partial<PluginDetail> = {}): PluginDetail => ({
    scope: "machine",
    events: [],
    config: [],
    compatible: true,
    ...over,
  });

  it("names the switch it gets, what it watches, and what it will ask to be told", () => {
    hoisted.detail = doc({
      scope: "project",
      events: ["task.created", "task.completed"],
      config: [
        { key: "webhook", label: "Webhook URL", secret: true, required: true },
        { key: "events", label: "Which events", secret: false, required: false },
      ],
    });
    render();
    open(0);

    expect(detail()!.textContent).toContain(t("plugins.want.perProject"));
    expect(detail()!.textContent).toContain(
      tf("plugins.want.events", { events: "task.created, task.completed" }),
    );
    expect(detail()!.textContent).toContain("Webhook URL");
    // A secret is the line worth seeing before installing: it means handing over a credential.
    expect(detail()!.textContent).toContain(t("plugins.want.secret"));
    expect(detail()!.textContent).toContain(t("plugins.cfg.required"));
  });

  // Compatibility is enforced at the gate that fires the plugin, so this warns and leaves the choice.
  it("says a build this amenbo cannot run, without taking the install away", () => {
    hoisted.detail = doc({ compatible: false, incompatibleReason: "needs amenbo 9.0" });
    render();
    open(0);

    expect(detail()!.textContent).toContain("needs amenbo 9.0");
    expect(button(t("plugins.install"))!.disabled).toBe(false);
  });

  // An entry only a third-party index offers has no detail on the official one — and nothing to say.
  it("draws the entry unchanged when the official catalog has no detail for it", () => {
    hoisted.detail = null;
    render();
    open(0);

    expect(detail()!.textContent).toContain("notify");
    expect(detail()!.textContent).not.toContain(t("plugins.want.perDevice"));
  });
});
