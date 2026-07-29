// @vitest-environment jsdom
// Install and enable are two acts (`AMB-D-351`). These tests hold the market to that: installing writes a
// plugin that fires nothing, enabling is the separate act that lets it run, and the switch cannot be moved
// until a project is named (`AMB-D-434`).
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
  /** What a disable answers it threw away — zero unless a test is about the discard. */
  droppedQueued: 0,
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
    setPluginEnabled: (name: string, projectId: number, enabled: boolean) => {
      if (hoisted.enableFails) return Promise.reject(hoisted.enableFails);
      hoisted.gated.push({ name, projectId, enabled });
      return Promise.resolve({ enabled, droppedQueued: hoisted.droppedQueued });
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
  source: "https://official",
  sourceName: "amenbo",
  featured: false,
});

const row = (over: Partial<PluginInstall> & { name: string }): PluginInstall => ({
  compatible: true,
  enabledProjects: [],
  config: [],
  rollback: false,
  ...over,
});

const buttons = () => Array.from(container.querySelectorAll("button"));
const button = (label: string) => buttons().find((b) => b.textContent === label);
/** The gate's own select: picking a project on it is the enable. */
const gatePicker = () => container.querySelector<HTMLSelectElement>(".pluggate select")!;
/** What a select offers, in order. */
const options = (el: HTMLSelectElement) => Array.from(el.options).map((o) => o.textContent);
const select = (el: HTMLSelectElement, value: string) => {
  el.value = value;
  el.dispatchEvent(new Event("change", { bubbles: true }));
};
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

    hoisted.installs = [row({ name: "notify", enabledProjects: [7] })];
    render();
    expect(container.textContent).toContain(t("plugins.enabledChip"));
  });

  it("shows what core refused with, rather than swallowing it", async () => {
    hoisted.projects = [{ id: 7, name: "solo" }];
    hoisted.installs = [row({ name: "notify" })];
    hoisted.enableFails = "plugin_signature_invalid";
    render();
    open(0);
    await act(async () => { select(gatePicker(), "7"); });
    expect(detail()!.textContent).toContain("plugin_signature_invalid");
  });
});

describe("the one switch, and it is a project's", () => {
  // Picking the project is the enable (`AMB-D-434`): turning a plugin on is itself the permission to run
  // its code, so the switch moves on the pick rather than stopping to ask a second question.
  it("enables the project picked, with nothing else asked", async () => {
    hoisted.projects = [{ id: 1, name: "alpha" }, { id: 2, name: "beta" }];
    hoisted.installs = [row({ name: "notify" })];
    render();
    open(0);
    await act(async () => { select(gatePicker(), "2"); });
    expect(hoisted.gated).toEqual([{ name: "notify", projectId: 2, enabled: true }]);
  });

  it("turns one off from beside the project it is on in", async () => {
    hoisted.projects = [{ id: 7, name: "solo" }];
    hoisted.installs = [row({ name: "notify", enabledProjects: [7] })];
    render();
    open(0);
    await act(async () => { button(t("plugins.disable"))!.click(); });
    expect(hoisted.gated).toEqual([{ name: "notify", projectId: 7, enabled: false }]);
  });

  // The detail draws the same control the installed screen does (`AMB-D-412`), so it says where the
  // plugin is on without being told which project to look through.
  it("names the projects it is on in, and offers the rest", () => {
    hoisted.projects = [{ id: 1, name: "alpha" }, { id: 2, name: "beta" }];
    hoisted.installs = [row({ name: "notify", enabledProjects: [1] })];
    render();
    open(0);
    expect(detail()!.textContent).toContain("alpha");
    expect(options(gatePicker())).toEqual([t("plugins.gate.addProject"), "beta"]);
  });

  it("says so for a plugin that is on nowhere", () => {
    hoisted.projects = [{ id: 7, name: "solo" }];
    hoisted.installs = [row({ name: "notify" })];
    render();
    open(0);
    expect(detail()!.textContent).toContain(t("plugins.gate.offEverywhere"));
  });

  // An open gate on a build amenbo cannot speak to fires nothing, so the switch says why instead of moving.
  it("refuses to enable a plugin this build cannot speak to, and names the mismatch", () => {
    hoisted.projects = [{ id: 7, name: "solo" }];
    hoisted.installs = [
      row({ name: "notify", compatible: false, incompatibleReason: "needs amenbo 9.0" }),
    ];
    render();
    open(0);
    expect(detail()!.textContent).toContain("needs amenbo 9.0");
    expect(gatePicker().disabled).toBe(true);
  });
});

// The catalog's second half (`AMB-D-385`): the list carries what a row draws, and what installing one
// would actually mean is fetched for the one entry someone opened. It is read before an install, which is
// the point — a plugin that will want a credential, or that this build cannot run, says so first.
describe("what the opened entry says installing it would mean", () => {
  const doc = (over: Partial<PluginDetail> = {}): PluginDetail => ({
    events: [],
    config: [],
    compatible: true,
    ...over,
  });

  it("names what it watches, and what it will ask to be told", () => {
    hoisted.detail = doc({
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
    expect(detail()!.textContent).not.toContain(t("plugins.want.perProject"));
  });
});
