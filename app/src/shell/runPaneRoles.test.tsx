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
// **A role with a round is a card, and the rest are icons on the bar** — the cards in the order the
// roles are defined in, whatever the round is doing.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Say } from "../talk/nameplate";
import type { TaskRoleDto } from "../bindings/bindings";
import type { BuiltinRun } from "../talk/automationStep";
import { TerminalPane } from "./TerminalPane";
import { t, tf } from "../core/i18n";
import { filedLine, roleCards } from "./RunPicture";

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
    focused: () => {}, stop: () => {},
  }),
}));

/** The project's roles, as the pane reads them (`../core/projectRoles`). */
let roles: TaskRoleDto[] = [];
vi.mock("../core/projectRoles", () => ({ useProjectRoles: () => roles }));

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
async function pane(run: Say | null, { autoStart = false, builtin = null }: { autoStart?: boolean; builtin?: BuiltinRun | null } = {}): Promise<void> {
  await act(async () => {
    root.render(createElement(TerminalPane, {
      frame: run === null ? "1" : `run-${run.run}`,
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
    }));
  });
}

const faces = () => [...container.querySelectorAll<HTMLButtonElement>(".slot__face")];
const pressed = () => faces().find((one) => one.getAttribute("aria-pressed") === "true")?.textContent;
const shown = () => container.querySelector(".runroles");
const cards = () => [...container.querySelectorAll<HTMLElement>(".runroles__card")];
const idle = () => [...container.querySelectorAll<HTMLElement>(".runroles__idlelist > li")];

describe("the face a run's pane is turned to", () => {
  it("opens on the roles, with the terminal face kept behind them", async () => {
    await pane(FAILED);
    expect(faces().map((one) => one.textContent)).toEqual([t("auto.run.faceRoles"), t("auto.run.faceTerminal")]);
    expect(pressed()).toBe(t("auto.run.faceRoles"));
    expect(shown()).not.toBeNull();
    expect(shown()?.parentElement?.classList.contains("slot__body"), "the roles are not drawn over the terminal face").toBe(true);
  });

  it("turns to the terminal and back, and keeps the turn for that run's pane", async () => {
    await pane(FAILED);
    await act(async () => faces()[1]!.click());
    expect(pressed()).toBe(t("auto.run.faceTerminal"));
    expect(shown()).toBeNull();

    // The pane comes down with its page and goes up again for the same run.
    act(() => root.unmount());
    root = createRoot(container);
    await pane(FAILED);
    expect(pressed(), "a pane back up for the same run forgot the reader's turn").toBe(t("auto.run.faceTerminal"));

    // Another run opens on its roles, whatever the last one was turned to.
    await pane({ ...FAILED, run: 16 });
    expect(pressed()).toBe(t("auto.run.faceRoles"));

    await pane(FAILED);
    await act(async () => faces()[0]!.click());
    expect(shown()).not.toBeNull();
  });

  it("is not a question on an ordinary pane", async () => {
    await pane(null);
    expect(faces()).toHaveLength(0);
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
    expect(pressed()).toBe(t("auto.run.faceRoles"));
    await pane(ASKING);
    expect(pressed(), "a step asking a person was left behind the roles").toBe(t("auto.run.faceTerminal"));
    await pane({ ...RUNNING, step: "after" });
    expect(pressed()).toBe(t("auto.run.faceRoles"));
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
    expect(pressed()).toBe(t("auto.run.faceRoles"));
    await pane({ ...RUNNING, run: 23, step: "after" });
    expect(pressed()).toBe(t("auto.run.faceRoles"));
  });

  it("does not turn a run that is not going", async () => {
    await pane({ ...ASKING, run: 24, state: { ...FAILED.state!, status: "paused", word: "Paused" } });
    expect(pressed()).toBe(t("auto.run.faceRoles"));
  });
});

