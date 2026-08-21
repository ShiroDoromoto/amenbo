// @vitest-environment jsdom
// The installed screen answers "what does this machine hold, and is it firing" (`AMB-D-351`) without ever
// reading the catalog. These tests hold it to that: every install is listed, each project × plugin
// crossing is a row of its own (`AMB-D-447`), and everything that crossing can do — its switch, its
// settings — happens in that row, for the project it names and no other (`AMB-D-434`).
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { NONE_SELECTED } from "../core/pluginInstalls";
import type { PluginConfigField, PluginInstall } from "../core/pluginInstalls";
import type {
  PluginActionDto,
  PluginActionRanDto,
  PluginCheckDto,
  PluginDeviceRowDto,
  PluginProjectRowDto,
  PluginWantedSettingDto,
} from "../bindings/bindings";
import type { PluginCatalogRead, PluginUpdate } from "../core/pluginUpdates";

const hoisted = vi.hoisted(() => ({
  installs: [] as PluginInstall[],
  /** What the catalog holds beyond what is installed — the offer each row draws from. */
  updates: [] as PluginUpdate[],
  /** What that offer was measured against — the frame the count is to be read inside. */
  catalog: { read: "fetched" } as PluginCatalogRead,
  /** Which plugins had an update applied. */
  applied: [] as string[],
  loading: false,
  error: undefined as unknown,
  gated: [] as { name: string; projectId: number | null; enabled: boolean }[],
  /** What a disable answers it threw away — zero unless a test is about the discard. */
  droppedQueued: 0,
  /** What the switch is turned away with, for a test about a refusal rather than a move. */
  refuse: undefined as string | undefined,
  /** What the author's own check said when the switch was pressed (`AMB-D-664`) — none unless a test
   *  is about a verdict. */
  check: undefined as PluginCheckDto | undefined,
  /** Every check raised after a write, in order — the crossing it was raised at included. */
  checked: [] as { name: string; projectId: number | null }[],
  /** What that check answers (`AMB-D-664`) — nothing unless a test is about a save-time verdict, which
   *  is also core's answer for a crossing whose gate is shut. */
  saveCheck: undefined as PluginCheckDto | undefined,
  /** Every operation pressed, in order — the values it was handed included, since those are the whole
   *  question about an `ask`. */
  pressed: [] as { name: string; cmd: string; supplied: Record<string, string>; projectId: number | null }[],
  /** What a press answers with. */
  ran: { ok: true } as PluginActionRanDto,
  projects: [] as { id: number; name: string }[],
  /** What each project holds for a plugin's settings, keyed by plugin then project. */
  held: {} as Record<string, Record<number, PluginConfigField[]>>,
  removed: [] as string[],
  /** Every setting written, in order — the project included, since that is what a value belongs to. */
  wrote: [] as { name: string; key: string; value: string; projectId: number | null }[],
  /** What the uninstall answers — the receipt the screen reports from. */
  receipt: {} as Record<string, unknown>,
  /** What the confirmation was asked, and how it was answered. */
  asked: [] as string[],
  confirm: true,
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
    // A value is one project's, so this seam answers nothing until one is named (`AMB-D-434`).
    usePluginConfig: (name: string, projectId: number | null) => ({
      fields: projectId == null ? [] : (hoisted.held[name]?.[projectId] ?? []),
      loading: false,
    }),
    setPluginEnabled: (name: string, projectId: number, enabled: boolean) => {
      hoisted.gated.push({ name, projectId, enabled });
      if (hoisted.refuse != null) return Promise.reject(new Error(hoisted.refuse));
      // A check that refused leaves the gate where it was and comes back as the verdict, which is what
      // the form draws (`AMB-D-664`).
      const moved = hoisted.check && !hoisted.check.ok ? !enabled : enabled;
      return Promise.resolve({
        enabled: moved,
        droppedQueued: hoisted.droppedQueued,
        check: hoisted.check,
      });
    },
    runPluginAction: (
      name: string,
      cmd: string,
      supplied: Record<string, string>,
      projectId: number | null,
    ) => {
      hoisted.pressed.push({ name, cmd, supplied, projectId });
      return Promise.resolve(hoisted.ran);
    },
    uninstallPlugin: (name: string) => {
      hoisted.removed.push(name);
      return Promise.resolve(hoisted.receipt);
    },
    setPluginConfig: (name: string, key: string, value: string, projectId: number | null) => {
      hoisted.wrote.push({ name, key, value, projectId });
      return Promise.resolve();
    },
    checkPluginSettings: (name: string, projectId: number | null) => {
      hoisted.checked.push({ name, projectId });
      return Promise.resolve(hoisted.saveCheck ?? null);
    },
  };
});

// The detection seam, replaced whole: what is offered, and what the two build moves were called with.
vi.mock("../core/pluginUpdates", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/pluginUpdates")>();
  return {
    ...orig,
    usePluginUpdates: () => ({
      updates: hoisted.updates, catalog: hoisted.catalog, loading: false, error: undefined,
    }),
    refreshPluginUpdates: () => {},
    applyPluginUpdate: (name: string) => {
      hoisted.applied.push(name);
      return Promise.resolve(true);
    },
  };
});

vi.mock("../core/dialog", () => ({
  confirmDialog: (message: string) => {
    hoisted.asked.push(message);
    return Promise.resolve(hoisted.confirm);
  },
}));

vi.mock("../core/snapshot", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/snapshot")>();
  return {
    ...orig,
    subscribe: () => () => {},
    getSnapshot: () => ({ ...orig.getSnapshot(), projects: hoisted.projects }),
  };
});

import { PluginInstalledScreen } from "./PluginInstalledScreen";
import { agoSecondsLabel, t, tn, tf } from "../core/i18n";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

/** One crossing of a plugin with a project: off and holding nothing, until a test says otherwise. */
const at = (project: number, over: Partial<PluginProjectRowDto> = {}): PluginProjectRowDto => ({
  project,
  enabled: false,
  hasValue: false,
  requiredUnset: false,
  ...over,
});

