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

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// Nothing here opens a terminal, so what would draw one is stubbed out of the way.
vi.mock("../talk/agent", () => ({ mountAgentFrame: () => Promise.resolve(() => {}) }));
vi.mock("../talk/terminal", async (actual) => ({
  ...(await actual<typeof import("../talk/terminal")>()),
  endTerminal: vi.fn(async () => {}),
  pasteIntoTerminal: vi.fn(async () => {}),
}));
// The row is a live thing of its own; what it draws is not what this is about.
vi.mock("../talk/plate", () => ({
  mountPlate: () => ({
    opened: () => {}, closed: () => {}, named: () => {}, took: () => {}, stated: () => {}, numbered: () => {},
    focused: () => {}, stop: () => {},
  }),
}));

/** A run that failed at its second step, as a pane come back with the app is handed it. */
const FAILED: Say = {
  automation: "Nightly", run: 15, step: "check", automationId: 3, placement: 2, box: 2, builtin: false,
  action: null, task: null,
  state: { status: "failed", word: "Failed", pauseRequested: false, why: null, exit: null, errorExit: false, acknowledged: false },
};

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  window.innerWidth = 1600;
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

/** A pane with no terminal in it, on a run or on none. */
async function pane(run: Say | null): Promise<void> {
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
});
