// @vitest-environment jsdom
// The pane an automation run is drawn in, and the terminal in it being swapped at every step.
//
// **One run is one pane.** A run is a line of steps and each step is a terminal of its own, so a pane
// per step would have the page rearrange itself under a reader at every report — the one thing the
// arrangement promises does not happen (`AMB-D-939`). The place stands still and what is running in
// it is what changes, which is invisible in code that looks right either way: a second pane is a
// working screen too, until somebody is reading the first one.
//
// **And it stands quietly.** A run opens its pane by itself, and may be a run in a project the reader
// is not looking at, so neither the pane being worked in nor the page moves. What says a pane has
// arrived is the ring, which is up for a moment and gone.
//
// **The pane takes up the terminal the host started.** A step's terminal is started, and the one
// before it ended, on the host whether or not the pane is drawn (`crate::pty::open_step`), so what the
// face does with a step is put its session on the run's place for the pane to draw.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PaneStart } from "../talk/terminal";
import type { StepOpened, StepRun } from "../talk/automationStep";

const hoisted = vi.hoisted(() => ({
  /** Every opening the face asked for, in order. */
  opened: [] as PaneStart[],
  /** The sessions the face asked to be ended, in order. */
  ended: [] as string[],
  /** Whoever the face handed to `onStep`, so a test can put a step through it. */
  heard: null as ((one: StepOpened) => void) | null,
  /** The runs the pane asked to be stopped, in order. */
  stopped: [] as number[],
  /** What the question above a removal is answered with. */
  says: true,
  /** The steps the host says are running as the face comes up (`standingSteps`). */
  standing: [] as StepOpened[],
}));

vi.mock("../talk/agent", () => ({
  mountAgentFrame: (
    _host: HTMLElement,
    _lang: string,
    on: { opened: (s: string) => void },
    start: PaneStart = {},
  ) => {
    hoisted.opened.push(start);
    on.opened(start.session ?? `s${hoisted.opened.length}`);
    return Promise.resolve(() => {});
  },
}));

vi.mock("../talk/automationStep", () => ({
  onStep: (fn: (one: StepOpened) => void) => {
    hoisted.heard = fn;
    return () => { hoisted.heard = null; };
  },
  standingSteps: () => Promise.resolve(hoisted.standing),
}));

vi.mock("../talk/terminal", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../talk/terminal")>()),
  endTerminal: (session: string) => {
    hoisted.ended.push(session);
    return Promise.resolve();
  },
}));

vi.mock("../talk/frames", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../talk/frames")>()),
  frameNames: async () => new Map(),
  nameFrame: async () => new Map(),
  savedLayout: async () => null,
  keepLayout: async () => {},
}));

vi.mock("../core/automations", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../core/automations")>()),
  stopRun: (run: number) => {
    hoisted.stopped.push(run);
    return Promise.resolve(true);
  },
}));

vi.mock("../core/dialog", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../core/dialog")>()),
  confirmDialog: () => Promise.resolve(hoisted.says),
}));

vi.mock("../files/FilesPanel", () => ({ FilesPanel: () => null }));
vi.mock("../files/FolderTree", () => ({ FolderTree: () => null }));
vi.mock("../mock/adapter", () => ({
  dataAdapter: { listProjects: () => [{ id: 1, name: "amenbo" }] },
}));
vi.mock("../core/boundFolders", () => ({
  useBoundFolders: () => ({
    all: [{ path: "/work/a", exists: true }],
    live: [{ path: "/work/a", exists: true }],
    answered: true,
  }),
}));

import { WorkspaceFace } from "./WorkspaceFace";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

const q = (sel: string) => [...container.querySelectorAll<HTMLElement>(sel)];
/** The places drawn on the page that is up. */
const panes = () => q(".slot:not(.slot--empty)");
/** Which place the face says is being worked in, or null where it says none. */
const worked = () => container.querySelector(".slot--focused")?.getAttribute("data-hand") ?? null;

/** Press the one control that takes a place away, on the pane that is up. */
async function close() {
  await act(async () => {
    q(".slot__end")[0]!.click();
    await new Promise((r) => setTimeout(r, 0));
  });
}

function step(over: Partial<StepRun> = {}): StepRun {
  return {
    runStep: 1,
    seq: 1,
    name: "取る",
    task: { id: 5252, ref: "AMB-T-5252", title: "ペインのヘッダを描く" },
    session: "step-1",
    agent: "claude",
    folder: "/work/a",
    interactive: false,
    ...over,
  };
}

/** Put a step of run 7 through the road the host tells the window on. */
async function arrive(one: Partial<StepOpened> = {}) {
  await act(async () => {
    hoisted.heard?.({ run: 7, project: 1, step: step(), missing: [], ...one });
    await new Promise((r) => setTimeout(r, 0));
  });
}

