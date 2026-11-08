// @vitest-environment jsdom
// Which face a run's pane is turned to — its roles or its terminal (`./RunPicture`).
//
// **A run's pane opens on its roles**, every run and every time, and the reader's turn is kept for
// that run's pane alone: put up again on another page it is as they left it, and the next run opens on
// its roles again — save while a step that may wait for a person runs, when it is turned to its
// terminal and turned back after (`AMB-T-5776`). **The terminal face stays mounted under the roles**,
// so the program in it is not ended by looking away. An ordinary pane has no roles and no control for
// them.
//
// **A role with a task is a desk, and the rest are on the bar** — the desks in the order the roles are
// defined in, whatever their seats are doing. A desk's head says the role, how many seats are at work
// out of its headcount and a square per seat; under it a row per seat at work, the free seats as one
// row, and the tasks waiting for it. There is no choice between cards and a list.
//
// **A round that needs a person, out of sight in the room, is named at the room's edge**, and the
// notice scrolls the room to it when pressed — never by itself. It is read again as the room scrolls.
//
// **A seat is pressed to read its round's terminal**, in the same pane. The terminal has a bar over
// it: the way back to the roles — the only way, the row above has no control for it — the role, the
// seat, the task, the step, how long it has run, and — where a step turned the pane — why. It names no
// other seat.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Say } from "../talk/nameplate";
import type { AutomationLaunchRoleStateDto, AutomationLaunchStateDto, AutomationRunCardDto, TaskRoleDto } from "../bindings/bindings";
import type { BuiltinRun } from "../talk/automationStep";
import { TerminalPane } from "./TerminalPane";
import { t, tf } from "../core/i18n";
import {
  attnOf, desksOf, filedLine, minutesOf, noticeOf, runDesks, seatOf, seatStateOf, silenceOf, unseenOf,
} from "./RunPicture";
import { showTerminalOf } from "./runFaces";

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
    opened: () => {}, closed: () => {}, named: () => {}, took: () => {}, stated: () => {}, numbered: () => {}, timed: () => {},
    focused: () => {}, stop: () => {}, lastOutput: () => null,
  }),
}));

/** The project's roles, as the pane reads them (`../core/projectRoles`). */
let roles: TaskRoleDto[] = [];
vi.mock("../core/projectRoles", () => ({ useProjectRoles: () => roles }));

/** The launch the pane draws, as its roles and the bar over its terminal read it (`../core/automations`). */
let launched: AutomationLaunchStateDto | null = null;
vi.mock("../core/automations", async (actual) => ({
  ...(await actual<typeof import("../core/automations")>()),
  useLaunchState: (launch: number | null) => (launch === null ? null : launched),
}));

/** A run under way of the automation `automation`, as a seat of a launch carries it. */
const liveRun = (run: number, more: Partial<AutomationRunCardDto> = {}): AutomationRunCardDto => ({
  run, project: 1, projectName: "P", automation: 3, automationName: "Nightly", status: "running",
  pauseRequested: false, pauseBeforeNextTask: false, pausableBeforeNextTask: false, waiting: false, waitsForRoom: false,
  stepsDone: 1, passes: [], reportWithheld: [], acknowledged: false, ...more,
} as AutomationRunCardDto);

/** A role of a launch, with the runs `runs` in its seats. */
const launchRole = (role: number, name: string, runs: AutomationRunCardDto[], headcount = runs.length): AutomationLaunchRoleStateDto => ({
  role, name, defined: headcount, headcount, underWay: runs.length, toLetGo: 0, held: false, runs, waiting: 0, firstWaiting: [],
});

/** The launch 5, with the roles `roles`. */
const launchOf = (roles: AutomationLaunchRoleStateDto[]): AutomationLaunchStateDto => ({
  launch: 5, project: 1, automation: 3, automationName: "Nightly", status: "running", maxHeadcount: 10, roles,
});

const role = (id: number, name: string, automationId = 3, icon: string | null = null): TaskRoleDto => ({
  id, name, icon, automationId, automationName: "Nightly",
});

/** A run that failed at its second step, as a pane come back with the app is handed it. */
const FAILED: Say = {
  automation: "Nightly", run: 15, step: "check", automationId: 3, placement: 2, box: 2, builtin: false, interactive: false, by: null, command: null, startedAt: null,
  action: null, task: null,
  state: { status: "failed", word: "Failed", pauseRequested: false, pauseBeforeNextTask: false, pausableBeforeNextTask: false, why: null, exit: null, errorExit: false, acknowledged: false },
};

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  roles = [];
  launched = null;
  window.innerWidth = 1600;
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

/** A pane, on a run or on none — with a terminal put up where `autoStart`, or a built-in in its place. */
async function pane(
  run: Say | null,
  { autoStart = false, builtin = null, launch = null, onGoToRun }: {
    autoStart?: boolean; builtin?: BuiltinRun | null; launch?: number | null; onGoToRun?: (run: number) => void;
  } = {},
): Promise<void> {
  await act(async () => {
    root.render(createElement(TerminalPane, {
      frame: run === null ? "1" : launch === null ? `run-${run.run}` : `launch-${launch}`,
      launch,
      hue: 199,
      project: 1,
      names: new Map(),
      start: { cwd: "/work/here" },
      autoStart,
      focused: true,
      run,
      builtin,
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
      onGoToRun,
    }));
  });
}