/// A role with a round is a card, and the rest are icons on the bar.
describe("the roles of a run", () => {
  const RUNNING: Say = {
    ...FAILED,
    run: 41,
    step: "write the tests",
    task: { ref: "AMB-T-7", title: "Fix the search filter", seq: 1 },
    round: { role: 12, startedAt: Date.now() - 65_000 },
    state: { ...FAILED.state!, status: "running", word: "Running" },
  };

  it("draws the role with a round as a card, and the rest on the bar, in the order they are defined in", async () => {
    roles = [role(11, "Alice"), role(12, "Bob", 3, "robot"), role(13, "Carol"), role(21, "Elsewhere", 9)];
    await pane(RUNNING);
    expect(cards().map((one) => one.dataset.role)).toEqual(["12"]);
    const card = cards()[0]!;
    expect(card.querySelector(".runroles__name")?.textContent).toBe("Bob");
    expect(card.querySelector("[data-icon]")?.getAttribute("data-icon")).toBe("robot");
    expect(card.querySelector(".runroles__task")?.textContent).toBe("AMB-T-7 Fix the search filter");
    expect(card.querySelector(".runroles__step")?.textContent).toBe("write the tests");
    expect(card.querySelector(".runroles__time")?.textContent).toMatch(/1:0\d/);
    // Another automation's role is not this run's.
    expect(idle().map((one) => one.dataset.role)).toEqual(["11", "13"]);
    expect(idle()[0]!.querySelector("[aria-label]")?.getAttribute("aria-label")).toBe(tf("auto.run.roles.idleOne", { name: "Alice" }));
    expect(container.querySelector(".runroles__idlecount")?.textContent).toBe(tf("auto.run.roles.idle", { n: 2 }));
  });

  it("says a round that may wait for a person, and one that failed, on its card", async () => {
    roles = [role(12, "Bob")];
    await pane({ ...RUNNING, interactive: true, run: 42 });
    // The pane is turned to the terminal while such a step runs; the roles are turned to by hand.
    await act(async () => faces()[0]!.click());
    expect(cards()[0]!.dataset.state).toBe("asks");
    expect(cards()[0]!.querySelector(".runroles__chip")?.textContent).toBe(t("auto.run.roles.asks"));

    await pane({ ...RUNNING, run: 43, state: FAILED.state });
    expect(cards()[0]!.dataset.state).toBe("failed");
    expect(cards()[0]!.querySelector(".runroles__chip")?.textContent).toBe(t("auto.run.roles.failed"));
    expect(cards()[0]!.querySelector(".runroles__time"), "a failed round went on counting").toBeNull();
    expect(container.querySelector(".runroles__filed"), "a round that filed nothing has a line").toBeNull();
  });

  it("puts one line under the pane for a failed round that was filed", async () => {
    roles = [role(12, "Bob"), role(14, "Dave")];
    await pane({
      ...RUNNING,
      run: 45,
      round: { role: 12, startedAt: null, incident: { ref: "AMB-T-99", role: 14, person: false } },
      state: FAILED.state,
    });
    expect(container.querySelector(".runroles__filed")?.textContent).toBe(
      tf("auto.run.roles.filedRole", { role: "Bob", to: "Dave", ref: "AMB-T-99" }),
    );
  });

  it("draws no card for a run that is over, or waiting for its next task", async () => {
    roles = [role(11, "Alice"), role(12, "Bob")];
    await pane({ ...RUNNING, run: 44, state: { ...FAILED.state!, status: "completed", word: "Completed" } });
    expect(cards()).toHaveLength(0);
    expect(container.querySelector(".runroles__none")?.textContent).toBe(t("auto.run.roles.none"));
    expect(idle()).toHaveLength(2);
  });
});

describe("roleCards", () => {
  const RUNNING: Say = {
    ...FAILED,
    round: { role: 2, startedAt: null },
    state: { ...FAILED.state!, status: "running", word: "Running" },
  };

  it("keeps the cards and the bar in the order the roles are defined in", () => {
    const got = roleCards(RUNNING, [role(3, "C"), role(2, "B"), role(1, "A")]);
    expect(got.cards.map((one) => one.role)).toEqual([2]);
    expect(got.idle.map((one) => one.id)).toEqual([3, 1]);
  });

  it("draws the round of an automation with no role as one card named after the automation", () => {
    const got = roleCards({ ...RUNNING, round: { role: null, startedAt: null } }, []);
    expect(got.cards).toEqual([{ role: null, name: "Nightly", icon: "robot", state: "running" }]);
    expect(got.idle).toEqual([]);
  });

  it("draws no card while the run waits for its next task", () => {
    const got = roleCards({ ...RUNNING, state: { ...RUNNING.state!, waiting: true } }, [role(2, "B")]);
    expect(got.cards).toEqual([]);
    expect(got.idle.map((one) => one.id)).toEqual([2]);
  });

  it("draws the icon a role was given only where it is one drawn here", () => {
    expect(roleCards(RUNNING, [role(2, "B", 3, "nothing-like-it")]).cards[0]!.icon).toBe("person");
  });
});

describe("filedLine", () => {
  const roles = [role(2, "B"), role(4, "D")];
  const failed = (incident: { ref: string; role: number | null; person: boolean } | undefined): Say => ({
    ...FAILED,
    round: { role: 2, startedAt: null, ...(incident === undefined ? {} : { incident }) },
  });
  const line = (run: Say) => filedLine(roleCards(run, roles).cards, run, roles);

  it("names the role the failure was filed to", () => {
    expect(line(failed({ ref: "AMB-T-9", role: 4, person: false }))).toBe(
      tf("auto.run.roles.filedRole", { role: "B", to: "D", ref: "AMB-T-9" }),
    );
  });

  it("says a person where it went to one, and names nobody where the role is gone", () => {
    expect(line(failed({ ref: "AMB-T-9", role: null, person: true }))).toBe(
      tf("auto.run.roles.filedPerson", { role: "B", ref: "AMB-T-9" }),
    );
    expect(line(failed({ ref: "AMB-T-9", role: 77, person: false }))).toBe(
      tf("auto.run.roles.filed", { role: "B", ref: "AMB-T-9" }),
    );
  });

  it("has no line where the round filed nothing, or did not fail", () => {
    expect(line(failed(undefined))).toBeNull();
    const running: Say = {
      ...failed({ ref: "AMB-T-9", role: 4, person: false }),
      state: { ...FAILED.state!, status: "running", word: "Running" },
    };
    expect(line(running)).toBeNull();
  });
});
