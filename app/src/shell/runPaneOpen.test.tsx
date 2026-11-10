// @vitest-environment jsdom
// Whether a pane with no terminal in it offers to open one (`AMB-T-5667`).
//
// A pane kept from the last time the app was up offers it: that is how a person starts a session in
// a place. **A run's pane does not** — one come back with the app for a run held or failed has no
// terminal either, but one opened by hand there would be an ordinary session with nothing to do with
// the run, under a row that goes on naming the run's step.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Say } from "../talk/nameplate";
import { TerminalPane } from "./TerminalPane";
import { t, tf } from "../core/i18n";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const hoisted = vi.hoisted(() => ({
  /** Every script tail a pane drew, in order. */
  tails: [] as string[],
  /** The lines Amenbo wrote under each of them, in the same order. */
  after: [] as (readonly string[])[],
}));

// Nothing here opens a terminal, so what would draw one is stubbed out of the way.
vi.mock("../talk/agent", () => ({ mountAgentFrame: () => Promise.resolve(() => {}) }));
vi.mock("../talk/terminal", async (actual) => ({
  ...(await actual<typeof import("../talk/terminal")>()),
  endTerminal: vi.fn(async () => {}),
  pasteIntoTerminal: vi.fn(async () => {}),
  mountTail: (_host: HTMLElement, tail: string, after: readonly string[] = []) => {
    hoisted.tails.push(tail);
    hoisted.after.push(after);
    return () => {};
  },
}));
// The project's roles, as the line saying where a failure was filed reads them (`./RunPicture`).
vi.mock("../core/projectRoles", () => ({
  useProjectRoles: () => [
    { id: 12, name: "Bob", icon: null, automationId: 3, automationName: "Nightly", automationArchived: false },
    { id: 14, name: "Dave", icon: null, automationId: 3, automationName: "Nightly", automationArchived: false },
  ],
}));
// The row is a live thing of its own; what it draws is not what this is about.
vi.mock("../talk/plate", () => ({
  mountPlate: () => ({
    opened: () => {}, closed: () => {}, named: () => {}, took: () => {}, stated: () => {}, numbered: () => {}, timed: () => {},
    focused: () => {}, stop: () => {}, lastOutput: () => null,
  }),
}));

/** A run that failed at its second step, as a pane come back with the app is handed it. */
const FAILED: Say = {
  automation: "Nightly", run: 15, step: "check", automationId: 3, placement: 2, box: 2, builtin: false, interactive: false, by: null, command: null, startedAt: null,
  action: null, task: null,
  state: { status: "failed", word: "Failed", pauseRequested: false, pauseBeforeNextTask: false, pausableBeforeNextTask: false, why: null, exit: null, errorExit: false, acknowledged: false },
};

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  hoisted.tails = [];
  hoisted.after = [];
  window.innerWidth = 1600;
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

/** A pane with no terminal in it, on a run or on none — with what a script step wrote, where one did. */
async function pane(run: Say | null, tail: string | null = null, tailCode: number | null = null): Promise<void> {
  await act(async () => {
    root.render(createElement(TerminalPane, {
      frame: run === null ? "1" : "run-15",
      hue: 199,
      project: 1,
      names: new Map(),
      start: { cwd: "/work/here" },
      autoStart: false,
      focused: true,
      run,
      tail,
      tailCode,
      onOpened: () => {},
      onSaid: () => {},
      onPath: () => {},
      onClosed: () => {},
      onDrop: () => {},
      onName: () => {},
      written: "",
      onWrite: () => {},
      composeOpen: false,
      onFold: () => {},
      onFocus: () => {},
    }));
  });
}

const openHere = () => container.querySelector(".slot__open");

describe("opening a terminal in a pane with none in it", () => {
  it("is offered on a place kept from the last run", async () => {
    await pane(null);
    expect(openHere(), "a place with nothing in it had no way to start a session").not.toBeNull();
  });

  it("is not offered on a run's pane", async () => {
    await pane(FAILED);
    expect(openHere(), "a run's pane offered a session that has nothing to do with the run").toBeNull();
  });

  it("says in its body how a run come back failed ended, and how a held one goes on (AMB-T-5753)", async () => {
    await pane(FAILED);
    expect(container.querySelector(".slot__runbody-line")?.textContent).toBe(
      tf("auto.run.body.failed", { step: "check" }),
    );

    // Come back with the app, a run's pane may know no step: the sentence names none rather than an
    // empty one.
    await pane({ ...FAILED, step: "" });
    expect(container.querySelector(".slot__runbody-line")?.textContent).toBe(t("auto.run.body.failedNoStep"));

    await pane({ ...FAILED, state: { ...FAILED.state!, status: "paused", word: "Paused" } });
    expect(container.querySelector(".slot__runbody-line")?.textContent).toBe(t("auto.run.body.paused"));
  });

  it("draws what a script step wrote, kept on the run, in place of the body (AMB-D-1016)", async () => {
    await pane(FAILED, "built 3 files\nerror: no such file");
    expect(hoisted.tails).toEqual(["built 3 files\nerror: no such file"]);
    expect(container.querySelector(".slot__runbody"), "the body stood over the script's tail").toBeNull();
  });

  it("writes under a failed round's kept screen, as its last lines, how its program ended and where the failure was filed", async () => {
    await pane(
      { ...FAILED, round: { role: 12, startedAt: null, incident: { id: 99, ref: "AMB-T-99", role: 14, person: false, at: null, seat: null } } },
      "uploading v2.4.0\nError: upload failed (HTTP 503)",
      1,
    );
    expect(hoisted.tails).toEqual(["uploading v2.4.0\nError: upload failed (HTTP 503)"]);
    expect(hoisted.after).toEqual([[
      tf("face.endedCode", { code: "1" }),
      tf("auto.run.term.filedRole", { to: "Dave" }),
    ]]);
    expect(container.querySelector(".workspace__note"), "nothing is said outside the terminal").toBeNull();
  });

  it("says the program ended without a code where none was kept", async () => {
    await pane(
      { ...FAILED, round: { role: 12, startedAt: null, incident: { id: 99, ref: "AMB-T-99", role: null, person: true, at: null, seat: null } } },
      "Error: upload failed (HTTP 503)",
    );
    expect(hoisted.after).toEqual([[t("face.endedNoCode"), t("auto.run.term.filedPerson")]]);
  });

  it("says where the failure was filed for a round that has no role, and so no role card", async () => {
    await pane(
      { ...FAILED, round: { role: null, startedAt: null, incident: { id: 99, ref: "AMB-T-99", role: 14, person: false, at: null, seat: null } } },
      "Error: upload failed (HTTP 503)",
      0,
    );
    expect(hoisted.after).toEqual([[
      tf("face.endedCode", { code: "0" }),
      tf("auto.run.term.filedRole", { to: "Dave" }),
    ]]);
  });

  it("writes nothing under a tail kept from a run that did not fail", async () => {
    await pane({ ...FAILED, state: { ...FAILED.state!, status: "completed", word: "Completed" } }, "built 3 files");
    expect(hoisted.after).toEqual([[]]);
  });
});