/** The device's own row (`AMB-D-601`): off and holding nothing, until a test says otherwise. */
const onDevice = (over: Partial<PluginDeviceRowDto> = {}): PluginDeviceRowDto => ({
  enabled: false,
  hasValue: false,
  requiredUnset: false,
  ...over,
});

/**
 * One installed plugin; `on` names the projects it fires in, as rows of those crossings.
 *
 * Naming a `device` row instead is what a plugin its author declared the machine's looks like
 * (`AMB-D-601`) — one gate, and no project crossing at all — so the declaration follows from the row
 * rather than being spelled twice and able to disagree with itself.
 */
const row = ({ on = [], ...over }: Partial<PluginInstall> & { name: string; on?: number[] }): PluginInstall => ({
  compatible: true,
  projects: on.map((project) => at(project, { enabled: true })),
  config: [],
  actions: [],
  scope: over.device ? "machine" : "project",
  ...over,
});

/** One waiting build, offered with nothing in the way until a test puts something there. */
const offer = (over: Partial<PluginUpdate> & { name: string }): PluginUpdate => ({
  desc: `${over.name} does a thing`,
  missing: [],
  ...over,
});

/**
 * One declared setting, holding nothing until a test says otherwise. It stands in for both faces of a
 * setting — the author's declaration a row is drawn from, and what a project holds — so one object can be
 * handed to either. The state follows the value unless a test names it.
 */
const field = (
  over: Partial<PluginConfigField & PluginWantedSettingDto> & { key: string },
): PluginConfigField & PluginWantedSettingDto => ({
  label: over.key,
  secret: false,
  required: false,
  readonly: false,
  secretSet: false,
  fieldType: "text",
  options: [],
  when: [],
  state: over.value != null || over.secretSet ? "chosen" : "unanswered",
  ...over,
});

/** One operation its author declared (`AMB-D-664`), asking for nothing until a test says it does. */
const action = (over: Partial<PluginActionDto> & { cmd: string }): PluginActionDto => ({
  label: over.cmd,
  ask: [],
  when: [],
  ...over,
});

const button = (label: string) =>
  Array.from(container.querySelectorAll("button")).find((b) => b.textContent === label);
const rows = () => Array.from(container.querySelectorAll(".feed__item"));
const boxes = () => Array.from(container.querySelectorAll<HTMLInputElement>(".plugcfg input"));
const type = (el: HTMLInputElement, value: string) => {
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
  setter.call(el, value);
  el.dispatchEvent(new Event("input", { bubbles: true }));
};
const select = (el: HTMLSelectElement, value: string) => {
  el.value = value;
  el.dispatchEvent(new Event("change", { bubbles: true }));
};
/** One plugin's picker: what it offers is the projects it has no crossing with yet. */
const gatePicker = (i = 0) => rows()[i].querySelector<HTMLSelectElement>("select")!;
/** What a select offers, in order. */
const options = (el: HTMLSelectElement) => Array.from(el.options).map((o) => o.textContent);
/** Every badge on screen, in order — the row's own state, told apart from the prose around it. */
const chips = () => Array.from(container.querySelectorAll(".chip")).map((c) => c.textContent);

beforeEach(() => {
  hoisted.installs = [];
  hoisted.loading = false;
  hoisted.error = undefined;
  hoisted.gated = [];
  hoisted.droppedQueued = 0;
  hoisted.refuse = undefined;
  hoisted.check = undefined;
  hoisted.checked = [];
  hoisted.saveCheck = undefined;
  hoisted.pressed = [];
  hoisted.ran = { ok: true, show: [] };
  hoisted.projects = [];
  hoisted.held = {};
  hoisted.removed = [];
  hoisted.wrote = [];
  hoisted.updates = [];
  hoisted.catalog = { read: "fetched" };
  hoisted.applied = [];
  hoisted.asked = [];
  hoisted.confirm = true;
  hoisted.receipt = {
    wasEnabled: false, secrets: true,
    projectValues: 2, projectGates: 1, directory: true, runsLog: true, anything: true,
  };
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
    hoisted.installs = [row({ name: "notify" }), row({ name: "worktree", on: [1] })];
    render();

    expect(rows()).toHaveLength(2);
    expect(container.textContent).toContain("notify");
    expect(container.textContent).toContain("worktree");
    expect(container.textContent).toContain(tf("plugins.installedCount", { count: 2 }));
    // Enabled is the fact worth badging; installed is what every row on this screen already is.
    expect(container.textContent).toContain(t("plugins.enabledChip"));
    // Each row answers for itself: the one that is on names its project, the one that is not says so.
    expect(rows()[0].textContent).toContain(t("plugins.gate.offEverywhere"));
    expect(rows()[1].textContent).toContain("alpha");
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
  // Picking a project draws its crossing; the switch is in the row (`AMB-D-447`). A crossing has to exist
  // before what an enable would be refused over can be read there, or filled in.
  it("draws the crossing for the project picked, and enables from that row", async () => {
    hoisted.projects = [{ id: 1, name: "alpha" }, { id: 2, name: "beta" }];
    hoisted.installs = [row({ name: "notify" })];
    render();

    act(() => { select(gatePicker(), "2"); });
    expect(chips()).toEqual(["beta"]);
    expect(hoisted.gated, "picking writes nothing on its own").toEqual([]);

    await act(async () => { button(t("plugins.enable"))!.click(); });
    expect(hoisted.gated).toEqual([{ name: "notify", projectId: 2, enabled: true }]);
  });

  it("turns one off from beside the project it is on in", async () => {
    hoisted.projects = [{ id: 1, name: "alpha" }, { id: 2, name: "beta" }];
    hoisted.installs = [row({ name: "notify", on: [2] })];
    render();
    await act(async () => { button(t("plugins.disable"))!.click(); });
    expect(hoisted.gated).toEqual([{ name: "notify", projectId: 2, enabled: false }]);
  });

  // The whole point of the list: a project it is on in is named whether or not anyone is looking at
  // that project, and only the ones left are offered.
  it("names every project it is on in, and offers only the rest", () => {
    hoisted.projects = [
      { id: 1, name: "alpha" }, { id: 2, name: "beta" }, { id: 3, name: "gamma" },
    ];
    hoisted.installs = [row({ name: "notify", on: [1, 3] })];
    render();

    expect(chips()).toEqual([
      t("plugins.enabledChip"),
      "alpha", t("plugins.enabledChip"),
      "gamma", t("plugins.enabledChip"),
    ]);
    expect(options(gatePicker())).toEqual([t("plugins.gate.addProject"), "beta"]);
  });

  // A crossing with every project there is: nothing is left to add, so nothing is offered.
  it("drops the picker when there is no project left to add", () => {
    hoisted.projects = [{ id: 1, name: "alpha" }];
    hoisted.installs = [row({ name: "notify", on: [1] })];
    render();
    expect(container.querySelector("select")).toBeNull();
  });

  // The discard a disable makes is real and there is no other trace of it: those events are not
  // delivered late, and re-enabling starts from now (`AMB-D-399`). An empty queue says nothing at all —
  // a line every time would train the eye past the one time it matters, which is the CLI's line too.
  it("says how many waiting events a disable threw away, and stays quiet when none did", async () => {
    hoisted.projects = [{ id: 1, name: "alpha" }, { id: 2, name: "beta" }];
    hoisted.installs = [row({ name: "notify", on: [1] })];
    hoisted.droppedQueued = 3;
    render();
    await act(async () => { button(t("plugins.disable"))!.click(); });
    expect(container.textContent).toContain(tn("plugins.droppedQueued", 3));

    hoisted.droppedQueued = 0;
    render();
    await act(async () => { button(t("plugins.disable"))!.click(); });
    expect(container.textContent).not.toContain(tn("plugins.droppedQueued", 0));
  });

  // An open gate on a build Amenbo cannot speak to fires nothing, so the switch says why instead of moving.
  it("refuses to enable a plugin this build cannot speak to, and names the mismatch", () => {
    hoisted.projects = [{ id: 1, name: "alpha" }];
    hoisted.installs = [
      row({ name: "notify", compatible: false, incompatibleReason: "needs Amenbo 9.0" }),
    ];
    render();
    expect(container.textContent).toContain("needs Amenbo 9.0");
    expect(container.textContent).toContain(t("plugins.incompatibleChip"));

    // The crossing still draws — a project can hold settings for a plugin that cannot run — and it is
    // the switch there, not the picker, that is shut.
    act(() => { select(gatePicker(), "1"); });
    expect(button(t("plugins.enable"))!.disabled).toBe(true);
  });
});

