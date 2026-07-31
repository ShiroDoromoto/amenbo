// @vitest-environment jsdom
// The DTO can be right (`mismatch` / `legacy` / `pointerMissing` / stale) and the marks still never appear if the
// screen's branching drops them — leaving a broken folder to call itself AI-ready and say nothing. What is
// checked here is the fidelity of row → display.
//
// The folder section is wrapped in `inTauri()`, so we pretend to be inside the Tauri shell. Only the boundaries
// are stubbed (reads, writes, the confirm dialog); the screen's own rendering and branching run for real.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { BoundFolderDto } from "../bindings/bindings";
import type { PluginInstall } from "../core/pluginInstalls";

const hoisted = vi.hoisted(() => ({
  /** The folders the list reads (what `fetchBoundFolders` answers). */
  folders: [] as BoundFolderDto[],
  /** What this machine holds, each row naming the projects it fires in. */
  installs: [] as PluginInstall[],
  /** The gates that were moved, arguments and all. */
  gated: [] as { name: string; projectId: number; enabled: boolean }[],
  /** Canned answers for the confirm dialog, consumed from the front; once exhausted, everything is OK. */
  answers: [] as boolean[],
  /** What this project answered about starting its AI on amenbo — null for never asked. */
  consent: null as boolean | null,
  /** The writes that were called, arguments and all. */
  calls: [] as Array<Array<number | string>>,
}));

vi.mock("../core/snapshot", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/snapshot")>();
  return { ...orig, inTauri: () => true };
});
// The plugin seam, replaced whole: what is installed, and what a moved switch was called with.
vi.mock("../core/pluginInstalls", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/pluginInstalls")>();
  return {
    ...orig,
    usePluginInstalls: () => ({ installs: hoisted.installs, loading: false, error: undefined }),
    // What a project *holds* is core's read, and nothing here is about a value already stored — the
    // crossings under test are the empty ones, so this seam answers with nothing rather than reaching out.
    usePluginConfig: () => ({ fields: [], loading: false }),
    setPluginEnabled: (name: string, projectId: number, enabled: boolean) => {
      hoisted.gated.push({ name, projectId, enabled });
      return Promise.resolve({ enabled, droppedQueued: 0 });
    },
  };
});
vi.mock("../core/dialog", () => ({
  confirmDialog: () => Promise.resolve(hoisted.answers.shift() ?? true),
}));
vi.mock("../core/mutations", () => {
  const record = (name: string) => (...args: Array<number | string>) => {
    hoisted.calls.push([name, ...args]);
    return Promise.resolve();
  };
  return {
    fetchProjectSettings: (id: number) =>
      Promise.resolve({ id, name: "検証PJ", notes: "", color: "#9aa7b2", view: "board", archived: false }),
    fetchBoundFolders: () => Promise.resolve(hoisted.folders),
    bindFolder: record("bindFolder"), unbindFolder: record("unbindFolder"),
    revealFolder: record("revealFolder"), openTerminal: record("openTerminal"),
    pickFolder: () => Promise.resolve(null),
    updateProject: record("updateProject"), deleteProject: record("deleteProject"),
    setProjectArchived: record("setProjectArchived"),
    fetchAgentHookConsent: () => Promise.resolve(hoisted.consent),
    clearAgentHookConsent: (projectId: number) => {
      hoisted.calls.push(["clearAgentHookConsent", projectId]);
      hoisted.consent = null;
      return Promise.resolve();
    },
  };
});

import { ProjectSettingsScreen } from "./ProjectSettingsScreen";
import { t, tf } from "../core/i18n";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

/** One folder with nothing wrong with it: it exists, it has a pointer, and nothing disagrees. */
function folder(over: Partial<BoundFolderDto> = {}): BoundFolderDto {
  return { path: "/w/one", exists: true, mismatch: null, legacy: false, pointerMissing: false, ...over };
}