const shown = () => container.querySelector(".runroles");
/** The face the pane is turned to: the roles while they are drawn, and the terminal otherwise. */
const pressed = () => (shown() === null ? "terminal" : "roles");
/** Turn the pane to the terminal, as a reader does: the first seat pressed. */
const toTerminal = () => container.querySelector<HTMLButtonElement>("button.runseat")!.click();
/** Turn the pane back to the roles, as a reader does: the way back on the bar over the terminal. */
const toRoles = () => container.querySelector<HTMLButtonElement>(".runterm__back")!.click();
const desks = () => [...container.querySelectorAll<HTMLElement>(".runroles__desk")];
const idle = () => [...container.querySelectorAll<HTMLElement>(".runroles__idleone")];
/** The seats at work, as rows pressed for their terminal. */
const seats = () => [...container.querySelectorAll<HTMLButtonElement>("button.runseat")];

describe("the face a run's pane is turned to", () => {
  it("opens on the roles, with the terminal face kept behind them", async () => {
    await pane(FAILED);
    expect(container.querySelector(".slot__faces"), "the row above offers the roles or the terminal").toBeNull();
    expect(pressed()).toBe("roles");
    expect(shown()).not.toBeNull();
    expect(shown()?.parentElement?.classList.contains("slot__body"), "the roles are not drawn over the terminal face").toBe(true);
  });

  // The body is clipped where the roles or a card stand straight in it (`styles/global.css`), so that
  // in a pane an eighth of the page the card's "!" and the roles stay inside the frame. jsdom lays
  // nothing out, so what is pinned is the shape that rule asks for.
  it("stands the roles and the failed run's card straight in the body the clip is drawn on", async () => {
    await pane(FAILED);
    const body = container.querySelector(".slot__body");
    expect(body?.querySelector(":scope > .runroles"), "the roles are not a child of the body").not.toBeNull();
    expect(body?.querySelector(":scope > .slot__builtin.slot__runbody"), "the run's card is not a child of the body").not.toBeNull();
  });

  it("turns to the terminal and back, and keeps the turn for that run's pane", async () => {
    await pane(FAILED);
    await act(async () => toTerminal());
    expect(pressed()).toBe("terminal");
    expect(shown()).toBeNull();

    // The pane comes down with its page and goes up again for the same run.
    act(() => root.unmount());
    root = createRoot(container);
    await pane(FAILED);
    expect(pressed(), "a pane back up for the same run forgot the reader's turn").toBe("terminal");

    // Another run opens on its roles, whatever the last one was turned to.
    await pane({ ...FAILED, run: 16 });
    expect(pressed()).toBe("roles");

    await pane(FAILED);
    await act(async () => toRoles());
    expect(shown()).not.toBeNull();
  });

  it("is not a question on an ordinary pane", async () => {
    await pane(null);
    expect(container.querySelector(".runterm"), "an ordinary pane has a way back to roles").toBeNull();
    expect(shown()).toBeNull();
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

  it("turns the pane to its terminal while it runs, and back to the roles after it", async () => {
    await pane(RUNNING);
    expect(pressed()).toBe("roles");
    await pane(ASKING);
    expect(pressed(), "a step asking a person was left behind the roles").toBe("terminal");
    await pane({ ...RUNNING, step: "after" });
    expect(pressed()).toBe("roles");
  });

  it("leaves the terminal up after it where the reader had chosen the terminal", async () => {
    await pane({ ...RUNNING, run: 22 });
    await act(async () => toTerminal());
    await pane({ ...ASKING, run: 22 });
    await pane({ ...RUNNING, run: 22, step: "after" });
    expect(pressed()).toBe("terminal");
  });

  it("keeps a press made while it runs as the reader's choice", async () => {
    await pane({ ...ASKING, run: 23 });
    await act(async () => toRoles());
    expect(pressed()).toBe("roles");
    await pane({ ...RUNNING, run: 23, step: "after" });
    expect(pressed()).toBe("roles");
  });

  it("does not turn a run that is not going", async () => {
    await pane({ ...ASKING, run: 24, state: { ...FAILED.state!, status: "paused", word: "Paused" } });
    expect(pressed()).toBe("roles");
  });
});

/// A role with a task is a desk, and the rest are on the bar.
describe("the desks of a launch", () => {
  const FRONT: Say = {
    ...FAILED,
    run: 31,
    step: "write the tests",
    task: { ref: "AMB-T-7", title: "Fix the search filter", seq: 1 },
    round: { role: 12, startedAt: Date.now() - 65_000 },
    state: { ...FAILED.state!, status: "running", word: "Running" },
  };
  const started = new Date(Date.now() - 65_000).toISOString();
  const task = (id: number, title: string) => ({ id, ref: `AMB-T-${id}`, title, seq: 1 });
  const waiting = (id: number, title: string, fromRole?: number) => ({
    task: id, taskRef: `AMB-T-${id}`, title, ...(fromRole === undefined ? {} : { fromRole }),
  });

  /** A launch of four roles: the first with two at work and a seat free, the second with nobody at
   *  work and tasks waiting, the third with nothing, and the fourth, held, with one at work. */
  const busy = (): AutomationLaunchStateDto => launchOf([
    {
      ...launchRole(12, "Bob", [
        liveRun(31, { role: 12, task: task(7, "Fix the search filter"), stepName: "write the tests", roleStartedAt: started,
          passes: [
            { fromRole: 12, toRole: 13, task: 8, taskRef: "AMB-T-8", taskTitle: "Review the filter", current: true },
            { fromRole: 12, toRole: 14, task: 2, taskRef: "AMB-T-2", taskTitle: "Earlier", current: false },
          ] }),
        liveRun(32, { role: 12, task: task(9, "Tidy the logs"), stepName: "pick an option", interactive: true, roleStartedAt: started }),
      ], 3),
      icon: "seed-bob",
    },
    {
      ...launchRole(13, "Carol", [], 1),
      waiting: 5,
      firstWaiting: [waiting(8, "Review the filter", 12), waiting(10, "Check the docs"), waiting(11, "Read the plan")],
    },
    launchRole(14, "山田", [], 2),
    { ...launchRole(15, "さくら", [liveRun(33, { role: 15, task: task(20, "Draw the icon"), stepName: "sketch", roleStartedAt: started })], 2), held: true },
  ]);

  it("draws a desk per role with a task, in the order they are defined in, and the rest on the bar with their headcount", async () => {
    launched = busy();
    await pane(FRONT, { launch: 5 });
    expect(desks().map((one) => one.dataset.role)).toEqual(["12", "13", "15"]);
    expect(idle().map((one) => [one.dataset.role, one.textContent])).toEqual([
      ["14", `山田${tf("auto.run.roles.headcount", { n: 2 })}`],
    ]);
    expect(idle()[0]!.querySelector("svg.identicon"), "a role on the bar is drawn by its seed").not.toBeNull();
    expect(container.querySelector(".runroles__idlecount")?.textContent).toBe(tf("auto.run.roles.idle", { n: 1 }));
    expect(container.querySelector(".slot__views"), "the room offers cards or a list").toBeNull();
  });

  it("heads a desk with the role, how many are at work of its headcount, a square per seat and its menu", async () => {
    launched = busy();
    await pane(FRONT, { launch: 5 });
    const bob = desks()[0]!;
    expect(bob.querySelector(".runroles__head > svg.identicon")).not.toBeNull();
    expect(bob.querySelector(".runroles__name")?.textContent).toBe("Bob");
    expect(bob.querySelector(".runroles__staff")?.textContent).toBe(tf("auto.run.roles.staff", { n: 2, m: 3 }));
    expect([...bob.querySelectorAll<HTMLElement>(".runroles__squares > i")].map((one) => one.dataset.state ?? "free"))
      .toEqual(["on", "asks", "free"]);
    const more = bob.querySelector<HTMLButtonElement>(".runroles__more")!;
    expect([more.getAttribute("aria-haspopup"), more.getAttribute("aria-label")]).toEqual(["dialog", tf("auto.run.roles.menu", { name: "Bob" })]);
    expect(bob.querySelector(".runroles__held"), "a role taking tasks says it stops").toBeNull();
  });

  it("counts no failed or paused seat as at work, and frames a failed seat's square in the stop colour", async () => {
    launched = launchOf([launchRole(12, "Bob", [
      liveRun(31, { role: 12, status: "failed" }),
      liveRun(32, { role: 12, status: "paused" }),
      liveRun(33, { role: 12 }),
    ], 3)]);
    await pane(FRONT, { launch: 5 });
    const bob = desks()[0]!;
    expect(bob.querySelector(".runroles__staff")?.textContent).toBe(tf("auto.run.roles.staff", { n: 1, m: 3 }));
    expect([...bob.querySelectorAll<HTMLElement>(".runroles__squares > i")].map((one) => one.dataset.state ?? "free"))
      .toEqual(["failed", "free", "on"]);
  });

  it("says each seat at work, marks the one that may wait for a person, and the role it passed a task on to", async () => {
    launched = busy();
    await pane(FRONT, { launch: 5 });
    const bob = desks()[0]!;
    expect(bob.dataset.state, "a desk with a seat that may wait is not marked").toBe("asks");
    const [first, second] = [...bob.querySelectorAll<HTMLElement>("button.runseat")];
    expect(first!.querySelector(".runseat__task")?.textContent).toBe("AMB-T-7 Fix the search filter");
    expect(first!.querySelector(".runseat__step")?.textContent).toBe("write the tests");
    expect([...first!.querySelectorAll(".runseat__hand")].map((one) => one.textContent), "only this round's passes are said")
      .toEqual([tf("auto.run.roles.handsTo", { role: "Carol" })]);
    expect(first!.querySelector(".runseat__time")?.textContent).toBe(tf("face.elapsed", { time: minutesOf(65_000) }));
    expect(first!.querySelector(".runroles__pulse")?.getAttribute("aria-label")).toBe(t("auto.run.roles.working"));

    // Another seat's step that may wait for a person is read off its own row.
    expect(second!.dataset.state).toBe("asks");
    expect(second!.querySelector(".runseat__chip")?.textContent).toBe(t("auto.run.roles.asks"));
    expect(second!.querySelector(".runseat__time"), "a seat that may wait counted its time").toBeNull();
    expect(second!.querySelector(".runroles__pulse"), "a seat that may wait has the dot").toBeNull();

    const free = bob.querySelector(".runseat--free");
    expect(free?.tagName).toBe("DIV");
    expect(free?.textContent).toBe(`1${tf("auto.run.seatsFree", { n: 1 })}`);
  });

  it("counts the tasks waiting for a role, the first three of them and how many more, and says which was passed on", async () => {
    launched = busy();
    await pane(FRONT, { launch: 5 });
    const carol = desks()[1]!;
    expect(carol.querySelector(".runroles__staff")?.textContent).toBe(tf("auto.run.roles.staff", { n: 0, m: 1 }));
    expect(carol.querySelectorAll("button.runseat")).toHaveLength(0);
    expect(carol.querySelector(".runseat--free")?.textContent).toBe(`1${tf("auto.run.seatsFree", { n: 1 })}`);
    const head = carol.querySelector(".runroles__stackhead")!;
    expect(head.textContent).toBe(tf("auto.run.roles.waiting", { n: 5 }));
    expect(head.querySelector("b")?.textContent).toBe("5");
    const lines = [...carol.querySelectorAll<HTMLElement>(".runroles__waiting")];
    expect(lines.map((one) => one.textContent)).toEqual([
      `AMB-T-8 Review the filter\u3000${tf("auto.run.roles.from", { role: "Bob" })}`,
      "AMB-T-10 Check the docs",
      "AMB-T-11 Read the plan",
      tf("auto.run.roles.more", { n: 2 }),
    ]);
    expect(lines.map((one) => one.dataset.from ?? null)).toEqual(["true", null, null, null]);
  });

  it("says a held role stops after the task, on its head, its seats at work and its free seats", async () => {
    launched = busy();
    await pane(FRONT, { launch: 5 });
    const sakura = desks()[2]!;
    expect(sakura.dataset.held).toBe("true");
    expect(sakura.querySelector(".runroles__held")?.textContent).toBe(t("auto.run.roles.held"));
    expect(sakura.querySelector("button.runseat .runseat__last")?.textContent).toBe(t("auto.run.roles.lastTask"));
    expect(sakura.querySelector(".runseat--free")?.textContent).toBe(`1${tf("auto.run.seatsFreeHeld", { n: 1 })}`);
    expect(desks()[0]!.querySelector(".runseat__last"), "a role taking tasks said it stops").toBeNull();
  });

  it("draws every role as held while the launch is paused", async () => {
    launched = { ...busy(), status: "paused" };
    await pane(FRONT, { launch: 5 });
    expect(desks().map((one) => one.dataset.held)).toEqual(["true", "true", "true"]);
  });

  it("says no role has a task where none has, and nothing while the launch is read", async () => {
    launched = launchOf([launchRole(12, "Bob", [], 1)]);
    await pane(FRONT, { launch: 5 });
    expect(desks()).toHaveLength(0);
    expect(container.querySelector(".runroles__none")?.textContent).toBe(t("auto.run.roles.none"));

    launched = null;
    await pane({ ...FRONT, run: 34 }, { launch: 5 });
    expect(desks(), "a launch not read yet drew the run alone").toHaveLength(0);
    expect(container.querySelector(".runroles__none")).toBeNull();
  });
});

describe("desksOf", () => {
  const FRONT: Say = { ...FAILED, run: 5, interactive: true, state: { ...FAILED.state!, status: "running", word: "Running" } };

  it("makes a desk of a role with a run in a seat or a task waiting, and puts the rest on the bar", () => {
    const got = desksOf(launchOf([
      launchRole(1, "A", [liveRun(4)], 2),
      launchRole(2, "B", [], 3),
      { ...launchRole(3, "C", [], 1), waiting: 1, firstWaiting: [{ task: 9, taskRef: "AMB-T-9", title: "T", fromRole: 1 }] },
      { ...launchRole(4, "D", [liveRun(6), liveRun(7)], 1), icon: "seed" },
    ]), FRONT);
    expect(got.desks.map((one) => [one.role, one.icon, one.seats.map((seat) => seat.run), one.free, one.waiting])).toEqual([
      [1, null, [4], 1, 0],
      [3, null, [], 1, 1],
      [4, "seed", [6, 7], 0, 0],
    ]);
    expect(got.desks[1]!.first).toEqual([{ task: 9, ref: "AMB-T-9", title: "T", from: "A" }]);
    expect(got.idle).toEqual([{ role: 2, name: "B", icon: null, headcount: 3 }]);
  });

  it("names the roles a seat's round passed a task on to once each, leaving out an earlier round's and a role gone", () => {
    const pass = (toRole: number | undefined, current: boolean) => ({ fromRole: 1, ...(toRole === undefined ? {} : { toRole }), current });
    const got = desksOf(launchOf([
      launchRole(1, "A", [liveRun(4, { passes: [pass(2, true), pass(2, true), pass(3, false), pass(99, true), pass(undefined, true)] })]),
      launchRole(2, "B", []),
      launchRole(3, "C", []),
    ]), FRONT);
    expect(got.desks[0]!.seats[0]!.to).toEqual(["B"]);
  });
});

describe("runDesks", () => {
  const RUNNING: Say = {
    ...FAILED,
    task: { ref: "AMB-T-7", title: "Fix", seq: 1 },
    round: { role: 2, startedAt: 1000 },
    state: { ...FAILED.state!, status: "running", word: "Running" },
  };

  it("draws the run's role as a desk with the run its one seat, and the rest on the bar, in the order they are defined in", () => {
    const got = runDesks(RUNNING, [role(3, "C"), role(2, "B", 3, "seed"), role(1, "A"), role(9, "Elsewhere", 9)]);
    expect(got.desks).toEqual([{
      role: 2, name: "B", icon: "seed", headcount: 1, held: false, free: 0, waiting: 0, first: [],
      seats: [{ run: 15, task: "AMB-T-7 Fix", step: "check", state: "running", since: 1000, to: [] }],
    }]);
    expect(got.idle.map((one) => [one.role, one.headcount])).toEqual([[3, null], [1, null]]);
  });

  it("draws a round under no role as one desk named after the automation, its roles on the bar", () => {
    const got = runDesks({ ...RUNNING, round: { role: null, startedAt: null } }, [role(2, "B"), role(3, "C")]);
    expect(got.desks.map((one) => [one.role, one.name])).toEqual([[null, "Nightly"]]);
    expect(got.idle.map((one) => one.role)).toEqual([2, 3]);
  });

  it("draws a round under no role at its automation's one role, where it has only one, and leaves the bar empty", () => {
    const failed = runDesks({ ...RUNNING, round: { role: null, startedAt: null }, state: FAILED.state }, [role(2, "B")]);
    expect(failed.desks.map((one) => [one.role, one.name])).toEqual([[2, "B"]]);
    expect(failed.idle).toEqual([]);
  });

  it("draws no desk while the run waits for its next task, or once it is over", () => {
    expect(runDesks({ ...RUNNING, state: { ...RUNNING.state!, waiting: true } }, [role(2, "B")]).desks).toEqual([]);
    expect(runDesks({ ...RUNNING, state: { ...RUNNING.state!, status: "completed" } }, [role(2, "B")]).desks).toEqual([]);
  });

  it("says how the run stands on its seat", () => {
    const state = (run: Say) => runDesks(run, [role(2, "B")]).desks[0]!.seats[0]!.state;
    expect(state({ ...RUNNING, interactive: true })).toBe("asks");
    expect(state({ ...RUNNING, state: FAILED.state })).toBe("failed");
    expect(state({ ...RUNNING, state: { ...RUNNING.state!, status: "paused" } })).toBe("paused");
  });
});

describe("minutesOf", () => {
  it("counts a round's time in whole minutes", () => {
    expect(minutesOf(59_000)).toBe(minutesOf(0));
    expect(minutesOf(6 * 60_000 + 59_000)).toBe(minutesOf(6 * 60_000));
    expect(minutesOf(41 * 60_000)).toContain("41");
    expect(minutesOf(-1000)).toBe(minutesOf(0));
  });
});

describe("silenceOf", () => {
  const at = 100_000_000;
  const min = 60_000;

  it("measures a round under way, the one that may wait for a person included", () => {
    expect(silenceOf("running", at - 6 * min, at, 10)).toEqual({ stage: 1, for: 6 * min });
    expect(silenceOf("asks", at - 21 * min, at, 10)).toEqual({ stage: 2, for: 21 * min });
  });

  it("says no time while the silence is short of the first stage", () => {
    expect(silenceOf("running", at - min, at, 10)).toEqual({ stage: 0, for: null });
  });

  it("does not measure a round that is paused or failed", () => {
    expect(silenceOf("paused", at - 60 * min, at, 10)).toEqual({ stage: 0, for: null });
    expect(silenceOf("failed", at - 60 * min, at, 10)).toEqual({ stage: 0, for: null });
  });
});

describe("filedLine", () => {
  const roles = [role(2, "B"), role(4, "D")];
  const failed = (incident: { ref: string; role: number | null; person: boolean } | undefined): Say => ({
    ...FAILED,
    round: { role: 2, startedAt: null, ...(incident === undefined ? {} : { incident }) },
  });

  it("names the role the failure was filed to", () => {
    expect(filedLine(failed({ ref: "AMB-T-9", role: 4, person: false }), roles)).toBe(
      tf("auto.run.roles.filedRole", { role: "B", to: "D", ref: "AMB-T-9" }),
    );
  });

  it("says a person where it went to one, and names nobody where the role is gone", () => {
    expect(filedLine(failed({ ref: "AMB-T-9", role: null, person: true }), roles)).toBe(
      tf("auto.run.roles.filedPerson", { role: "B", ref: "AMB-T-9" }),
    );
    expect(filedLine(failed({ ref: "AMB-T-9", role: 77, person: false }), roles)).toBe(
      tf("auto.run.roles.filed", { role: "B", ref: "AMB-T-9" }),
    );
  });

  it("has no line where the round filed nothing, or did not fail", () => {
    expect(filedLine(failed(undefined), roles)).toBeNull();
    const running: Say = {
      ...failed({ ref: "AMB-T-9", role: 4, person: false }),
      state: { ...FAILED.state!, status: "running", word: "Running" },
    };
    expect(filedLine(running, roles)).toBeNull();
  });
});

describe("a round out of sight", () => {
  const FAILING: Say = {
    ...FAILED,
    run: 71,
    step: "write the tests",
    task: { ref: "AMB-T-7", title: "Fix the search filter", seq: 1 },
    round: { role: 12, startedAt: null },
  };

  /** Where the mark — the seat's chip — stands on the page, and where the seat does; the room shows
   *  from 0 to 100. */
  let at = { top: 150, bottom: 250 };
  let seat = { top: 0, bottom: 0 };
  const scrollBy = vi.fn();
  const box = (top: number, bottom: number) =>
    ({ top, bottom, left: 0, right: 100, width: 100, height: bottom - top, x: 0, y: top, toJSON: () => ({}) }) as DOMRect;

  beforeEach(() => {
    at = { top: 150, bottom: 250 };
    seat = { top: 0, bottom: 0 };
    scrollBy.mockReset();
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
      if (this.classList.contains("runroles__room")) return box(0, 100);
      if (this.dataset.attn !== undefined) return box(at.top, at.bottom);
      if (this.classList.contains("runseat")) return box(seat.top, seat.bottom);
      return box(0, 0);
    });
    (HTMLElement.prototype as unknown as { scrollBy: unknown }).scrollBy = scrollBy;
  });

  afterEach(() => {
    vi.restoreAllMocks();
    delete (HTMLElement.prototype as unknown as { scrollBy?: unknown }).scrollBy;
  });

  const notices = () => [...container.querySelectorAll<HTMLButtonElement>(".runroles__unseen")];
  const marks = () => [...container.querySelectorAll<HTMLElement>("[data-attn]")];

  it("says what is below the room at its bottom edge, and scrolls to it only when pressed", async () => {
    roles = [role(12, "Bob")];
    await pane(FAILING);
    expect(seats()[0]!.dataset.attn, "the seat itself was the mark").toBeUndefined();
    expect(marks().map((one) => [one.className, one.dataset.attn])).toEqual([["runseat__chip", "failed"]]);
    const what = tf("auto.run.roles.attn.failed", { n: 1 });
    expect(notices().map((one) => [one.dataset.edge, one.textContent, one.dataset.stop])).toEqual([["bottom", `↓ ${what}`, "true"]]);
    expect(notices()[0]!.getAttribute("aria-label")).toBe(tf("auto.run.roles.unseenDown", { what }));
    expect(scrollBy, "the room scrolled by itself").not.toHaveBeenCalled();

    await act(async () => notices()[0]!.click());
    expect(scrollBy).toHaveBeenCalledWith({ top: 158, behavior: "smooth" });
  });

  it("is read again when the room scrolls, and not before", async () => {
    roles = [role(12, "Bob")];
    await pane({ ...FAILING, run: 72 });
    expect(notices()).toHaveLength(1);

    at = { top: 20, bottom: 90 };
    await pane({ ...FAILING, run: 72, step: "check" });
    expect(notices(), "the notice moved without a scroll").toHaveLength(1);

    const room = container.querySelector<HTMLElement>(".runroles__room")!;
    await act(async () => room.dispatchEvent(new Event("scroll")));
    expect(notices()).toHaveLength(0);

    at = { top: -120, bottom: -20 };
    await act(async () => room.dispatchEvent(new Event("scroll")));
    expect(notices().map((one) => one.dataset.edge)).toEqual(["top"]);
    await act(async () => notices()[0]!.click());
    expect(scrollBy).toHaveBeenCalledWith({ top: -128, behavior: "smooth" });
  });

  it("names nothing for a round that needs no person", async () => {
    roles = [role(12, "Bob")];
    await pane({ ...FAILING, run: 73, state: { ...FAILED.state!, status: "paused", word: "Paused" } });
    expect(marks()).toHaveLength(0);
    expect(notices()).toHaveLength(0);
  });

  it("says a mark is below where the seat is in sight but its mark is cut off at the room's bottom", async () => {
    roles = [role(12, "Bob")];
    seat = { top: 10, bottom: 120 };
    at = { top: 95, bottom: 113 };
    await pane({ ...FAILING, run: 74 });
    expect(notices().map((one) => one.dataset.edge)).toEqual(["bottom"]);

    await act(async () => notices()[0]!.click());
    expect(scrollBy).toHaveBeenCalledWith({ top: 21, behavior: "smooth" });

    at = { top: 74, bottom: 92 };
    const room = container.querySelector<HTMLElement>(".runroles__room")!;
    await act(async () => room.dispatchEvent(new Event("scroll")));
    expect(notices()).toHaveLength(0);
  });
});