// An open gate is not a plugin that fires (`AMB-D-359`): a build Amenbo cannot speak to is handed no
// event, so the row has to say that rather than let "enabled" stand for "working".
describe("a plugin this build cannot speak to", () => {
  it("reads as enabled-but-silent, not as enabled", () => {
    hoisted.projects = [{ id: 1, name: "alpha" }];
    hoisted.installs = [
      row({
        name: "notify",
        on: [1],
        compatible: false,
        incompatibleReason: "payload v2, this build speaks v1",
      }),
    ];
    render();
    expect(chips()).toEqual([t("plugins.notFiring"), "alpha", t("plugins.enabledChip")]);
    // Core's own line, not a second judgement of our own.
    expect(container.textContent).toContain("payload v2, this build speaks v1");
  });

  it("leaves a compatible row wearing the plain enabled badge", () => {
    hoisted.projects = [{ id: 1, name: "alpha" }];
    hoisted.installs = [row({ name: "notify", on: [1] })];
    render();
    expect(chips()).toEqual([t("plugins.enabledChip"), "alpha", t("plugins.enabledChip")]);
  });
});

// The layer is the author's (`AMB-D-601`), and this face reads it off the manifest that was installed. The
// declaration is said in prose and the gate it settles is drawn as one row: the whole point of settling
// the layer by declaration is that `plugin enable` means one thing, so a second switch would undo it
// (`AMB-D-379`).
describe("the layer a plugin declared", () => {
  it("says a device-wide plugin reads every project, and draws its one gate as the device's row", () => {
    hoisted.projects = [{ id: 1, name: "alpha" }];
    hoisted.installs = [row({ name: "carry", device: onDevice() })];
    render();

    expect(container.textContent).toContain(t("plugins.scope.machine"));
    // One row, named for the device rather than for a project — and no project to add beside it, since
    // there is no crossing to make.
    expect(chips()).toEqual([t("plugins.gate.device")]);
    expect(container.querySelector("select")).toBeNull();
    expect(container.textContent).not.toContain(t("plugins.gate.offEverywhere"));
  });

  it("draws that one row as on once the device's gate is open", () => {
    hoisted.projects = [{ id: 1, name: "alpha" }];
    hoisted.installs = [row({ name: "carry", device: onDevice({ enabled: true }) })];
    render();

    // The badge on the plugin itself reads the device's gate too: a machine-wide plugin has no project
    // row for `firesAnywhere` to have found.
    expect(chips()).toEqual([
      t("plugins.enabledChip"),
      t("plugins.gate.device"),
      t("plugins.enabledChip"),
    ]);
  });

  // The device's row is the same row at the other layer (`AMB-D-601`), and that includes its settings:
  // the form opens inside it, drawn from the author's schema, before any value was ever read for the
  // device — a declared box and the save under it, not an empty panel.
  it("opens the settings form inside the device's own row, boxes and save and all", async () => {
    hoisted.projects = [{ id: 1, name: "alpha" }];
    hoisted.installs = [
      row({
        name: "worktree",
        config: [field({ key: "base", label: "Base branch", required: true })],
        device: onDevice({ requiredUnset: true }),
      }),
    ];
    render();

    expect(container.textContent).toContain(t("plugins.cfg.requiredEmpty"));
    await act(async () => { button(t("plugins.cfg.open"))!.click(); });
    expect(boxes().length).toBeGreaterThan(0);
    expect(button(t("plugins.cfg.save"))).toBeTruthy();
  });

  it("says nothing for a project's plugin, which is every plugin that declared nothing", () => {
    hoisted.projects = [{ id: 1, name: "alpha" }];
    hoisted.installs = [row({ name: "notify", on: [1] })];
    render();

    expect(container.textContent).not.toContain(t("plugins.scope.machine"));
    expect(container.textContent).not.toContain(t("plugins.gate.device"));
  });
});