async function mount() {
  await act(async () => {
    root.render(createElement(WorkspaceFace, { onWindow: () => {}, note: null, projectId: 1 }));
    await new Promise((r) => setTimeout(r, 0));
  });
}

beforeEach(() => {
  localStorage.clear();
  window.innerWidth = 1600;
  hoisted.opened = [];
  hoisted.ended = [];
  hoisted.heard = null;
  hoisted.stopped = [];
  hoisted.says = true;
  hoisted.standing = [];
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the pane a run is drawn in", () => {
  it("stands on the first step and takes up the terminal the host started for it", async () => {
    await mount();
    expect(panes()).toHaveLength(0);

    await arrive();
    expect(panes()).toHaveLength(1);
    const start = hoisted.opened[hoisted.opened.length - 1]!;
    expect(start.session).toBe("step-1");
    expect(start.runStep).toBe(1);
    // What a person opens in the place after the step is a session of its own, with nothing of it
    // written on the frame: the place is the run's (`AMB-D-869`).
    expect(start.fresh).toBe(true);
    expect(start.agent).toBe("claude");
    expect(start.cwd).toBe("/work/a");
  });

  it("stands for a step told before the face was up", async () => {
    // A run started from the command line before the workspace was ever asked for has its step's
    // terminal running with nobody told (`crate::automation`'s `StepsStanding`).
    hoisted.standing = [{ run: 7, project: 1, step: step(), missing: [] }];
    await mount();

    expect(panes()).toHaveLength(1);
    expect(hoisted.opened[hoisted.opened.length - 1]!.session).toBe("step-1");
  });

  it("does not take the pane being worked in", async () => {
    await mount();
    await arrive();
    expect(worked()).toBe(null);
  });

  it("keeps one pane at the next step, and draws the next step's terminal in it", async () => {
    await mount();
    await arrive();

    await arrive({ step: step({ runStep: 2, name: "直す", session: "step-2" }) });
    expect(panes()).toHaveLength(1);
    // The host ended the terminal before it, so the face ends nothing itself.
    expect(hoisted.ended).toEqual([]);
    // The second pane is handed the second step's session, not the first one's: told to take up the
    // session before it, it would draw the step that is over.
    expect(hoisted.opened[hoisted.opened.length - 1]!.session).toBe("step-2");
  });

  it("touches nothing where the run was stopped instead", async () => {
    await mount();
    await arrive();
    const openings = hoisted.opened.length;

    await arrive({ step: undefined, missing: ["下書き"] });
    expect(panes()).toHaveLength(1);
    expect(hoisted.opened).toHaveLength(openings);
    // The terminal of the step before it is the whole of what a reader has to go on, so nothing here
    // ends it: that belongs to whatever stopped the run.
    expect(hoisted.ended).toEqual([]);
  });
});

describe("what the row above a run's pane says, and what closing it does", () => {
  it("says where the run has got to, and marks the pane as a run's", async () => {
    // The four are the ledger's and the execution row's, never the agent's word about itself
    // (`AMB-D-858`, `../talk/nameplate`).
    await mount();
    await arrive();

    expect(q(".plate-run__step")[0]?.textContent).toBe("取る");
    expect(q(".plate-run__seq")[0]?.textContent).toContain("1");
    expect(q(".plate-run__no")[0]?.textContent).toContain("7");
    expect(q(".plate-run__task")[0]?.textContent).toBe("AMB-T-5252");
    expect(q(".plate__auto")[0]?.hidden).toBe(false);
  });

  it("stops the run when the pane is closed, and takes the place away", async () => {
    // A run left going with its pane gone is one nobody can see, reach or stop. What it was holding
    // is handed back by core, which is why nothing of that is worked out here (`AMB-T-5247`).
    await mount();
    await arrive();

    await close();

    expect(hoisted.stopped).toEqual([7]);
    expect(panes()).toHaveLength(0);
    // The terminal ends too, and after the stop: a step whose terminal had gone first would have
    // reported nothing either way.
    expect(hoisted.ended).toEqual(["step-1"]);
  });

  it("stops nothing where the question was answered no", async () => {
    await mount();
    await arrive();
    hoisted.says = false;

    await close();

    expect(hoisted.stopped).toEqual([]);
    expect(panes()).toHaveLength(1);
  });

  it("stops nothing when an ordinary pane is closed", async () => {
    // The way out of an ordinary pane is what it always was. Only a pane standing for a run has a
    // run to stop, and a face that asked anyway would be stopping runs nobody started.
    await mount();
    await act(async () => {
      container.querySelector<HTMLElement>(".slot--empty .slot__open")!
        .dispatchEvent(new MouseEvent("click", { bubbles: true }));
      await new Promise((r) => setTimeout(r, 0));
    });
    expect(panes(), "the pane a person opened is up").toHaveLength(1);

    await close();

    expect(hoisted.stopped).toEqual([]);
    expect(panes()).toHaveLength(0);
  });
});
