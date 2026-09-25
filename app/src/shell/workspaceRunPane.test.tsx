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
import type { BuiltinRun, StepOpened, StepRun } from "../talk/automationStep";
import type { AutomationRunCardDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  /** Every opening the face asked for, in order. */
  opened: [] as PaneStart[],
  /** The sessions the face asked to be ended, in order. */
  ended: [] as string[],
  /** Whoever the face handed to `onStep`, so a test can put a step through it. */
  heard: null as ((one: StepOpened) => void) | null,
  /** The runs the pane asked to be stopped, in order. */
  stopped: [] as number[],
  /** The runs the pane asked to be paused, in order. */
  paused: [] as number[],
  /** The runs the pane asked to be picked up again, in order. */
  resumed: [] as number[],
  /** What the question above a removal is answered with. */
  says: true,
  /** The steps the host says are running as the face comes up (`standingSteps`). */
  standing: [] as StepOpened[],
  /** The runs the host answers the face's panes with (`useRunCards`). */
  cards: [] as AutomationRunCardDto[],
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
  useRunCards: () => hoisted.cards,
  stopRun: (run: number) => {
    hoisted.stopped.push(run);
    return Promise.resolve(true);
  },
  pauseRun: (run: number) => {
    hoisted.paused.push(run);
    return Promise.resolve();
  },
  resumeRun: (run: number) => {
    hoisted.resumed.push(run);
    return Promise.resolve();
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
import { t, tf } from "../core/i18n";

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
    automationName: "家計簿の開発ループ",
    name: "取る",
    task: { id: 5252, ref: "AMB-T-5252", title: "ペインのヘッダを描く", seq: 1 },
    session: "step-1",
    agent: "claude",
    folder: "/work/a",
    interactive: false,
    ...over,
  };
}

/** Run 7, as the host reads it for the pane. */
function runCard(over: Partial<AutomationRunCardDto> = {}): AutomationRunCardDto {
  return {
    run: 7,
    project: 1,
    projectName: "amenbo",
    automation: 3,
    automationName: "家計簿の開発ループ",
    status: "running",
    pauseRequested: false,
    waiting: false,
    stepName: "取る",
    stepsDone: 1,
    ...over,
  };
}