/** Render the screen and wait until the folder list has hydrated (`fetchBoundFolders`). */
async function render(folders: BoundFolderDto[]) {
  hoisted.folders = folders;
  await act(async () => {
    root.render(createElement(ProjectSettingsScreen, { projectId: 1, onBack: () => {}, onGone: () => {} }));
    await new Promise((r) => setTimeout(r, 0));
  });
}

/** Find a row by path (a row is the `.newproj__field` that holds a `.newproj__folder`). */
function row(path: string): HTMLElement {
  const code = Array.from(container.querySelectorAll("code.newproj__path")).find((c) => c.textContent === path);
  return code!.closest(".newproj__field") as HTMLElement;
}
const warnings = (r: HTMLElement) => Array.from(r.querySelectorAll("[role=alert]")).map((e) => e.textContent ?? "");
const buttons = (r: HTMLElement) => Array.from(r.querySelectorAll("button")).map((b) => b.textContent ?? "");
const button = (r: HTMLElement, text: string) =>
  Array.from(r.querySelectorAll("button")).find((b) => b.textContent?.includes(text));

beforeEach(() => {
  hoisted.folders = [];
  hoisted.installs = [];
  hoisted.consent = null;
  hoisted.gated.length = 0;
  hoisted.answers.length = 0;
  hoisted.calls.length = 0;
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("invariants held by the rows of the linked-folder list", () => {
  it("only a fully healthy row claims \"AI-ready\" and shows no warning or relink", async () => {
    await render([folder()]);
    const r = row("/w/one");
    expect(r.textContent).toContain(t("projset.aiReady"));
    expect(warnings(r)).toEqual([]);
    expect(button(r, t("projset.relink"))).toBeUndefined();
    // It exists, so both ways of opening it are offered.
    expect(button(r, t("newproj.openTerminal"))).toBeDefined();
    expect(button(r, t("newproj.openFinder"))).toBeDefined();
  });

  it("a mismatched row states what conflicts with what and does not drop from the list (id is authoritative)", async () => {
    await render([folder({ mismatch: { projectId: 1, recorded: "old-name", actual: "検証PJ" } })]);
    const r = row("/w/one");
    expect(warnings(r)).toEqual([
      `⚠ ${tf("projset.folderElsewhere", { recorded: "old-name", projectId: 1, actual: "検証PJ" })}`,
    ]);
    expect(button(r, t("projset.relink"))).toBeDefined();
  });

  it("a mismatch whose target has no slug falls back to \"unnamed\" (so the wording is not full of holes)", async () => {
    await render([folder({ mismatch: { projectId: 1, recorded: "old-name", actual: null } })]);
    expect(warnings(row("/w/one"))[0]).toContain(t("projset.folderNoSlug"));
  });

  it("a legacy-format pointer says it is legacy and guides toward relinking", async () => {
    await render([folder({ legacy: true })]);
    const r = row("/w/one");
    expect(warnings(r)).toEqual([`⚠ ${t("projset.folderLegacyPointer")}`]);
    expect(button(r, t("projset.relink"))).toBeDefined();
  });

  it("a row with no pointer does not claim \"AI-ready\"", async () => {
    await render([folder({ pointerMissing: true })]);
    const r = row("/w/one");
    expect(r.textContent).not.toContain(t("projset.aiReady"));
    expect(r.textContent).toContain(t("projset.folderNoPointer"));
    expect(warnings(r)).toEqual([`⚠ ${t("projset.folderNoPointerHint")}`]);
    expect(button(r, t("projset.relink"))).toBeDefined();
  });

  it("a row whose folder is gone says stale and offers neither an open path nor relink (only unbind)", async () => {
    await render([folder({ exists: false })]);
    const r = row("/w/one");
    expect(r.textContent).toContain(t("projset.folderStale"));
    expect(r.textContent).not.toContain(t("projset.aiReady"));
    // A folder that no longer exists cannot be opened, and no pointer can be written into it — unbinding is all that is left.
    expect(buttons(r)).toEqual([t("projset.unbind")]);
  });

  it("relink rewrites a pointer to this project into that folder", async () => {
    await render([folder({ pointerMissing: true })]);
    await act(async () => {
      button(row("/w/one"), t("projset.relink"))!.click();
      await new Promise((r) => setTimeout(r, 0));
    });
    expect(hoisted.calls).toEqual([["bindFolder", 1, "/w/one"]]);
  });

  it("when broken and healthy rows are mixed, each mark appears only on its own row", async () => {
    await render([folder(), folder({ path: "/w/two", legacy: true })]);
    expect(warnings(row("/w/one"))).toEqual([]);
    expect(warnings(row("/w/two"))).toEqual([`⚠ ${t("projset.folderLegacyPointer")}`]);
  });
});

/**
 * One installed plugin, crossing no project until a test names one: `on` is a project it fires in,
 * `filledIn` one that holds a value without the switch being on (`AMB-D-434`).
 */
function install(
  { on = [], filledIn = [], ...over }:
  Partial<PluginInstall> & { name: string; on?: number[]; filledIn?: number[] },
): PluginInstall {
  return {
    compatible: true,
    projects: [
      ...on.map((project) => ({ project, enabled: true, hasValue: false, requiredUnset: false })),
      ...filledIn.map((project) => ({ project, enabled: false, hasValue: true, requiredUnset: false })),
    ],
    config: [],
    rollback: false,
    ...over,
  };
}

/** The plugins section, found by its heading. */
function pluginsSection(): HTMLElement {
  const head = Array.from(container.querySelectorAll(".settings__h")).find(
    (h) => h.textContent === t("projset.plugins"),
  );
  return head!.closest(".settings__section") as HTMLElement;
}
const picker = () => pluginsSection().querySelector("select") as HTMLSelectElement;
const offered = (el: HTMLSelectElement) => Array.from(el.options).map((o) => o.textContent);

/** Pick `value` in the plugins section's picker and let the row it draws settle. */
async function pick(value: string) {
  await act(async () => {
    picker().value = value;
    picker().dispatchEvent(new Event("change", { bubbles: true }));
    await new Promise((r) => setTimeout(r, 0));
  });
}

/** Press the button in the plugins section whose label is `label`, and let the write settle. */
async function press(label: string) {
  const button = Array.from(pluginsSection().querySelectorAll("button")).find(
    (b) => b.textContent === label,
  );
  await act(async () => {
    button!.click();
    await new Promise((r) => setTimeout(r, 0));
  });
}

// The project's own face of the crossings (`AMB-D-447`): the same rows the plugin screen draws, read
// from this end — one project, and the plugins it crosses.
describe("this project's plugin crossings", () => {
  it("lists the ones it crosses, and offers the rest", async () => {
    hoisted.installs = [
      install({ name: "worktree", on: [1, 2] }),
      install({ name: "notify", on: [2] }),
    ];
    await render([]);

    const section = pluginsSection();
    expect(section.textContent).toContain("worktree");
    expect(offered(picker())).toEqual([t("projset.pluginsAdd"), "notify"]);
  });

  // Off is not the same as nothing to say: a value this project holds is a crossing, and hiding it
  // would leave the one place it can be read from the project's side blank.
  it("lists a plugin this project filled in without turning on", async () => {
    hoisted.installs = [install({ name: "notify", filledIn: [1] })];
    await render([]);

    const section = pluginsSection();
    expect(section.textContent).toContain("notify");
    expect(section.textContent).toContain(t("plugins.cfg.filled"));
    expect(section.querySelector("select")).toBeNull();
  });

  it("says so when this project crosses none, without hiding what is installed", async () => {
    hoisted.installs = [install({ name: "notify", on: [2] })];
    await render([]);
    expect(pluginsSection().textContent).toContain(t("projset.pluginsNone"));
    expect(offered(picker())).toEqual([t("projset.pluginsAdd"), "notify"]);
  });

  it("points at the market when this machine holds no plugin at all", async () => {
    await render([]);
    expect(pluginsSection().textContent).toContain(t("plugins.emptyInstalled"));
    expect(pluginsSection().querySelector("select")).toBeNull();
  });

  // Picking draws the crossing; the row's own switch is what runs somebody else's code (`AMB-D-351`).
  it("draws the crossing for the plugin picked, and enables from that row", async () => {
    hoisted.installs = [install({ name: "notify" })];
    await render([]);

    await pick("notify");
    expect(hoisted.gated).toEqual([]);
    expect(pluginsSection().textContent).toContain("notify");

    await press(t("plugins.enable"));
    expect(hoisted.gated).toEqual([{ name: "notify", projectId: 1, enabled: true }]);
  });

  // The mark is readable before anything is pressed, and the settings that answer it open in the row.
  it("marks a crossing short of a required value, and opens its settings in the row", async () => {
    hoisted.installs = [install({
      name: "notify",
      config: [{ key: "webhook_url", label: "Webhook", required: true, secret: false, fieldType: "text", options: [] }],
    })];
    await render([]);

    await pick("notify");
    expect(pluginsSection().textContent).toContain(t("plugins.cfg.requiredEmpty"));
    expect(hoisted.gated).toEqual([]);

    await press(t("plugins.cfg.open"));
    expect(pluginsSection().textContent).toContain("Webhook");
  });

  it("turns one off from the row it is listed on", async () => {
    hoisted.installs = [install({ name: "worktree", on: [1] })];
    await render([]);
    await press(t("plugins.disable"));
    expect(hoisted.gated).toEqual([{ name: "worktree", projectId: 1, enabled: false }]);
  });
});

/** The consent section, found by its heading. */
function harnessSection(): HTMLElement {
  const head = Array.from(container.querySelectorAll(".settings__h")).find(
    (h) => h.textContent === t("projset.harness"),
  );
  return head!.closest(".settings__section") as HTMLElement;
}
const clearButton = () =>
  Array.from(harnessSection().querySelectorAll("button")).find(
    (b) => b.textContent === t("projset.harnessClear"),
  ) as HTMLButtonElement;

// The way back out of an answer that is otherwise final (`AMB-D-459`). What the section is judged on is
// that its three states stay three: a refusal and a project nobody has asked are both silent everywhere
// else, and this is the one screen that tells them apart.
describe("the answer about starting this project's AI on amenbo", () => {
  it("shows a yes as a yes, with the way back out of it offered", async () => {
    hoisted.consent = true;
    await render([]);
    expect(harnessSection().textContent).toContain(t("projset.harnessYes"));
    expect(clearButton().disabled).toBe(false);
  });

  it("shows a refusal as a refusal rather than as silence", async () => {
    hoisted.consent = false;
    await render([]);
    expect(harnessSection().textContent).toContain(t("projset.harnessNo"));
    expect(harnessSection().textContent).not.toContain(t("projset.harnessUnanswered"));
    expect(clearButton().disabled).toBe(false);
  });

  // Never asked is not a no: it is the state the question is put from, so there is nothing to clear.
  it("keeps a project that was never asked apart from one that said no, and offers it no clearing", async () => {
    hoisted.consent = null;
    await render([]);
    expect(harnessSection().textContent).toContain(t("projset.harnessUnanswered"));
    expect(clearButton().disabled).toBe(true);
  });

  it("clears a refusal, landing the screen on never-asked without a reload", async () => {
    hoisted.consent = false;
    await render([]);

    await act(async () => {
      clearButton().click();
      await new Promise((r) => setTimeout(r, 0));
    });

    expect(hoisted.calls).toContainEqual(["clearAgentHookConsent", 1]);
    expect(harnessSection().textContent).toContain(t("projset.harnessUnanswered"));
    expect(clearButton().disabled).toBe(true);
  });
});