describe("attnOf, unseenOf and noticeOf", () => {
  it("marks a round that may wait, one that failed, and one quiet into the second stage", () => {
    expect(attnOf("asks", { stage: 0, for: null })).toBe("asks");
    expect(attnOf("failed", { stage: 0, for: null })).toBe("failed");
    expect(attnOf("running", { stage: 2, for: 1 })).toBe("stalled");
    expect(attnOf("running", { stage: 1, for: 1 })).toBeNull();
    expect(attnOf("paused", { stage: 0, for: null })).toBeNull();
  });

  it("counts a mark as out of sight once any of it is more than 1px past the room's edge", () => {
    const marks = [
      { attn: "asks" as const, top: -2, bottom: 16 },
      { attn: "failed" as const, top: -1, bottom: 17 },
      { attn: "asks" as const, top: 84, bottom: 102 },
      { attn: "stalled" as const, top: 83, bottom: 101 },
    ];
    const { up, down } = unseenOf(0, 100, marks);
    expect(up.map((one) => one.attn)).toEqual(["asks"]);
    expect(down.map((one) => one.attn)).toEqual(["asks"]);
  });

  it("names each kind with how many, in one order, and in the stop colour where one failed or stalled", () => {
    expect(noticeOf([])).toBeNull();
    expect(noticeOf(["asks", "asks"])).toEqual({ what: tf("auto.run.roles.attn.asks", { n: 2 }), stop: false });
    expect(noticeOf(["stalled", "asks", "failed"])).toEqual({
      what: [
        tf("auto.run.roles.attn.asks", { n: 1 }),
        tf("auto.run.roles.attn.failed", { n: 1 }),
        tf("auto.run.roles.attn.stalled", { n: 1 }),
      ].join(" ・ "),
      stop: true,
    });
  });
});