// The settings a plugin's author declared (`AMB-D-356`), drawn as a form Amenbo generates. Amenbo judges
// nothing in it: a text box and a masked pair are the two kinds there are, every value is the project's
// on screen, and all of them go out through the one write boundary.
describe("the settings form", () => {
  it("offers settings at a crossing only for a plugin that declares any, and marks the refusal there", () => {
    hoisted.projects = [{ id: 1, name: "alpha" }];
    hoisted.installs = [
      row({
        name: "notify",
        config: [field({ key: "webhook", required: true })],
        projects: [at(1, { requiredUnset: true })],
      }),
      row({ name: "quiet", on: [1] }),
    ];
    hoisted.held = { notify: { 1: [field({ key: "webhook", required: true })] } };
    render();

    // The mark is the crossing's, and it is readable before the switch is pressed (`AMB-D-447`).
    expect(rows()[0].textContent).toContain(t("plugins.cfg.requiredEmpty"));
    expect(button(t("plugins.cfg.open"))).toBeTruthy();
    // The plugin that declares nothing has nothing to open, at any crossing.
    expect(rows()[1].textContent).not.toContain(t("plugins.cfg.open"));
  });

  // The refusal is a fact about one press, and the row it stands in is where the answer to it goes
  // (`AMB-D-447`). So a value saved here retires it: the sentence names what was missing at the time,
  // and a row saying both that its settings are filled in and that they are keeping it off is a row
  // telling a reader two different things.
  it("retires the refusal in a row once the settings there are written", async () => {
    hoisted.projects = [{ id: 1, name: "alpha" }];
    hoisted.installs = [
      row({
        name: "notify",
        config: [field({ key: "webhook", required: true })],
        projects: [at(1, { requiredUnset: true })],
      }),
    ];
    hoisted.held = { notify: { 1: [field({ key: "webhook", required: true })] } };
    hoisted.refuse = "notify cannot be enabled: required setting(s) not provided: webhook";
    render();

    await act(async () => { button(t("plugins.enable"))!.click(); });
    expect(container.textContent).toContain("required setting(s) not provided");

    act(() => { button(t("plugins.cfg.open"))!.click(); });
    act(() => { type(boxes()[0], "https://example.test/hook"); });
    await act(async () => { button(t("plugins.cfg.save"))!.click(); });

    expect(hoisted.wrote).toEqual([
      { name: "notify", key: "webhook", value: "https://example.test/hook", projectId: 1 },
    ]);
    expect(container.textContent, "the reason is gone once its premise is").not.toContain(
      "required setting(s) not provided",
    );
  });

  it("writes a text setting for the project whose row it is in, and only what changed", async () => {
    hoisted.projects = [{ id: 7, name: "alpha" }];
    hoisted.installs = [
      row({
        name: "notify",
        on: [7],
        config: [field({ key: "events" }), field({ key: "room" })],
      }),
    ];
    hoisted.held = { notify: { 7: [field({ key: "events", value: "push" }), field({ key: "room" })] } };
    render();
    act(() => { button(t("plugins.cfg.open"))!.click(); });

    // The box opens holding what is stored, so an edit is a correction rather than a retype.
    expect(boxes()[0].value).toBe("push");
    act(() => { type(boxes()[0], "push,merge"); });
    await act(async () => { button(t("plugins.cfg.save"))!.click(); });

    expect(hoisted.wrote).toEqual([
      { name: "notify", key: "events", value: "push,merge", projectId: 7 },
    ]);
  });

  // A setting belongs to a project and to nothing else (`AMB-D-434`), and the row it is opened in has
  // already said which (`AMB-D-447`) — so the form asks nobody a second time.
  it("writes for the project whose row it was opened in, without asking again", async () => {
    hoisted.projects = [{ id: 7, name: "alpha" }, { id: 8, name: "beta" }];
    hoisted.installs = [row({ name: "notify", on: [8], config: [field({ key: "events" })] })];
    render();
    act(() => { button(t("plugins.cfg.open"))!.click(); });

    expect(container.querySelector(".plugcfg select"), "no picker of its own").toBeNull();

    act(() => { type(boxes()[0], "deploy"); });
    await act(async () => { button(t("plugins.cfg.save"))!.click(); });

    expect(hoisted.wrote).toEqual([
      { name: "notify", key: "events", value: "deploy", projectId: 8 },
    ]);
  });

  // A secret is written and never read back, so the second box is the only check on a typo there is.
  it("asks for a secret twice, and writes nothing when the two do not match", async () => {
    hoisted.projects = [{ id: 7, name: "alpha" }];
    hoisted.installs = [row({ name: "notify", on: [7], config: [field({ key: "token", secret: true })] })];
    render();
    act(() => { button(t("plugins.cfg.open"))!.click(); });

    act(() => { type(boxes()[0], "shh"); });
    act(() => { type(boxes()[1], "shhh"); });
    await act(async () => { button(t("plugins.cfg.save"))!.click(); });
    expect(hoisted.wrote).toEqual([]);
    expect(container.textContent).toContain(t("plugins.cfg.secretMismatch"));

    act(() => { type(boxes()[1], "shh"); });
    await act(async () => { button(t("plugins.cfg.save"))!.click(); });
    // A secret is this project's, like every other value (`AMB-D-434`).
    expect(hoisted.wrote).toEqual([{ name: "notify", key: "token", value: "shh", projectId: 7 }]);
  });

  // A field whose author declared candidates has three answers, and the form has to draw all three
  // (`AMB-D-415`): ticked boxes, none of them, and nobody having answered — where the default is ticked
  // and named, because that is what a run receives as things stand.
  it("draws a choice as its candidates, and says which of the three answers is in force", async () => {
    hoisted.projects = [{ id: 7, name: "alpha" }];
    const events = field({
      key: "events",
      fieldType: "multi",
      options: [
        { value: "task.done", label: "Done", when: [] },
        { value: "task.rejected", label: "Rejected", when: [] },
      ],
      defaultValue: "task.done",
    });
    hoisted.installs = [row({ name: "notify", on: [7], config: [events] })];
    hoisted.held = { notify: { 7: [events] } };
    render();
    act(() => { button(t("plugins.cfg.open"))!.click(); });

    // Unanswered: the author's default is ticked and said out loud, and the boxes are the candidates.
    expect(container.textContent).toContain(t("plugins.cfg.default"));
    expect(boxes().map((b) => b.checked)).toEqual([true, false]);

    // Ticking the other one writes both, in the order the author declared them.
    act(() => { boxes()[1].click(); });
    await act(async () => { button(t("plugins.cfg.save"))!.click(); });
    expect(hoisted.wrote).toEqual([
      { name: "notify", key: "events", value: "task.done,task.rejected", projectId: 7 },
    ]);
  });

  // **A condition is read against the answers on screen, not the ones in the store** (`AMB-D-727`). The
  // platform's half is already gone by the time the form has this — core settles it — so what is left is
  // the half that moves under the user's fingers, and it has to move at the tick rather than at the save.
  it("draws a conditioned field, its candidates and its button the moment the answer they read changes", async () => {
    hoisted.projects = [{ id: 7, name: "alpha" }];
    const transport = field({
      key: "transport",
      fieldType: "multi",
      options: [
        { value: "icloud", label: "iCloud", when: [] },
        { value: "cloudflare", label: "Cloudflare", when: [] },
      ],
    });
    const worker = field({
      key: "worker_url",
      label: "Worker の URL",
      when: [{ field: "transport", has: "cloudflare" }],
    });
    hoisted.installs = [
      row({
        name: "viewer",
        on: [7],
        config: [transport, worker],
        actions: [action({ cmd: "tunnel", label: "Raise the tunnel", when: [{ field: "transport", has: "cloudflare" }] })],
      }),
    ];
    hoisted.held = { viewer: { 7: [transport, worker] } };
    render();
    act(() => { button(t("plugins.cfg.open"))!.click(); });

    // Nothing answers it, so neither the field nor the button that acts on it is on the form.
    expect(container.textContent).not.toContain("Worker の URL");
    expect(button("Raise the tunnel")).toBeUndefined();

    // Ticking Cloudflare brings both out — before any save, which is the whole point.
    act(() => { boxes()[1].click(); });
    expect(container.textContent).toContain("Worker の URL");
    expect(button("Raise the tunnel")).toBeDefined();

    // And unticking it puts them away again.
    act(() => { boxes()[1].click(); });
    expect(container.textContent).not.toContain("Worker の URL");
    expect(button("Raise the tunnel")).toBeUndefined();
  });

  // A candidate carries its own condition, and it is read the same way — the checkbox goes, the field it
  // belongs to stays.
  it("drops a candidate whose own condition does not hold", async () => {
    hoisted.projects = [{ id: 7, name: "alpha" }];
    const mode = field({ key: "mode", fieldType: "multi", options: [{ value: "advanced", label: "Advanced", when: [] }] });
    const events = field({
      key: "events",
      fieldType: "multi",
      options: [
        { value: "task.done", label: "Done", when: [] },
        { value: "task.rejected", label: "Rejected", when: [{ field: "mode", has: "advanced" }] },
      ],
    });
    hoisted.installs = [row({ name: "notify", on: [7], config: [mode, events] })];
    hoisted.held = { notify: { 7: [mode, events] } };
    render();
    act(() => { button(t("plugins.cfg.open"))!.click(); });

    expect(container.textContent).toContain("Done");
    expect(container.textContent).not.toContain("Rejected");

    // Ticking `advanced` — the first box on the form — offers the candidate it gates.
    act(() => { boxes()[0].click(); });
    expect(container.textContent).toContain("Rejected");
  });

  // Unticking the last box is an answer, not a retraction: it writes the reserved word, because an empty
  // value is where "nobody answered" already lives.
  it("writes the word for none of them when every box comes off", async () => {
    hoisted.projects = [{ id: 7, name: "alpha" }];
    const events = field({
      key: "events",
      fieldType: "multi",
      options: [{ value: "task.done", label: "Done", when: [] }],
      value: "task.done",
    });
    hoisted.installs = [row({ name: "notify", on: [7], config: [events] })];
    hoisted.held = { notify: { 7: [events] } };
    render();
    act(() => { button(t("plugins.cfg.open"))!.click(); });

    act(() => { boxes()[0].click(); });
    await act(async () => { button(t("plugins.cfg.save"))!.click(); });
    expect(hoisted.wrote).toEqual([
      { name: "notify", key: "events", value: NONE_SELECTED, projectId: 7 },
    ]);
  });

  // The two answers a form must not collapse: "none of them" says so, and the way back to the default is
  // the same write that empties any other field, wearing the name of what it does here.
  it("names the none-of-them answer, and offers the default back", async () => {
    hoisted.projects = [{ id: 7, name: "alpha" }];
    const events = field({
      key: "events",
      fieldType: "multi",
      options: [{ value: "task.done", label: "Done", when: [] }],
      defaultValue: "task.done",
      value: NONE_SELECTED,
      state: "none",
    });
    hoisted.installs = [row({ name: "notify", on: [7], config: [events] })];
    hoisted.held = { notify: { 7: [events] } };
    render();
    act(() => { button(t("plugins.cfg.open"))!.click(); });

    expect(container.textContent).toContain(t("plugins.cfg.noneChosen"));
    expect(boxes()[0].checked).toBe(false);

    await act(async () => { button(t("plugins.cfg.restoreDefault"))!.click(); });
    expect(hoisted.wrote).toEqual([{ name: "notify", key: "events", value: "", projectId: 7 }]);
    // And the boxes go back to the author's, not blank: the write that restores a default must not be
    // drawn as the answer that declines everything.
    expect(boxes()[0].checked).toBe(true);
  });

  // A crossing the install says nothing about — one just drawn from the picker — takes its mark from the
  // author's schema, and reads it the way the enable gate does: a field the author put a default behind is
  // never unanswered (`AMB-D-415`), so it is not something an enable would be refused over.
  it("marks a fresh crossing only over a required setting with no default behind it", () => {
    hoisted.projects = [{ id: 1, name: "alpha" }];
    hoisted.installs = [
      row({
        name: "notify",
        config: [field({ key: "events", required: true, defaultValue: "task.done" })],
      }),
      row({ name: "hooks", config: [field({ key: "webhook", required: true })] }),
    ];
    render();
    act(() => { select(gatePicker(0), "1"); });
    act(() => { select(gatePicker(1), "1"); });

    expect(rows()[0].textContent).not.toContain(t("plugins.cfg.requiredEmpty"));
    expect(rows()[1].textContent).toContain(t("plugins.cfg.requiredEmpty"));
  });

  // The author's own paragraph belongs under the input it explains, drawn as they typed it — their line
  // breaks kept, and nothing in it turned into a link (`AMB-D-656`).
  it("draws the author's paragraph under the field, as plain text", () => {
    hoisted.projects = [{ id: 7, name: "alpha" }];
    const webhook = field({
      key: "webhook",
      help: "Create it under Incoming Webhooks.\n\n[Not a link](https://example.test/x)",
      placeholder: "https://hooks.example.test/T000/B000",
    });
    hoisted.installs = [row({ name: "notify", on: [7], config: [webhook] })];
    hoisted.held = { notify: { 7: [webhook] } };
    render();
    act(() => { button(t("plugins.cfg.open"))!.click(); });

    const help = container.querySelector(".plugcfg__help")!;
    expect(help.textContent).toBe(
      "Create it under Incoming Webhooks.\n\n[Not a link](https://example.test/x)",
    );
    expect(help.querySelector("a")).toBe(null);
    expect(boxes()[0].placeholder).toBe("https://hooks.example.test/T000/B000");
  });

  // A default is the value a run really receives; an example is not. So the box shows the default where
  // there is one, and falls to the author's example only where there is not (`AMB-D-656` / `AMB-D-474`).
  it("shows the default in the empty box, and the example only without one", () => {
    hoisted.projects = [{ id: 7, name: "alpha" }];
    const base = field({ key: "base", defaultValue: "main", placeholder: "release/*" });
    hoisted.installs = [row({ name: "worktree", on: [7], config: [base] })];
    hoisted.held = { worktree: { 7: [base] } };
    render();
    act(() => { button(t("plugins.cfg.open"))!.click(); });

    expect(boxes()[0].placeholder).toBe("main");
  });

  // A value the plugin writes back is not the user's to edit or to take away (`AMB-D-656`): the viewer's
  // three fields are its `setup`'s, and the clear button beside them was a way to break it.
  it("shows a readonly setting's value with no input and no way to clear it", () => {
    hoisted.projects = [{ id: 7, name: "alpha" }];
    const worker = field({ key: "worker_url", readonly: true, value: "https://amenbo.example.test" });
    hoisted.installs = [row({ name: "viewer", on: [7], config: [worker] })];
    hoisted.held = { viewer: { 7: [worker] } };
    render();
    act(() => { button(t("plugins.cfg.open"))!.click(); });

    expect(container.querySelector(".plugcfg__fixed")!.textContent).toBe("https://amenbo.example.test");
    expect(boxes()).toEqual([]);
    expect(button(t("plugins.cfg.clear"))).toBe(undefined);
  });

  // Clearing is the same door as setting: an empty value is "not provided", which is what `required` reads.
  it("clears a held setting with an empty value", async () => {
    hoisted.projects = [{ id: 7, name: "alpha" }];
    hoisted.installs = [row({ name: "notify", on: [7], config: [field({ key: "events" })] })];
    hoisted.held = { notify: { 7: [field({ key: "events", value: "push" })] } };
    render();
    act(() => { button(t("plugins.cfg.open"))!.click(); });

    await act(async () => { button(t("plugins.cfg.clear"))!.click(); });
    expect(hoisted.wrote).toEqual([{ name: "notify", key: "events", value: "", projectId: 7 }]);
    expect(container.textContent).toContain(t("plugins.cfg.cleared"));
  });
});

