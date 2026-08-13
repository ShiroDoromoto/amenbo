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
  /** How many times the detail sent the reader to the installed screen. */
  wentToInstalled: 0,
  /** What a disable answers it threw away — zero unless a test is about the discard. */
  droppedQueued: 0,
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

/** One installed plugin; `on` names the projects it fires in, as rows of those crossings. */
const row = ({ on = [], ...over }: Partial<PluginInstall> & { name: string; on?: number[] }): PluginInstall => ({
  compatible: true,
  projects: on.map((project) => ({ project, enabled: true, hasValue: false, requiredUnset: false })),
  config: [],
  scope: "project",
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
  hoisted.wentToInstalled = 0;
  hoisted.projects = [];
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
      createElement(PluginMarketScreen, { onOpenInstalled: () => hoisted.wentToInstalled++ }),
    ),
  );

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

    hoisted.installs = [row({ name: "notify", on: [7] })];
    render();
    expect(container.textContent).toContain(t("plugins.enabledChip"));
  });

});

// Installing is aimed at no project (`AMB-D-412`), so the face that installs never asks about one — not
// even after the fact, by turning the button that was pressed into the switch where it stood.
describe("what the install face says once a plugin has landed", () => {
  it("says it is here and runs nothing, and offers the one way on", async () => {
    hoisted.projects = [{ id: 1, name: "alpha" }, { id: 2, name: "beta" }];
    hoisted.installs = [row({ name: "notify" })];
    render();
    open(0);

    expect(detail()!.textContent).toContain(t("plugins.installed"));
    expect(detail()!.textContent).toContain(t("plugins.landedInert"));
    // No project is asked for here, and no switch is moved from here.
    expect(detail()!.querySelector("select")).toBeNull();

    await act(async () => { button(t("plugins.turnItOn"))!.click(); });
    expect(hoisted.wentToInstalled).toBe(1);
    expect(hoisted.gated).toEqual([]);
  });

  // Where it is on is the row's to say (`AMB-D-412`); this face is about what exists and what is here.
  it("says the same thing for a plugin already on somewhere", () => {
    hoisted.projects = [{ id: 1, name: "alpha" }];
    hoisted.installs = [row({ name: "notify", on: [1] })];
    render();
    open(0);
    expect(detail()!.textContent).toContain(t("plugins.landedInert"));
    expect(detail()!.querySelector("select")).toBeNull();
  });
});

// The catalog's second half (`AMB-D-385`): the list carries what a row draws, and what installing one
// would actually mean is fetched for the one entry someone opened. It is read before an install, which is
// the point — a plugin that will want a credential, or that this build cannot run, says so first.
describe("what the opened entry says installing it would mean", () => {
  const doc = (over: Partial<PluginDetail> = {}): PluginDetail => ({
    events: [],
    config: [],
    scope: "project",
    compatible: true,
    ...over,
  });

  it("names what it watches, and what it will ask to be told", () => {
    hoisted.detail = doc({
      events: ["task.created", "task.completed"],
      config: [
        { key: "webhook", label: "Webhook URL", secret: true, required: true, readonly: false, fieldType: "text", options: [] },
        { key: "events", label: "Which events", secret: false, required: false, readonly: false, fieldType: "text", options: [] },
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

  // The layer is the author's declaration, and the face it matters on is this one: after the install the
  // gate *is* the consent, so a device-wide plugin has to be readable as such before it is taken on
  // (`AMB-D-601`). It replaces the per-project line rather than joining it — the two cannot both be true,
  // and a reader shown both would be reading a switch that does not exist.
  it("says a device-wide plugin reads every project, in place of the per-project line", () => {
    hoisted.detail = doc({ scope: "machine" });
    render();
    open(0);

    expect(detail()!.textContent).toContain(t("plugins.scope.machine"));
    expect(detail()!.textContent).not.toContain(t("plugins.want.perProject"));
    // Said, not offered: nothing was added for a reader to set, and the install stays the one act here.
    expect(detail()!.querySelector("input")).toBeNull();
    expect(button(t("plugins.install"))!.disabled).toBe(false);
  });

  // The default, and the one the three plugins already published rely on: an author who wrote no `scope`
  // gets the face they always had.
  it("leaves the ordinary plugin's line alone", () => {
    hoisted.detail = doc();
    render();
    open(0);

    expect(detail()!.textContent).toContain(t("plugins.want.perProject"));
    expect(detail()!.textContent).not.toContain(t("plugins.scope.machine"));
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
