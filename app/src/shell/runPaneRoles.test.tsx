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
// roles are defined in, whatever the round is doing. **A card draws a line only to a role it touches**,
// and where that role has no card it says "→ role" instead. **Where the cards do not all fit in the
// room, each round is a row of a list**, unless the reader holds the room to one or the other.
//
// **A round that needs a person, out of sight in the room, is named at the room's edge**, and the
// notice scrolls the room to it when pressed — never by itself. It is read again as the room scrolls.
//
// **A card or a row is pressed to read its round's terminal**, and the terminal has a bar over it: the way back to
// the roles, the other rounds of the same automation, and — where a step turned the pane — why.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Say } from "../talk/nameplate";
import type { AutomationRunCardDto, TaskRoleDto } from "../bindings/bindings";
import type { BuiltinRun } from "../talk/automationStep";
import { TerminalPane } from "./TerminalPane";
import { t, tf } from "../core/i18n";
import {
  attnOf, cardsFit, filedLine, fitOf, handsAway, handsOf, noticeOf, roleCards, roundsOf, silenceOf, unseenOf, type RoleCard,
} from "./RunPicture";

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

/** The runs under way, as the bar over a run's terminal reads them (`../core/automations`). */
let live: AutomationRunCardDto[] = [];
vi.mock("../core/automations", async (actual) => ({
  ...(await actual<typeof import("../core/automations")>()),
  useLiveRuns: () => live,
}));

/** A run under way of the automation `automation`, as the "running" tab's row carries it. */
const liveRun = (run: number, more: Partial<AutomationRunCardDto> = {}): AutomationRunCardDto => ({
  run, project: 1, projectName: "P", automation: 3, automationName: "Nightly", status: "running",
  pauseRequested: false, pauseBeforeNextTask: false, pausableBeforeNextTask: false, waiting: false, waitsForRoom: false,
  stepsDone: 1, passes: [], reportWithheld: [], acknowledged: false, ...more,
} as AutomationRunCardDto);

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
  live = [];
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
  { autoStart = false, builtin = null, onGoToRun }: { autoStart?: boolean; builtin?: BuiltinRun | null; onGoToRun?: (run: number) => void } = {},
): Promise<void> {
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
      onGoToRun,
    }));
  });
}

const faces = () => [...container.querySelectorAll<HTMLButtonElement>(".slot__face")];
const pressed = () => faces().find((one) => one.getAttribute("aria-pressed") === "true")?.textContent;
const shown = () => container.querySelector(".runroles");
const cards = () => [...container.querySelectorAll<HTMLElement>(".runroles__card")];
const idle = () => [...container.querySelectorAll<HTMLElement>(".runroles__idlelist > li")];
const views = () => [...container.querySelectorAll<HTMLButtonElement>(".slot__view")];
const rows = () => [...container.querySelectorAll<HTMLElement>(".runroles__row")];