// The settings face is where a plugin author's own code speaks to a person (`AMB-D-664`): the check it
// ran when the switch was pressed, and the operations it declared. Both are drawn here and nowhere else
// — the CLI is told the keys and never the sentences.
describe("what the author's own code says on the form", () => {
  it("draws a refusing check's sentences where each one belongs, and leaves the gate shut", async () => {
    hoisted.projects = [{ id: 7, name: "alpha" }];
    hoisted.installs = [row({ name: "mail", config: [field({ key: "smtp_host" })], projects: [at(7)] })];
    hoisted.held = { mail: { 7: [field({ key: "smtp_host", value: "smtp.example.test" })] } };
    hoisted.check = {
      ok: false,
      answered: true,
      message: "SCENARIO — the mailbox would not answer",
      fields: { smtp_host: "SCENARIO — there is a space in it" },
      show: [],
    };
    render();

    await act(async () => { button(t("plugins.enable"))!.click(); });

    // The refusal opens the form itself: the sentences are about boxes, and a button someone has to find
    // first is a refusal nobody reads.
    expect(container.querySelector(".plugcfg"), "nobody pressed the settings button").not.toBeNull();
    expect(container.textContent).toContain("SCENARIO — the mailbox would not answer");
    expect(container.textContent).toContain("SCENARIO — there is a space in it");
  });

  // A silence is Amenbo's reading of a run, with nothing of the plugin's in it (`AMB-D-354`) — so the
  // form says what happened in its own words and leaves the reason to the execution log.
  it("says the check did not answer when it said nothing readable", async () => {
    hoisted.projects = [{ id: 7, name: "alpha" }];
    hoisted.installs = [row({ name: "mail", config: [field({ key: "smtp_host" })], projects: [at(7)] })];
    hoisted.check = { ok: false, answered: false, fields: {}, show: [] };
    render();

    await act(async () => { button(t("plugins.enable"))!.click(); });
    expect(container.textContent).toContain(t("plugins.check.noAnswer"));
  });

  // The other moment the author's code speaks (`AMB-D-664`): a save raises the same check, once the boxes
  // have all landed — one run for the save, not one per box, since each box is its own write.
  it("raises one check after a save, and draws what it said over a save that stands", async () => {
    hoisted.projects = [{ id: 7, name: "alpha" }];
    hoisted.installs = [
      row({
        name: "mail",
        on: [7],
        config: [field({ key: "smtp_host" }), field({ key: "smtp_user" })],
      }),
    ];
    hoisted.saveCheck = {
      ok: false,
      answered: true,
      message: "SCENARIO — the mailbox would not answer",
      fields: { smtp_user: "SCENARIO — it is not an address" },
      show: [],
    };
    render();
    act(() => { button(t("plugins.cfg.open"))!.click(); });
    act(() => {
      type(boxes()[0], "smtp.example.test");
      type(boxes()[1], "postmaster");
    });

    await act(async () => { button(t("plugins.cfg.save"))!.click(); });

    expect(hoisted.wrote).toEqual([
      { name: "mail", key: "smtp_host", value: "smtp.example.test", projectId: 7 },
      { name: "mail", key: "smtp_user", value: "postmaster", projectId: 7 },
    ]);
    expect(hoisted.checked, "two writes, and one check over what they left").toEqual([
      { name: "mail", projectId: 7 },
    ]);
    expect(container.textContent).toContain("SCENARIO — the mailbox would not answer");
    expect(container.textContent).toContain("SCENARIO — it is not an address");
    // Nothing is taken back by it: the save is reported as the save it was (`AMB-D-664`).
    expect(container.textContent).toContain(t("plugins.cfg.saved"));
  });

  // A verdict is about the values as they stood when it ran, so a save replaces the switch's rather than
  // leaving its sentence standing over values it never saw.
  it("replaces the switch's verdict with what the save's check said", async () => {
    hoisted.projects = [{ id: 7, name: "alpha" }];
    hoisted.installs = [row({ name: "mail", config: [field({ key: "smtp_host" })], projects: [at(7)] })];
    hoisted.check = {
      ok: false, answered: true, message: "SCENARIO — when it was pressed", fields: {}, show: [],
    };
    hoisted.saveCheck = {
      ok: true, answered: true, message: "SCENARIO — after the save", fields: {}, show: [],
    };
    render();

    await act(async () => { button(t("plugins.enable"))!.click(); });
    expect(container.textContent).toContain("SCENARIO — when it was pressed");

    act(() => { type(boxes()[0], "smtp.example.test"); });
    await act(async () => { button(t("plugins.cfg.save"))!.click(); });

    expect(container.textContent).not.toContain("SCENARIO — when it was pressed");
    expect(container.textContent).toContain("SCENARIO — after the save");
  });

  // What a form may raise is what the manifest named in advance (`AMB-D-522`): the press hands back the
  // declared `cmd`, never a line this screen composed.
  it("raises the declared call the pressed button names", async () => {
    hoisted.projects = [{ id: 7, name: "alpha" }];
    hoisted.installs = [
      row({ name: "slack", on: [7], actions: [action({ cmd: "config test", label: "Send a test" })] }),
    ];
    render();
    act(() => { button(t("plugins.cfg.open"))!.click(); });

    await act(async () => { button("Send a test")!.click(); });
    expect(hoisted.pressed).toEqual([
      { name: "slack", cmd: "config test", supplied: {}, projectId: 7 },
    ]);
    expect(container.textContent).toContain(t("plugins.act.ok"));
  });

  // Running somebody else's code is what enabling means (`AMB-D-351`), so an off crossing draws the
  // buttons and will not press them — with the line that says which.
  it("will not press an operation at a crossing the plugin is off in", () => {
    hoisted.projects = [{ id: 7, name: "alpha" }];
    hoisted.installs = [
      row({ name: "slack", projects: [at(7)], actions: [action({ cmd: "config test", label: "Send a test" })] }),
    ];
    render();
    act(() => { button(t("plugins.cfg.open"))!.click(); });

    expect(button("Send a test")!.disabled).toBe(true);
    expect(container.textContent).toContain(t("plugins.act.needsEnabled"));
  });

  // The one-time value: it rides with the press and is stored nowhere — not through the settings door,
  // and not in the form once the press is over (`AMB-D-664`).
  it("asks for what a press needs, hands it to that run, and keeps it nowhere", async () => {
    hoisted.projects = [{ id: 7, name: "alpha" }];
    hoisted.installs = [
      row({
        name: "viewer",
        on: [7],
        actions: [action({
          cmd: "setup",
          label: "Set it up",
          ask: [{ key: "api_token", label: "API token", secret: true }],
        })],
      }),
    ];
    render();
    act(() => { button(t("plugins.cfg.open"))!.click(); });

    // Pressing the button opens its boxes rather than running: the run needs an answer first.
    act(() => { button("Set it up")!.click(); });
    expect(hoisted.pressed).toEqual([]);
    const box = container.querySelector<HTMLInputElement>(".plugcfg__ask input")!;
    expect(box.type, "the author said it is a secret").toBe("password");
    expect(container.textContent).toContain(t("plugins.act.askNote"));

    act(() => { type(box, "t-1"); });
    await act(async () => { button(t("plugins.act.run"))!.click(); });

    expect(hoisted.pressed).toEqual([
      { name: "viewer", cmd: "setup", supplied: { api_token: "t-1" }, projectId: 7 },
    ]);
    expect(hoisted.wrote, "an asked value goes through no settings door").toEqual([]);
    expect(container.querySelector(".plugcfg__ask"), "and the boxes are gone with it").toBeNull();
  });

  // An operation has no return value: what a run that failed leaves on the screen is the one line its
  // author wrote (`AMB-D-664`), and nothing about the form changes.
  it("shows the line a failed operation wrote", async () => {
    hoisted.projects = [{ id: 7, name: "alpha" }];
    hoisted.installs = [
      row({ name: "slack", on: [7], actions: [action({ cmd: "config test", label: "Send a test" })] }),
    ];
    hoisted.ran = { ok: false, message: "SCENARIO — the webhook returned 404", show: [] };
    render();
    act(() => { button(t("plugins.cfg.open"))!.click(); });

    await act(async () => { button("Send a test")!.click(); });
    expect(container.textContent).toContain("SCENARIO — the webhook returned 404");
  });
});