function builtin(over: Partial<BuiltinRun> = {}): BuiltinRun {
  return {
    seq: 2,
    automationName: "家計簿の開発ループ",
    name: "worktree を切る",
    key: "worktree_cut",
    task: { id: 5252, ref: "AMB-T-5252", title: "ペインのヘッダを描く", seq: 1 },
    finished: false,
    waiting: false,
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
  hoisted.paused = [];
  hoisted.resumed = [];
  hoisted.says = true;
  hoisted.standing = [];
  hoisted.cards = [];
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

    expect(q(".plate__step b")[0]?.textContent).toBe("取る");
    expect(q(".plate__step")[0]?.textContent).toContain("1");
    expect(q(".plate__no")[0]?.textContent).toContain("7");
    expect(q(".plate-run__task")[0]?.textContent).toBe("AMB-T-5252");
    expect(q(".plate-run__title")[0]?.textContent).toBe("ペインのヘッダを描く");
    expect(q(".plate__auto")[0]?.hidden).toBe(false);
    // Headed with the automation the run is running, not with the folder the place stands in.
    expect(q(".plate__name")[0]?.textContent).toBe("家計簿の開発ループ");
    // Drawn as a run's pane, in the run's colour (`AMB-T-5428`).
    expect(q(".slot--run")).toHaveLength(1);
  });

  it("says the task a step took after it opened, on the same terminal", async () => {
    // A step that takes its task opens with none, and the host tells the window again once it has
    // taken one (`crate::automation::retell_task`, `AMB-T-5427`). The step and its terminal are the
    // same ones, so the pane is not built again: only the row moves.
    await mount();
    await arrive({ step: step({ task: undefined }) });
    expect(q(".plate-run__task")[0]?.textContent ?? "").toBe("");
    const openings = hoisted.opened.length;

    await arrive();

    expect(q(".plate-run__task")[0]?.textContent).toBe("AMB-T-5252");
    expect(q(".plate-run__title")[0]?.textContent).toBe("ペインのヘッダを描く");
    expect(panes()).toHaveLength(1);
    expect(hoisted.opened).toHaveLength(openings);
    expect(hoisted.ended).toEqual([]);
  });

  it("says nothing of the run's state before the run has been read", async () => {
    await mount();
    await arrive();

    expect(q(".plate__state")[0]?.hidden).toBe(true);
    expect(q(".plate-fail")[0]?.hidden).toBe(true);
  });

  it("says the run is running, and that it has completed once it has, on the same pane", async () => {
    // A run's pane outlives its last step: a completed run's pane went on naming the step it ended
    // on, and a reader took it for a run stopped partway (`AMB-T-5506`).
    hoisted.cards = [runCard()];
    await mount();
    await arrive();
    expect(q(".plate__state")[0]?.hidden).toBe(false);
    expect(q(".plate__state")[0]?.textContent).toBe(t("auto.run.running"));
    expect(q(".plate__state")[0]?.dataset.state).toBe("running");
    const openings = hoisted.opened.length;

    hoisted.cards = [runCard({ status: "completed", exitName: "" })];
    await arrive();

    expect(q(".plate__state")[0]?.textContent).toBe(t("auto.run.completed"));
    expect(q(".plate__state")[0]?.dataset.state).toBe("completed");
    expect(q(".plate-fail")[0]?.hidden).toBe(true);
    // Only the row moved: the step and its terminal are the same ones.
    expect(hoisted.opened).toHaveLength(openings);
  });

  it("says why a failed run failed, and at which step and way out", async () => {
    hoisted.cards = [runCard({
      status: "failed",
      stoppedReason: "halted",
      actionName: "下ごしらえ",
      exitName: "*",
    })];
    await mount();
    await arrive();

    expect(q(".plate__state")[0]?.textContent).toBe(t("auto.run.failed"));
    expect(q(".plate-fail")[0]?.hidden).toBe(false);
    expect(q(".plate-fail__why")[0]?.textContent).toBe(t("auto.run.halted"));
    expect(q(".plate-fail__where")[0]?.textContent).toBe(tf("face.runFailedAt", {
      step: tf("auto.run.inAction", { action: "下ごしらえ", step: "取る" }),
      exit: t("auto.pic.errorExit"),
    }));
  });

  it("says the step alone where a failed run's step left by no way out", async () => {
    // A program that exited before it reported left by none (`crashed`).
    hoisted.cards = [runCard({ status: "failed", stoppedReason: "crashed" })];
    await mount();
    await arrive();

    expect(q(".plate-fail__why")[0]?.textContent).toBe(t("auto.run.crashed"));
    expect(q(".plate-fail__where")[0]?.textContent).toBe("取る");
  });

  it("holds and stops a going run from its pane, in the running tab's words", async () => {
    // A reader watching a run is in its pane, and had to go to the running tab to act on it
    // (`AMB-T-5507`). Stopping from here keeps the pane: that is the end control's, and it asks first.
    hoisted.cards = [runCard()];
    await mount();
    await arrive();
    const acts = () => q(".slot__runact");
    expect(acts().map((b) => b.textContent)).toEqual([t("auto.run.pause"), t("auto.run.stop")]);

    await act(async () => {
      acts()[0]!.click();
      await new Promise((r) => setTimeout(r, 0));
    });
    expect(hoisted.paused).toEqual([7]);

    await act(async () => {
      acts()[1]!.click();
      await new Promise((r) => setTimeout(r, 0));
    });
    expect(hoisted.stopped).toEqual([7]);
    expect(panes()).toHaveLength(1);
  });

  it("will not ask for a pause twice while one is on its way", async () => {
    hoisted.cards = [runCard({ pauseRequested: true })];
    await mount();
    await arrive();

    expect((q(".slot__runact")[0] as HTMLButtonElement).disabled).toBe(true);
    expect((q(".slot__runact")[1] as HTMLButtonElement).disabled).toBe(false);
  });

  it("picks a held run up again from its pane", async () => {
    hoisted.cards = [runCard({ status: "paused" })];
    await mount();
    await arrive();
    expect(q(".slot__runact").map((b) => b.textContent)).toEqual([t("auto.run.resume"), t("auto.run.stop")]);

    await act(async () => {
      q(".slot__runact")[0]!.click();
      await new Promise((r) => setTimeout(r, 0));
    });
    expect(hoisted.resumed).toEqual([7]);
  });

  it("offers no moves on a run that is over, or not read yet", async () => {
    await mount();
    await arrive();
    expect(q(".slot__runact")).toHaveLength(0);

    hoisted.cards = [runCard({ status: "completed", exitName: "" })];
    await arrive();
    expect(q(".slot__runact")).toHaveLength(0);
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

describe("a built-in on a run's pane", () => {
  /** The card a built-in stands on, where one is drawn. */
  const card = () => q(".slot__builtin");

  it("stands the pane on the card when the run starts with one, and opens no terminal", async () => {
    // A run that starts with a built-in has no terminal to stand its pane on, and the pane is stood
    // at once all the same (`AMB-D-964`).
    await mount();
    await arrive({ step: undefined, builtin: builtin({ seq: 1, task: undefined }) });

    expect(panes()).toHaveLength(1);
    expect(card()).toHaveLength(1);
    expect(q(".slot__builtin-name")[0]?.textContent).toBe("worktree を切る");
    expect(hoisted.opened).toHaveLength(0);
    // The row above says the run and the built-in, as it says a step.
    expect(q(".plate__step b")[0]?.textContent).toBe("worktree を切る");
    expect(q(".slot--run")).toHaveLength(1);
  });

  it("takes the place of the step before it, and writes itself over when it is done", async () => {
    await mount();
    await arrive();
    await arrive({ step: undefined, builtin: builtin() });

    expect(panes()).toHaveLength(1);
    expect(card()).toHaveLength(1);
    expect(q(".workspace__face")).toHaveLength(0);
    expect(q(".slot__builtin-ref")[0]?.textContent).toBe("AMB-T-5252");
    const doing = q(".slot__builtin-state")[0]?.textContent;

    await arrive({ step: undefined, builtin: builtin({ finished: true }) });

    // One card, said again — not a second one stacked under it.
    expect(card()).toHaveLength(1);
    expect(q(".slot__builtin-state")[0]?.textContent).not.toBe(doing);
  });

  it("says which way out a built-in left by once it has been carried out", async () => {
    await mount();
    await arrive({ step: undefined, builtin: builtin() });
    expect(q(".slot__builtin-exit")).toHaveLength(0);

    await arrive({ step: undefined, builtin: builtin({ finished: true, exitName: "*" }) });

    expect(q(".slot__builtin-exit")[0]?.textContent)
      .toBe(tf("face.builtinExit", { exit: t("auto.pic.errorExit") }));
  });

  it("says it is waiting and what for, and names no task, while a built-in waits for one", async () => {
    await mount();
    await arrive({
      step: undefined,
      builtin: builtin({
        name: "タスクに着手する",
        key: "take_task",
        task: undefined,
        waiting: true,
        looksFor: "assignee:me-ai status:todo ready:yes",
      }),
    });

    expect(card()).toHaveLength(1);
    expect(q(".slot__builtin-state")[0]?.textContent).toBe("Waiting for a task it can take");
    expect(q(".slot__builtin-filter")[0]?.textContent).toBe("assignee:me-ai status:todo ready:yes");
    expect(q(".slot__builtin-task")).toHaveLength(0);
  });

  it("gives the pane back to a terminal when the next step arrives", async () => {
    await mount();
    await arrive({ step: undefined, builtin: builtin({ seq: 1 }) });

    await arrive({ step: step({ runStep: 2, seq: 2, session: "step-2" }) });

    expect(panes()).toHaveLength(1);
    expect(card()).toHaveLength(0);
    expect(hoisted.opened[hoisted.opened.length - 1]!.session).toBe("step-2");
  });

  it("stands for a built-in told before the face was up", async () => {
    hoisted.standing = [{ run: 7, project: 1, builtin: builtin(), missing: [] }];
    await mount();

    expect(panes()).toHaveLength(1);
    expect(card()).toHaveLength(1);
  });
});