describe("the terminal of a round", () => {
  const RUNNING: Say = {
    ...FAILED,
    run: 61,
    step: "write the tests",
    task: { ref: "AMB-T-7", title: "Fix the search filter", seq: 1 },
    round: { role: 12, startedAt: null },
    state: { ...FAILED.state!, status: "running", word: "Running" },
  };
  const bar = () => container.querySelector<HTMLElement>(".runterm");
  const back = () => container.querySelector<HTMLButtonElement>(".runterm__back")!;
  const notice = () => container.querySelector(".runterm__notice");

  it("turns to the terminal when a seat is pressed, and back to the roles from the bar", async () => {
    roles = [role(12, "Bob")];
    await pane(RUNNING);
    expect(pressed()).toBe("roles");
    const open = seats()[0]!;
    expect(open.getAttribute("aria-label")).toBe(tf("auto.run.seatOpen", { role: "Bob", n: 1, task: "AMB-T-7 Fix the search filter" }));
    await act(async () => open.click());
    expect(pressed()).toBe("terminal");
    expect(shown()).toBeNull();
    expect(bar()?.parentElement?.classList.contains("slot__body"), "the bar is not in the pane's body").toBe(true);
    expect(container.querySelector(".runterm__task")?.textContent).toBe("Bob ・ AMB-T-7 Fix the search filter");
    expect(container.querySelector(".runterm__step")?.textContent).toBe("write the tests");
    expect(notice(), "a turn the reader made was said to be the step's").toBeNull();

    expect(back().textContent).toBe(t("auto.run.backToRoles"));
    await act(async () => back().click());
    expect(pressed()).toBe("roles");
    expect(shown()).not.toBeNull();
    expect(bar(), "the bar left the layout under the roles").not.toBeNull();
  });

  it("says why in one line where a step that may wait for a person turned it", async () => {
    roles = [role(12, "Bob")];
    await pane({ ...RUNNING, run: 62 });
    await pane({ ...RUNNING, run: 62, interactive: true });
    expect(pressed()).toBe("terminal");
    expect(notice()?.textContent).toBe(tf("auto.run.turnedForStep", { role: "Bob" }));
    // A press is the reader's, and the line goes with it.
    await act(async () => back().click());
    await act(async () => toTerminal());
    expect(notice()).toBeNull();
  });

  it("says nothing where the reader had the terminal up already", async () => {
    roles = [role(12, "Bob")];
    await pane({ ...RUNNING, run: 63 });
    await act(async () => toTerminal());
    await pane({ ...RUNNING, run: 63, interactive: true });
    expect(notice()).toBeNull();
  });

  // The way in from the task a failed round was filed as (`../screens/TaskDetailPane`): the pane comes
  // up on the round's terminal, says the step it fell on, and says nothing about a step turning it.
  it("says the step a failed round fell on, where it was opened from the task it was filed as", async () => {
    roles = [role(12, "Bob")];
    showTerminalOf(67);
    await pane({ ...RUNNING, run: 67, step: "upload the release", state: FAILED.state });
    expect(pressed()).toBe("terminal");
    expect(container.querySelector(".runterm__step")?.textContent)
      .toBe(tf("auto.run.stepFailed", { step: "upload the release" }));
    expect(notice(), "the reader opened it, no step turned it").toBeNull();
  });

  it("lists the launch's seats by role, and turns this same pane to the terminal of the one pressed", async () => {
    roles = [role(12, "Bob"), role(13, "Carol", 3, "robot")];
    launched = launchOf([
      launchRole(12, "Bob", [
        liveRun(64, { role: 12, task: { id: 1, ref: "AMB-T-7", title: "Fix the search filter", seq: 1 }, stepName: "write the tests" }),
        liveRun(66, { role: 12, status: "failed", task: { id: 2, ref: "AMB-T-9", title: "Tidy", seq: 1 } }),
      ], 3),
      launchRole(13, "Carol", []),
    ]);
    const went: number[] = [];
    await pane({ ...RUNNING, run: 64 }, { launch: 5, onGoToRun: (run) => went.push(run) });
    expect(pressed()).toBe("roles");
    expect(desks().map((one) => one.dataset.role), "a role with no task has a desk").toEqual(["12"]);
    expect(idle().map((one) => one.dataset.role)).toEqual(["13"]);
    expect(seats().map((one) => [one.dataset.run, one.dataset.state, one.getAttribute("aria-current")])).toEqual([
      ["64", "running", "true"],
      ["66", "failed", null],
    ]);
    expect(seats().map((one) => one.getAttribute("aria-label"))).toEqual([
      tf("auto.run.seatOpen", { role: "Bob", n: 1, task: "AMB-T-7 Fix the search filter" }),
      tf("auto.run.seatOpen", { role: "Bob", n: 2, task: "AMB-T-9 Tidy" }),
    ]);
    // The seat nobody is in is said, and is not a press.
    const free = container.querySelector<HTMLElement>(".runseat--free");
    expect(free?.tagName).toBe("DIV");
    expect(free?.textContent).toContain(tf("auto.run.seatsFree", { n: 1 }));

    // The seat in front turns this pane to its own terminal.
    await act(async () => seats()[0]!.click());
    expect(pressed()).toBe("terminal");
    expect(went).toEqual([]);

    // Another seat is reached from the roles alone: the bar names no other seat.
    expect(container.querySelector(".runterm__strip"), "the bar lists the other seats").toBeNull();
    await act(async () => back().click());
    await act(async () => seats()[1]!.click());
    expect(went).toEqual([66]);
    // Put in front in the same place, it comes up on its terminal.
    await pane({ ...RUNNING, run: 66, state: FAILED.state }, { launch: 5, onGoToRun: (run) => went.push(run) });
    expect(pressed()).toBe("terminal");
  });

  it("says the seat and how long its round has run on the bar over its terminal", async () => {
    roles = [role(12, "Bob")];
    launched = launchOf([launchRole(12, "Bob", [liveRun(63, { role: 12 }), liveRun(64, { role: 12 })])]);
    await pane({ ...RUNNING, run: 64, round: { role: 12, startedAt: Date.now() - 65_000 } }, { launch: 5 });
    await act(async () => seats().find((one) => one.dataset.run === "64")!.click());
    expect(pressed()).toBe("terminal");
    expect(container.querySelector(".runterm__task")?.textContent)
      .toBe(`Bob ・ ${tf("auto.run.seat", { n: 2 })} ・ AMB-T-7 Fix the search filter`);
    const time = container.querySelector(".runterm__time")?.textContent;
    expect([tf("face.elapsed", { time: "1:05" }), tf("face.elapsed", { time: "1:06" })]).toContain(time);
    expect(container.querySelector(".runterm__time .runroles__pulse"), "a round under way has no dot").not.toBeNull();

    // A step that may wait for a person is not drawn as one at work.
    await pane({ ...RUNNING, run: 64, interactive: true, round: { role: 12, startedAt: Date.now() - 65_000 } }, { launch: 5 });
    expect(container.querySelector(".runterm__time")).not.toBeNull();
    expect(container.querySelector(".runterm__time .runroles__pulse")).toBeNull();

    // A paused round has stopped at a moment the pane does not know.
    await pane({ ...RUNNING, run: 64, state: { ...FAILED.state!, status: "paused", word: "Paused" } }, { launch: 5 });
    expect(container.querySelector(".runterm__time")).toBeNull();
  });

  it("draws a run kept from before launches as one desk, the run its one seat", async () => {
    roles = [role(12, "Bob")];
    await pane({ ...RUNNING, run: 69 });
    expect(desks().map((one) => one.dataset.role)).toEqual(["12"]);
    expect(seats().map((one) => one.dataset.run)).toEqual(["69"]);
    expect(container.querySelector(".runseat--free")).toBeNull();
  });
});

describe("the seats of a launch", () => {
  const FRONT: Say = { ...FAILED, run: 5, interactive: true, state: { ...FAILED.state!, status: "running", word: "Running" } };

  it("numbers a run by its place among its role's seats", () => {
    const launch = launchOf([launchRole(1, "A", [liveRun(4)]), launchRole(2, "B", [liveRun(7), liveRun(5)])]);
    expect(seatOf(launch, 5)).toEqual({ role: "B", n: 2 });
    expect(seatOf(launch, 9)).toBeNull();
    expect(seatOf(null, 5)).toBeNull();
  });

  it("says how a seat stands, and that it may wait for a person off its own step or the pane's own run", () => {
    expect(seatStateOf(liveRun(5), FRONT)).toBe("asks");
    expect(seatStateOf(liveRun(6), FRONT)).toBe("running");
    expect(seatStateOf(liveRun(6, { interactive: true }), FRONT)).toBe("asks");
    expect(seatStateOf(liveRun(6, { waitsForRoom: true }), FRONT)).toBe("waiting");
    expect(seatStateOf(liveRun(6, { status: "paused" }), FRONT)).toBe("paused");
    expect(seatStateOf(liveRun(6, { status: "failed" }), FRONT)).toBe("failed");
  });
});