// The other half of the update offer (`AMB-D-359`): the banner takes them in bulk, and the row takes one.
// What needs a decision is named rather than offered as a button that would only be refused, and the way
// back from an update this face applied is on the same row.
describe("moving one plugin's build from its row", () => {
  it("offers the waiting build, and applies just that one", async () => {
    hoisted.installs = [row({ name: "notify" }), row({ name: "worktree" })];
    hoisted.updates = [offer({ name: "notify", desc: "now with sounds" })];
    render();

    expect(container.textContent).toContain(t("plugins.updates.waiting"));
    expect(container.textContent).toContain("now with sounds");

    await act(async () => { button(t("plugins.updates.apply"))!.click(); });
    expect(hoisted.applied).toEqual(["notify"]);
    expect(container.textContent).toContain(tf("plugins.updates.applied", { count: 1 }));
  });

  // A hold is not a button: the settings the new schema wants are opened from this same row, which is why
  // the offer is named here rather than sending anyone elsewhere.
  it("names an offer that needs a decision instead of offering it", () => {
    hoisted.projects = [{ id: 1, name: "alpha" }];
    hoisted.installs = [
      row({ name: "notify", on: [1], config: [field({ key: "token", required: true })] }),
    ];
    hoisted.updates = [offer({ name: "notify", hold: "settings", missing: ["token"] })];
    render();

    expect(container.textContent).toContain(
      tf("plugins.updates.holdSettings", { name: "notify", keys: "token" }),
    );
    expect(button(t("plugins.updates.apply"))).toBeUndefined();
    expect(button(t("plugins.cfg.open"))).toBeTruthy();
  });

  // The freshness window makes "nothing has changed" and "nothing had changed an hour ago" the same empty
  // list, so what the check was measured against is on screen beside it — the whole reason 0 can be read.
  it("says which catalog the count was measured against", () => {
    hoisted.installs = [row({ name: "notify" })];
    hoisted.catalog = { read: "cached", ageSeconds: 1800 };
    render();

    expect(container.textContent).toContain(
      tf("plugins.updates.catalog.cached", { ago: agoSecondsLabel(1800) }),
    );

    hoisted.catalog = { read: "unavailable" };
    render();
    expect(container.textContent).toContain(t("plugins.updates.catalog.unavailable"));
  });

  // Nothing installed reads no catalog at all, and the empty screen says the whole of it. A note about a
  // catalog nobody needed would only suggest something went wrong.
  it("says nothing about a catalog when there was nothing to compare", () => {
    hoisted.catalog = { read: "notNeeded" };
    render();

    expect(container.textContent).not.toContain(t("plugins.updates.catalog.fetched"));
    expect(container.textContent).not.toContain(t("plugins.updates.catalog.unavailable"));
  });

  it("says why an incompatible build is not offered", () => {
    hoisted.installs = [row({ name: "notify" })];
    hoisted.updates = [offer({ name: "notify", hold: "incompatible" })];
    render();

    expect(container.textContent).toContain(tf("plugins.updates.holdIncompatible", { name: "notify" }));
    expect(button(t("plugins.updates.apply"))).toBeUndefined();
  });

});

