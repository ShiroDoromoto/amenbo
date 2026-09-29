// @vitest-environment jsdom
// Which face a run's pane is turned to — its picture or its terminal (`AMB-T-5775`).
//
// **A run's pane opens on its picture**, every run and every time, and the reader's turn is kept for
// that run's pane alone: back up after a page turn it is as they left it, and the next run opens on
// its picture again — save while a step that may wait for a person runs, when it is turned to its
// terminal and turned back after (`AMB-T-5776`). **The terminal face stays mounted under the
// picture**, in the layout and dimmed, so the program in it is not ended by looking away and can be
// followed through the picture (`AMB-T-5832`). An ordinary pane
// has no picture and no control for one.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Say } from "../talk/nameplate";
import { TerminalPane } from "./TerminalPane";
import { t } from "../core/i18n";

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
  automation: "Nightly", run: 15, step: "check", automationId: 3, placement: 2, box: 2, builtin: false, interactive: false,
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

/** A pane, on a run or on none. */
async function pane(run: Say | null): Promise<void> {
  await act(async () => {
    root.render(createElement(TerminalPane, {
      frame: run === null ? "1" : `run-${run.run}`,
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

const faces = () => [...container.querySelectorAll<HTMLButtonElement>(".slot__face")];
const pressed = () => faces().find((one) => one.getAttribute("aria-pressed") === "true")?.textContent;
const picture = () => container.querySelector(".runpic");
const away = () => container.querySelector(".slot__body")?.classList.contains("slot__body--away");

describe("the face a run's pane is turned to", () => {
  it("opens on the picture, with the terminal face kept behind it", async () => {
    await pane(FAILED);
    expect(faces().map((one) => one.textContent)).toEqual([t("auto.run.facePicture"), t("auto.run.faceTerminal")]);
    expect(pressed()).toBe(t("auto.run.facePicture"));
    expect(picture()).not.toBeNull();
    expect(away(), "the terminal face under the picture was not dimmed").toBe(true);
    expect(picture()?.parentElement?.classList.contains("slot__body"), "the picture is not drawn over the terminal face").toBe(true);
  });

  it("turns to the terminal and back, and keeps the turn for that run's pane", async () => {
    await pane(FAILED);
    await act(async () => faces()[1]!.click());
    expect(pressed()).toBe(t("auto.run.faceTerminal"));
    expect(picture()).toBeNull();
    expect(away()).toBe(false);

    // The pane comes down with its page and goes up again for the same run.
    act(() => root.unmount());
    root = createRoot(container);
    await pane(FAILED);
    expect(pressed(), "a pane back up for the same run forgot the reader's turn").toBe(t("auto.run.faceTerminal"));

    // Another run opens on its picture, whatever the last one was turned to.
    await pane({ ...FAILED, run: 16 });
    expect(pressed()).toBe(t("auto.run.facePicture"));

    await pane(FAILED);
    await act(async () => faces()[0]!.click());
    expect(picture()).not.toBeNull();
  });

  it("is not a question on an ordinary pane", async () => {
    await pane(null);
    expect(faces()).toHaveLength(0);
    expect(picture()).toBeNull();
    expect(away()).toBe(false);
  });
});

/// A step that may wait for a person turns the pane to its terminal while it runs (`AMB-T-5776`).
describe("a step that may wait for a person", () => {
  const RUNNING: Say = {
    ...FAILED,
    run: 21,
    step: "ask",
    state: { ...FAILED.state!, status: "running", word: "Running" },
  };
  const ASKING: Say = { ...RUNNING, interactive: true };

  it("turns the pane to its terminal while it runs, and back to the picture after it", async () => {
    await pane(RUNNING);
    expect(pressed()).toBe(t("auto.run.facePicture"));
    await pane(ASKING);
    expect(pressed(), "a step asking a person was left behind the picture").toBe(t("auto.run.faceTerminal"));
    await pane({ ...RUNNING, step: "after" });
    expect(pressed()).toBe(t("auto.run.facePicture"));
  });

  it("leaves the terminal up after it where the reader had chosen the terminal", async () => {
    await pane({ ...RUNNING, run: 22 });
    await act(async () => faces()[1]!.click());
    await pane({ ...ASKING, run: 22 });
    await pane({ ...RUNNING, run: 22, step: "after" });
    expect(pressed()).toBe(t("auto.run.faceTerminal"));
  });

  it("keeps a press made while it runs as the reader's choice", async () => {
    await pane({ ...ASKING, run: 23 });
    await act(async () => faces()[0]!.click());
    expect(pressed()).toBe(t("auto.run.facePicture"));
    await pane({ ...RUNNING, run: 23, step: "after" });
    expect(pressed()).toBe(t("auto.run.facePicture"));
  });

  it("does not turn a run that is not going", async () => {
    await pane({ ...ASKING, run: 24, state: { ...FAILED.state!, status: "paused", word: "Paused" } });
    expect(pressed()).toBe(t("auto.run.facePicture"));
  });
});
