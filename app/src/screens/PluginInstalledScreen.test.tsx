// @vitest-environment jsdom
// The installed screen answers "what does this machine hold, and is it firing" (`AMB-D-351`) without ever
// reading the catalog. These tests hold it to that: every install is listed with the switch its author
// declared, the switch moves from here under the same consent the market asks, and a project-scoped gate
// still waits to be told which project it speaks for (`AMB-D-379`).
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PluginConfigField, PluginInstall } from "../core/pluginInstalls";

const hoisted = vi.hoisted(() => ({
  installs: [] as PluginInstall[],
  loading: false,
  error: undefined as unknown,
  gated: [] as { name: string; projectId: number | null; enabled: boolean }[],
  projects: [] as { id: number; name: string }[],
  removed: [] as string[],
  /** Every setting written, in order — the tier included, since that is what the switch chooses. */
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
    setPluginEnabled: (name: string, projectId: number | null, enabled: boolean) => {
      hoisted.gated.push({ name, projectId, enabled });
      return Promise.resolve(enabled);
    },
    uninstallPlugin: (name: string) => {
      hoisted.removed.push(name);
      return Promise.resolve(hoisted.receipt);
    },
    setPluginConfig: (name: string, key: string, value: string, projectId: number | null) => {
      hoisted.wrote.push({ name, key, value, projectId });
      return Promise.resolve();
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
import { t, tf } from "../core/i18n";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

const row = (over: Partial<PluginInstall> & { name: string }): PluginInstall => ({
  scope: "machine",
  consented: false,
  compatible: true,
  config: [],
  ...over,
});

/** One declared setting, holding nothing until a test says otherwise. */
const field = (over: Partial<PluginConfigField> & { key: string }): PluginConfigField => ({
  label: over.key,
  secret: false,
  required: false,
  secretSet: false,
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
/** The form's own selects: the tier, and (once the project tier is chosen) which project. */
const tiers = () => Array.from(container.querySelectorAll<HTMLSelectElement>(".plugcfg select"));
/** Every badge on screen, in order — the row's own state, told apart from the prose around it. */
const chips = () => Array.from(container.querySelectorAll(".chip")).map((c) => c.textContent);

beforeEach(() => {
  hoisted.installs = [];
  hoisted.loading = false;
  hoisted.error = undefined;
  hoisted.gated = [];
  hoisted.projects = [];
  hoisted.removed = [];
  hoisted.wrote = [];
  hoisted.asked = [];
  hoisted.confirm = true;
  hoisted.receipt = {
    wasEnabled: false, consent: true, machineDefaults: true, secrets: true,
    projectOverrides: 2, projectGates: 1, directory: true, runsLog: true, anything: true,
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
    expect(container.textContent).toContain(t("plugins.incompatibleChip"));
    expect(button(t("plugins.enable"))!.disabled).toBe(true);
  });
});

// An open gate is not a plugin that fires (`AMB-D-359`): a build amenbo cannot speak to is handed no
// event, so the row has to say that rather than let "enabled" stand for "working".
describe("a plugin this build cannot speak to", () => {
  it("reads as enabled-but-silent, not as enabled", () => {
    hoisted.installs = [
      row({
        name: "notify",
        consented: true,
        enabled: true,
        compatible: false,
        incompatibleReason: "payload v2, this build speaks v1",
      }),
    ];
    render();
    expect(chips()).toEqual([t("plugins.gate.machine"), t("plugins.notFiring")]);
    // Core's own line, not a second judgement of our own.
    expect(container.textContent).toContain("payload v2, this build speaks v1");
  });

  it("leaves a compatible row wearing the plain enabled badge", () => {
    hoisted.installs = [row({ name: "notify", consented: true, enabled: true })];
    render();
    expect(chips()).toEqual([t("plugins.gate.machine"), t("plugins.enabledChip")]);
  });
});

// The settings a plugin's author declared (`AMB-D-356`), drawn as a form amenbo generates. amenbo judges
// nothing in it: a text box and a masked pair are the two kinds there are, the tier is the user's choice,
// and every value goes out through the one write boundary.
describe("the settings form", () => {
  it("offers settings only for a plugin that declares any, and counts what an enable will refuse over", () => {
    hoisted.installs = [
      row({ name: "notify", config: [field({ key: "webhook", required: true })] }),
      row({ name: "quiet" }),
    ];
    render();

    expect(button(t("plugins.cfg.open"))).toBeTruthy();
    expect(container.textContent).toContain(tf("plugins.cfg.requiredUnset", { count: 1 }));
    // The row that declares nothing has nothing to open.
    expect(rows()[1].textContent).not.toContain(t("plugins.cfg.open"));
  });

  it("writes a text setting at the machine tier, and only what changed", async () => {
    hoisted.installs = [
      row({
        name: "notify",
        config: [field({ key: "events", machineValue: "push" }), field({ key: "room" })],
      }),
    ];
    render();
    act(() => { button(t("plugins.cfg.open"))!.click(); });

    // The box opens holding what is stored, so an edit is a correction rather than a retype.
    expect(boxes()[0].value).toBe("push");
    act(() => { type(boxes()[0], "push,merge"); });
    await act(async () => { button(t("plugins.cfg.save"))!.click(); });

    expect(hoisted.wrote).toEqual([
      { name: "notify", key: "events", value: "push,merge", projectId: null },
    ]);
  });

  // The tiers are the settings' own: an override is written for the project the form names, and an empty
  // one is no override at all — which is why the default it falls back to is shown beside it.
  it("writes the override for the project the tier switch names", async () => {
    hoisted.projects = [{ id: 7, name: "alpha" }, { id: 8, name: "beta" }];
    hoisted.installs = [row({ name: "notify", config: [field({ key: "events", machineValue: "push" })] })];
    render();
    act(() => { button(t("plugins.cfg.open"))!.click(); });

    act(() => { select(tiers()[0], "project"); });
    // Nothing can be written until the project is named — the machine default is not a fallback here.
    expect(container.textContent).toContain(t("plugins.cfg.pickProjectNote"));
    expect(button(t("plugins.cfg.save"))!.disabled).toBe(true);

    act(() => { select(tiers()[1], "8"); });
    expect(container.textContent).toContain(tf("plugins.cfg.fallback", { value: "push" }));
    act(() => { type(boxes()[0], "deploy"); });
    await act(async () => { button(t("plugins.cfg.save"))!.click(); });

    expect(hoisted.wrote).toEqual([
      { name: "notify", key: "events", value: "deploy", projectId: 8 },
    ]);
  });

  // A secret is written and never read back, so the second box is the only check on a typo there is.
  it("asks for a secret twice, and writes nothing when the two do not match", async () => {
    hoisted.installs = [row({ name: "notify", config: [field({ key: "token", secret: true })] })];
    render();
    act(() => { button(t("plugins.cfg.open"))!.click(); });

    act(() => { type(boxes()[0], "shh"); });
    act(() => { type(boxes()[1], "shhh"); });
    await act(async () => { button(t("plugins.cfg.save"))!.click(); });
    expect(hoisted.wrote).toEqual([]);
    expect(container.textContent).toContain(t("plugins.cfg.secretMismatch"));

    act(() => { type(boxes()[1], "shh"); });
    await act(async () => { button(t("plugins.cfg.save"))!.click(); });
    // A secret is one value for the device: the tier the form is on does not reach it.
    expect(hoisted.wrote).toEqual([{ name: "notify", key: "token", value: "shh", projectId: null }]);
  });

  // Clearing is the same door as setting: an empty value is "not provided", which is what `required` reads.
  it("clears a held setting with an empty value", async () => {
    hoisted.installs = [row({ name: "notify", config: [field({ key: "events", machineValue: "push" })] })];
    render();
    act(() => { button(t("plugins.cfg.open"))!.click(); });

    await act(async () => { button(t("plugins.cfg.clear"))!.click(); });
    expect(hoisted.wrote).toEqual([{ name: "notify", key: "events", value: "", projectId: null }]);
    expect(container.textContent).toContain(t("plugins.cfg.cleared"));
  });
});

// Uninstall is not disable (`AMB-D-357`): it takes the settings in every project, the secrets and the
// consent with it, and none of that comes back. So the question has to say so before anything is removed,
// and the answer has to say what actually went.
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
          t("plugins.removedPart.consent"),
          t("plugins.removedPart.runs"),
        ].join(t("common.listSeparator")),
      }),
    );
  });

  // A name that held nothing is not a failure — it is how a half-broken install gets cleaned up.
  it("says so when the name held nothing on this machine", async () => {
    hoisted.receipt = {
      wasEnabled: false, consent: false, machineDefaults: false, secrets: false,
      projectOverrides: 0, projectGates: 0, directory: false, runsLog: false, anything: false,
    };
    hoisted.installs = [row({ name: "notify" })];
    render();

    await act(async () => { button(t("plugins.remove"))!.click(); });
    expect(container.textContent).toContain(tf("plugins.removedNothing", { name: "notify" }));
  });
});