// Uninstall is not disable (`AMB-D-357`): it takes the settings in every project and the secrets with it,
// and none of that comes back. So the question has to say so before anything is removed, and the answer
// has to say what actually went.
describe("removing a plugin from this screen", () => {
  it("asks first, naming what goes beyond the plugin itself", async () => {
    hoisted.installs = [row({ name: "notify" })];
    render();

    await act(async () => { button(t("plugins.remove"))!.click(); });
    expect(hoisted.asked).toEqual([tf("plugins.removeConfirm", { name: "notify" })]);
    expect(hoisted.removed).toEqual(["notify"]);
  });

  it("removes nothing when the question is declined", async () => {
    hoisted.confirm = false;
    hoisted.installs = [row({ name: "notify" })];
    render();

    await act(async () => { button(t("plugins.remove"))!.click(); });
    expect(hoisted.removed).toEqual([]);
  });

  // The receipt is what makes "a re-install starts clean" believable, and it is drawn after the row is gone.
  it("says what was taken, once the row it was about is gone", async () => {
    hoisted.installs = [row({ name: "notify" })];
    render();
    await act(async () => { button(t("plugins.remove"))!.click(); });

    hoisted.installs = [];
    render();
    expect(container.textContent).toContain(
      tf("plugins.removed", {
        name: "notify",
        what: [
          t("plugins.removedPart.binary"),
          t("plugins.removedPart.settings"),
          t("plugins.removedPart.secrets"),
          t("plugins.removedPart.runs"),
        ].join(t("common.listSeparator")),
      }),
    );
  });

  // A name that held nothing is not a failure — it is how a half-broken install gets cleaned up.
  it("says so when the name held nothing on this machine", async () => {
    hoisted.receipt = {
      wasEnabled: false, secrets: false,
      projectValues: 0, projectGates: 0, directory: false, runsLog: false, anything: false,
    };
    hoisted.installs = [row({ name: "notify" })];
    render();

    await act(async () => { button(t("plugins.remove"))!.click(); });
    expect(container.textContent).toContain(tf("plugins.removedNothing", { name: "notify" }));
  });
});