describe("the face a run's pane is turned to", () => {
  it("opens on the roles, with the terminal face kept behind them", async () => {
    await pane(FAILED);
    expect(faces().map((one) => one.textContent)).toEqual([t("auto.run.faceRoles"), t("auto.run.faceTerminal")]);
    expect(pressed()).toBe(t("auto.run.faceRoles"));
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
    expect(card.querySelector(".runroles__pulse")?.getAttribute("aria-label")).toBe(t("auto.run.roles.working"));
    // Another automation's role is not this run's.
    expect(idle().map((one) => one.dataset.role)).toEqual(["11", "13"]);
    expect(idle()[0]!.querySelector("[aria-label]")?.getAttribute("aria-label")).toBe(tf("auto.run.roles.idleOne", { name: "Alice" }));
    expect(container.querySelector(".runroles__idlecount")?.textContent).toBe(tf("auto.run.roles.idle", { n: 2 }));
  });

  it("says a round that may wait for a person, and one that failed, on its card, and moves the dot of neither", async () => {
    roles = [role(12, "Bob")];
    await pane({ ...RUNNING, interactive: true, run: 42 });
    // The pane is turned to the terminal while such a step runs; the roles are turned to by hand.
    await act(async () => faces()[0]!.click());
    expect(cards()[0]!.dataset.state).toBe("asks");
    expect(cards()[0]!.querySelector(".runroles__chip")?.textContent).toBe(t("auto.run.roles.asks"));
    expect(cards()[0]!.querySelector(".runroles__pulse"), "a round that may wait has the dot").toBeNull();

    await pane({ ...RUNNING, run: 43, state: FAILED.state });
    expect(cards()[0]!.dataset.state).toBe("failed");
    expect(cards()[0]!.querySelector(".runroles__chip")?.textContent).toBe(t("auto.run.roles.failed"));
    expect(cards()[0]!.querySelector(".runroles__time"), "a failed round went on counting").toBeNull();
    expect(cards()[0]!.querySelector(".runroles__pulse"), "a failed round has the dot").toBeNull();
    expect(container.querySelector(".runroles__filed"), "a round that filed nothing has a line").toBeNull();

    await pane({ ...RUNNING, run: 44, state: { ...FAILED.state!, status: "paused", word: "Paused" } });
    expect(cards()[0]!.dataset.state).toBe("paused");
    expect(cards()[0]!.querySelector(".runroles__pulse"), "a paused round has the dot").toBeNull();
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
    // Dave has no card, so the failed card names him instead of drawing a line to him.
    const hands = cards()[0]!.querySelectorAll<HTMLElement>(".runroles__hand");
    expect([...hands].map((one) => [one.dataset.to, one.textContent])).toEqual([["14", "→ Dave"]]);
    expect(container.querySelector(".runroles__strokes")).toBeNull();
  });

  it("names nobody on the card where the failure went to a person", async () => {
    roles = [role(12, "Bob"), role(14, "Dave")];
    await pane({
      ...RUNNING,
      run: 46,
      round: { role: 12, startedAt: null, incident: { ref: "AMB-T-99", role: null, person: true } },
      state: FAILED.state,
    });
    expect(cards()[0]!.querySelector(".runroles__hand")).toBeNull();
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

  it("draws a round under no role as one card named after the automation, its roles on the bar", () => {
    const got = roleCards({ ...RUNNING, round: { role: null, startedAt: null } }, [role(2, "Nightlyの係")]);
    expect(got.cards).toEqual([{ role: null, name: "Nightly", icon: "robot", state: "running" }]);
    expect(got.idle.map((one) => one.id)).toEqual([2]);
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

describe("silenceOf", () => {
  const at = 100_000_000;
  const min = 60_000;
  const card = (state: RoleCard["state"]): RoleCard => ({ role: 2, name: "B", icon: "person", state });

  it("measures a round under way, the one that may wait for a person included", () => {
    expect(silenceOf(card("running"), at - 6 * min, at, 10)).toEqual({ stage: 1, for: 6 * min });
    expect(silenceOf(card("asks"), at - 21 * min, at, 10)).toEqual({ stage: 2, for: 21 * min });
  });

  it("says no time while the silence is short of the first stage", () => {
    expect(silenceOf(card("running"), at - min, at, 10)).toEqual({ stage: 0, for: null });
  });

  it("does not measure a round that is paused or failed", () => {
    expect(silenceOf(card("paused"), at - 60 * min, at, 10)).toEqual({ stage: 0, for: null });
    expect(silenceOf(card("failed"), at - 60 * min, at, 10)).toEqual({ stage: 0, for: null });
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

describe("handsOf and handsAway", () => {
  const roles = [role(2, "B"), role(4, "D")];
  const failed = (incident: { ref: string; role: number | null; person: boolean } | undefined): Say => ({
    ...FAILED,
    round: { role: 2, startedAt: null, ...(incident === undefined ? {} : { incident }) },
  });
  const hands = (run: Say) => handsOf(roleCards(run, roles).cards, run, roles);

  it("draws a line from a failed card to the role it was filed to", () => {
    expect(hands(failed({ ref: "AMB-T-9", role: 4, person: false }))).toEqual([{ from: 2, to: 4, name: "D", filed: true }]);
  });

  it("draws none where the failure went to a person, to a role since gone, or nowhere", () => {
    expect(hands(failed({ ref: "AMB-T-9", role: null, person: true }))).toEqual([]);
    expect(hands(failed({ ref: "AMB-T-9", role: 77, person: false }))).toEqual([]);
    expect(hands(failed(undefined))).toEqual([]);
  });

  it("names the other end only where it has no card, and every end on a row of the list", () => {
    const one = { from: 2, to: 4, name: "D", filed: true } as const;
    const card = (id: number) => ({ role: id, name: "", icon: "person", state: "running" }) as const;
    expect(handsAway(2, [one], [card(2)])).toEqual([one]);
    expect(handsAway(2, [one], [card(2), card(4)])).toEqual([]);
    expect(handsAway(2, [one], [])).toEqual([one]);
    expect(handsAway(4, [one], [])).toEqual([]);
  });
});

describe("cards or the list", () => {
  const RUNNING: Say = {
    ...FAILED,
    run: 51,
    step: "write the tests",
    task: { ref: "AMB-T-7", title: "Fix the search filter", seq: 1 },
    round: { role: 12, startedAt: Date.now() - 65_000 },
    state: { ...FAILED.state!, status: "running", word: "Running" },
  };

  afterEach(() => vi.restoreAllMocks());

  /** Rooms measured at `width` by `height`, as a laid-out page would measure them. */
  const room = (width: number, height: number) => {
    vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(width);
    vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(height);
  };

  it("is the room's to decide until the reader holds it, and is offered only while the roles are up", async () => {
    roles = [role(12, "Bob")];
    await pane(RUNNING);
    expect(views().map((one) => one.textContent)).toEqual([t("auto.run.viewAuto"), t("auto.run.viewCards"), t("auto.run.viewList")]);
    expect(views().find((one) => one.getAttribute("aria-pressed") === "true")?.textContent).toBe(t("auto.run.viewAuto"));
    expect(cards()).toHaveLength(1);

    await act(async () => faces()[1]!.click());
    expect(views(), "the terminal face offers a layout of the roles").toHaveLength(0);
  });

  it("draws a row of the list for a round where the cards do not fit the room", async () => {
    roles = [role(12, "Bob"), role(14, "Dave")];
    room(140, 100);
    await pane({
      ...RUNNING,
      run: 52,
      round: { role: 12, startedAt: null, incident: { ref: "AMB-T-99", role: 14, person: false } },
      state: FAILED.state,
    });
    expect(cards()).toHaveLength(0);
    expect(rows().map((one) => [one.dataset.role, one.dataset.state])).toEqual([["12", "failed"]]);
    const row = rows()[0]!;
    expect(row.querySelector(".runroles__name")?.textContent).toBe("Bob");
    expect(row.querySelector(".runroles__task")?.textContent).toBe("AMB-T-7 Fix the search filter");
    expect(row.querySelector(".runroles__chip")?.textContent).toBe(t("auto.run.roles.failed"));
    expect(row.querySelector(".runroles__hand")?.textContent).toBe("→ Dave");
    expect(row.querySelector(".runroles__pulse"), "a row has the dot").toBeNull();
  });

  it("holds to what the reader pressed, and keeps it for that run's pane", async () => {
    roles = [role(12, "Bob")];
    await pane({ ...RUNNING, run: 53 });
    await act(async () => views()[2]!.click());
    expect(rows()).toHaveLength(1);
    expect(cards()).toHaveLength(0);

    act(() => root.unmount());
    root = createRoot(container);
    await pane({ ...RUNNING, run: 53 });
    expect(rows(), "a pane back up for the same run forgot the layout it was held to").toHaveLength(1);

    await pane({ ...RUNNING, run: 54 });
    expect(views().find((one) => one.getAttribute("aria-pressed") === "true")?.textContent).toBe(t("auto.run.viewAuto"));

    // Held to cards, a room too small for them still draws them.
    room(140, 100);
    await act(async () => views()[1]!.click());
    expect(cards()).toHaveLength(1);
    expect(rows()).toHaveLength(0);
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

  /** Where the mark — the card's chip — stands on the page, and where the card does; the room shows
   *  from 0 to 100. */
  let at = { top: 150, bottom: 250 };
  let card = { top: 0, bottom: 0 };
  const scrollBy = vi.fn();
  const box = (top: number, bottom: number) =>
    ({ top, bottom, left: 0, right: 100, width: 100, height: bottom - top, x: 0, y: top, toJSON: () => ({}) }) as DOMRect;

  beforeEach(() => {
    at = { top: 150, bottom: 250 };
    card = { top: 0, bottom: 0 };
    scrollBy.mockReset();
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
      if (this.classList.contains("runroles__room")) return box(0, 100);
      if (this.dataset.attn !== undefined) return box(at.top, at.bottom);
      if (this.classList.contains("runroles__card")) return box(card.top, card.bottom);
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
    expect(cards()[0]!.dataset.attn, "the card itself was the mark").toBeUndefined();
    expect(marks().map((one) => [one.className, one.dataset.attn])).toEqual([["runroles__chip", "failed"]]);
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

  it("says a mark is below where the card is in sight but its mark is cut off at the room's bottom", async () => {
    roles = [role(12, "Bob")];
    card = { top: 10, bottom: 120 };
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
  const card = (state: RoleCard["state"]): RoleCard => ({ role: 2, name: "B", icon: "person", state });

  it("marks a round that may wait, one that failed, and one quiet into the second stage", () => {
    expect(attnOf(card("asks"), { stage: 0, for: null })).toBe("asks");
    expect(attnOf(card("failed"), { stage: 0, for: null })).toBe("failed");
    expect(attnOf(card("running"), { stage: 2, for: 1 })).toBe("stalled");
    expect(attnOf(card("running"), { stage: 1, for: 1 })).toBeNull();
    expect(attnOf(card("paused"), { stage: 0, for: null })).toBeNull();
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

describe("cardsFit and fitOf", () => {
  it("counts the cards a room holds, its padding and the gaps between them taken out", () => {
    expect(cardsFit(166, 134)).toBe(1);
    expect(cardsFit(165, 134)).toBe(0);
    expect(cardsFit(166, 133)).toBe(0);
    expect(cardsFit(324, 260)).toBe(4);
    expect(cardsFit(0, 0)).toBe(0);
  });

  it("turns to the list as soon as the cards do not fit, and back only once the room has grown 40px past it", () => {
    const first = fitOf(null, 166, 134, 1);
    expect(first.view).toBe("cards");
    const down = fitOf(first, 165, 134, 1);
    expect(down).toEqual({ view: "list", width: 165, height: 134 });
    expect(fitOf(down, 166, 134, 1).view, "the list turned back the moment the card fit").toBe("list");
    expect(fitOf(down, 204, 134, 1).view).toBe("list");
    expect(fitOf(down, 205, 134, 1).view).toBe("cards");
    expect(fitOf(down, 166, 174, 1).view).toBe("cards");
    // Grown, but still too small for the cards.
    expect(fitOf(down, 165, 300, 1).view).toBe("list");
  });

  it("starts on the list where the cards do not fit, and leaves it as soon as they do", () => {
    const first = fitOf(null, 160, 134, 1);
    expect(first.view).toBe("list");
    expect(fitOf(first, 166, 134, 1).view).toBe("cards");
  });

  it("turns to the list where more rounds come than the room holds", () => {
    const cards = fitOf(null, 324, 134, 2);
    expect(cards.view).toBe("cards");
    expect(fitOf(cards, 324, 134, 3).view).toBe("list");
    expect(fitOf(null, 10, 10, 0).view).toBe("cards");
  });
});

/// A card is pressed to read its round's terminal, and the terminal has a bar over it.
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
  const rounds = () => [...container.querySelectorAll<HTMLButtonElement>(".runterm__round")];

  it("turns to the terminal when a card is pressed, and back to the roles from the bar", async () => {
    roles = [role(12, "Bob")];
    await pane(RUNNING);
    expect(pressed()).toBe(t("auto.run.faceRoles"));
    const open = cards()[0]!.querySelector<HTMLButtonElement>(".runroles__open")!;
    expect(open.getAttribute("aria-label")).toBe(tf("auto.run.roundTerminal", { name: "Bob" }));
    await act(async () => open.click());
    expect(pressed()).toBe(t("auto.run.faceTerminal"));
    expect(shown()).toBeNull();
    expect(bar()?.parentElement?.classList.contains("slot__body"), "the bar is not in the pane's body").toBe(true);
    expect(container.querySelector(".runterm__task")?.textContent).toBe("Bob ・ AMB-T-7 Fix the search filter");
    expect(container.querySelector(".runterm__step")?.textContent).toBe("write the tests");
    expect(notice(), "a turn the reader made was said to be the step's").toBeNull();

    expect(back().textContent).toBe(t("auto.run.backToRoles"));
    await act(async () => back().click());
    expect(pressed()).toBe(t("auto.run.faceRoles"));
    expect(shown()).not.toBeNull();
    expect(bar(), "the bar left the layout under the roles").not.toBeNull();
  });

  it("turns to the terminal when a row of the list is pressed, and back to the list from the bar", async () => {
    roles = [role(12, "Bob")];
    // A room too small for a card, so the round is a row.
    vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(140);
    vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(100);
    try {
      await pane({ ...RUNNING, run: 68 });
      expect(cards()).toHaveLength(0);
      const open = rows()[0]!.querySelector<HTMLButtonElement>(".runroles__open")!;
      expect(open.getAttribute("aria-label")).toBe(tf("auto.run.roundTerminal", { name: "Bob" }));
      await act(async () => open.click());
      expect(pressed()).toBe(t("auto.run.faceTerminal"));

      await act(async () => back().click());
      expect(rows(), "the way back did not come back to the list").toHaveLength(1);
    } finally {
      vi.restoreAllMocks();
    }
  });

  it("says why in one line where a step that may wait for a person turned it", async () => {
    roles = [role(12, "Bob")];
    await pane({ ...RUNNING, run: 62 });
    await pane({ ...RUNNING, run: 62, interactive: true });
    expect(pressed()).toBe(t("auto.run.faceTerminal"));
    expect(notice()?.textContent).toBe(tf("auto.run.turnedForStep", { role: "Bob" }));
    // A press is the reader's, and the line goes with it.
    await act(async () => back().click());
    await act(async () => faces()[1]!.click());
    expect(notice()).toBeNull();
  });

  it("says nothing where the reader had the terminal up already", async () => {
    roles = [role(12, "Bob")];
    await pane({ ...RUNNING, run: 63 });
    await act(async () => faces()[1]!.click());
    await pane({ ...RUNNING, run: 63, interactive: true });
    expect(notice()).toBeNull();
  });

  it("puts the other rounds of the automation on the bar, and goes to one when it is pressed", async () => {
    roles = [role(12, "Bob"), role(13, "Carol", 3, "robot")];
    live = [liveRun(64), liveRun(66, { role: 13, status: "failed", task: { id: 1, ref: "AMB-T-9", title: "Tidy", seq: 1 } }), liveRun(65, { automation: 9 })];
    const went: number[] = [];
    await pane({ ...RUNNING, run: 64 }, { onGoToRun: (run) => went.push(run) });
    await act(async () => faces()[1]!.click());
    expect(rounds().map((one) => [one.dataset.run, one.dataset.state, one.getAttribute("aria-current")])).toEqual([
      ["64", "running", "true"],
      ["66", "failed", null],
    ]);
    // Each is named by its task, so two rounds under one role are told apart.
    expect(rounds().map((one) => one.getAttribute("aria-label"))).toEqual([
      tf("auto.run.roundTerminalOf", { name: "Bob", which: "AMB-T-7" }),
      tf("auto.run.roundTerminalOf", { name: "Carol", which: "AMB-T-9" }),
    ]);
    await act(async () => rounds()[1]!.click());
    expect(went).toEqual([66]);
    // Its pane goes up on the terminal.
    act(() => root.unmount());
    root = createRoot(container);
    await pane({ ...RUNNING, run: 66, round: { role: 13, startedAt: null }, state: FAILED.state });
    expect(pressed()).toBe(t("auto.run.faceTerminal"));
  });

  it("draws no rounds where this run is the automation's only one", async () => {
    live = [liveRun(67)];
    await pane({ ...RUNNING, run: 67 });
    expect(rounds()).toHaveLength(0);
  });
});

describe("roundsOf", () => {
  const RUN: Say = { ...FAILED, run: 5, round: { role: 2, startedAt: null }, interactive: true, state: { ...FAILED.state!, status: "running", word: "Running" } };

  it("draws each round by its role, or by the automation where it has none, in the order the runs began", () => {
    const task = { id: 1, ref: "AMB-T-2", title: "Tidy", seq: 1 };
    const got = roundsOf(RUN, [liveRun(7, { waiting: true, task }), liveRun(5), liveRun(3, { status: "paused", automationName: "Old" })], [role(2, "B")]);
    expect(got).toEqual([
      { run: 3, name: "Old", icon: "robot", dot: "paused", which: "#3" },
      { run: 5, name: "B", icon: "person", dot: "asks", which: "#5" },
      { run: 7, name: "Nightly", icon: "robot", dot: "waiting", which: "AMB-T-2" },
    ]);
  });

  it("draws none where no other run of the automation is under way", () => {
    expect(roundsOf(RUN, [liveRun(5), liveRun(8, { automation: 4 })], [])).toEqual([]);
  });
});
