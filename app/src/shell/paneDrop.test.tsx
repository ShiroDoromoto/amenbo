// @vitest-environment jsdom
// The one control a pane has, and the two things it must not do quietly.
//
// It removes the **place**, not the program in it (`../talk/layout`): a pane going away never ends a
// session on its own — that is a pane moving, and the session outlives it (`AMB-D-753`) — so this is
// the only thing on the face that takes a frame off it for good. What is pinned here is that it asks
// before it happens, that a refusal leaves everything exactly as it was, and that going through with
// it ends whatever was running: a session whose place has gone is one nobody can reach.
//
// It is drawn whether or not a terminal is running, because a frame kept from the last run has no
// session and is still a place somebody has to be able to get rid of.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PaneEvents } from "../talk/terminal";
import { TerminalPane } from "./TerminalPane";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const hoisted = vi.hoisted(() => ({
  /** What the frame was handed, so the test can play the host: open a session, then end it. */
  events: null as PaneEvents | null,
  /** The sessions the control asked the host to end. */
  ended: [] as string[],
  /** Whether the person said yes when asked. */
  agrees: true,
  /** How many times they were asked. */
  asked: 0,
  /** What the volatile area answers this pane's session is holding (`session_work`). */
  holding: [] as number[],
  /** Whether that read fails, which is what a window outside Tauri does. */
  workFails: false,
  /** The tasks the box handed back, in the order it moved them. */
  handedBack: [] as Array<[number, string]>,
  /** Whether handing one back is refused. */
  handBackFails: false,
}));

vi.mock("../talk/agent", () => ({
  mountAgentFrame: (_host: HTMLElement, _lang: string, on: PaneEvents) => {
    hoisted.events = on;
    return Promise.resolve(() => {});
  },
}));
vi.mock("../talk/terminal", () => ({
  endTerminal: vi.fn(async (session: string) => {
    hoisted.ended.push(session);
  }),
  pasteIntoTerminal: vi.fn(async () => {}),
}));
vi.mock("../core/dialog", () => ({
  confirmDialog: vi.fn(async () => {
    hoisted.asked++;
    return hoisted.agrees;
  }),
}));
vi.mock("../core/ipc", () => ({
  invoke: vi.fn(async (cmd: string) => {
    if (cmd !== "session_work") throw new Error(`unexpected command ${cmd}`);
    if (hoisted.workFails) throw new Error("no window to ask");
    return { holding: hoisted.holding, finished: 0 };
  }),
}));
vi.mock("../core/mutations", () => ({
  setStatus: vi.fn(async (id: number, status: string) => {
    if (hoisted.handBackFails) throw new Error("already reserved");
    hoisted.handedBack.push([id, status]);
  }),
}));
// The label is a live thing of its own; what it draws is not what this is about.
vi.mock("../talk/plate", () => ({
  mountPlate: () => ({
    opened: () => {}, said: () => {}, closed: () => {}, named: () => {},
    focused: () => {}, stop: () => {},
  }),
}));

let container: HTMLDivElement;
let root: Root;
/** The frames the control asked the face to take away. */
let dropped: string[];

beforeEach(() => {
  // The face measures the window to work out whether the columns beside the panes are columns at all
  // (`../talk/columns`). jsdom's window is 1024, which is genuinely too narrow for two panes and two
  // columns — so a test about what is drawn beside the panes says it is on a wide screen.
  window.innerWidth = 1600;
  hoisted.events = null;
  hoisted.ended = [];
  hoisted.agrees = true;
  hoisted.asked = 0;
  hoisted.holding = [];
  hoisted.workFails = false;
  hoisted.handedBack = [];
  hoisted.handBackFails = false;
  dropped = [];
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
  // The question is drawn through a portal into the body, so it does not go with the container.
  document.querySelectorAll(".modal__overlay").forEach((el) => el.remove());
});

/** A pane, either one that starts a terminal as soon as it is drawn or a place kept from the last run
 *  with nothing in it. */
async function pane(autoStart = true): Promise<void> {
  await act(async () => {
    root.render(createElement(TerminalPane, {
      frame: "1",
      project: 1,
      names: new Map(),
      start: { cwd: "/work/here" },
      autoStart,
      focused: true,
      onOpened: () => {},
      onSaid: () => {},
      onPath: () => {},
      onClosed: () => {},
      onDrop: (frame: string) => { dropped.push(frame); },
      onName: () => {},
      onFocus: () => {},
      onWaiting: () => {},
    }));
  });
}

const wayOut = () => container.querySelector<HTMLButtonElement>(".slot__end");
/** The question about what is being left behind, while it is on the screen (`./PaneDropAsk`). */
const asked = () => document.querySelector(".panedrop__modal");
/** What that question names, one ref to a line. */
const named = () =>
  Array.from(document.querySelectorAll(".panedrop__refs li"), (li) => li.textContent);
/** Press one of the question's three answers, and let what it sets off settle. */
const answer = async (which: number) => {
  const buttons = document.querySelectorAll<HTMLButtonElement>(".panedrop__action");
  await act(async () => { buttons[which]?.click(); });
  await act(async () => { await Promise.resolve(); });
};
/** Press it, and let the asking and the ending it awaits settle. */
const press = async () => {
  await act(async () => { wayOut()?.click(); });
  await act(async () => { await Promise.resolve(); });
};

describe("removing a pane", () => {
  it("is offered on a place with nothing running in it", async () => {
    await pane(false);
    expect(wayOut(), "a place kept from the last run had no way to be got rid of").not.toBeNull();
  });

  it("asks first, and a refusal leaves the place and the terminal alone", async () => {
    await pane();
    await act(async () => {
      hoisted.events?.opened("session-7", "2026-01-01T00:00:00Z", "/work/here");
    });
    hoisted.agrees = false;
    await press();

    expect(hoisted.asked, "the place was taken away without asking").toBe(1);
    expect(dropped, "a refusal took the place away anyway").toEqual([]);
    expect(hoisted.ended, "a refusal ended the terminal anyway").toEqual([]);
  });

  it("ends the terminal that was running, then takes the place away", async () => {
    await pane();
    await act(async () => {
      hoisted.events?.opened("session-7", "2026-01-01T00:00:00Z", "/work/here");
    });
    await press();

    expect(hoisted.ended, "the place went and left its session running").toEqual(["session-7"]);
    expect(dropped).toEqual(["1"]);
  });

  it("names what the session is holding, rather than asking the plain question", async () => {
    hoisted.holding = [3708, 3711];
    await pane();
    await act(async () => {
      hoisted.events?.opened("session-7", "2026-01-01T00:00:00Z", "/work/here");
    });
    await press();

    expect(hoisted.asked, "the plain confirmation was put over work about to be lost").toBe(0);
    expect(named(), "what stood to be lost was not named").toEqual(["AMB-T-3708", "AMB-T-3711"]);
    expect(dropped, "the place went while the question was still standing").toEqual([]);
    expect(hoisted.ended).toEqual([]);
  });

  it("moves nothing until one of the three is pressed, and cancelling moves nothing at all", async () => {
    hoisted.holding = [3708];
    await pane();
    await act(async () => {
      hoisted.events?.opened("session-7", "2026-01-01T00:00:00Z", "/work/here");
    });
    await press();
    expect(hoisted.handedBack, "the screen tidied the ledger up on its own").toEqual([]);

    await answer(2);

    expect(asked(), "the question stayed up after being called off").toBeNull();
    expect(hoisted.handedBack).toEqual([]);
    expect(dropped, "calling it off took the place away anyway").toEqual([]);
    expect(hoisted.ended).toEqual([]);
  });

  it("hands every one of them back before the place goes, when that is what was asked", async () => {
    hoisted.holding = [3708, 3711];
    await pane();
    await act(async () => {
      hoisted.events?.opened("session-7", "2026-01-01T00:00:00Z", "/work/here");
    });
    await press();
    await answer(0);

    expect(hoisted.handedBack).toEqual([[3708, "todo"], [3711, "todo"]]);
    expect(hoisted.ended).toEqual(["session-7"]);
    expect(dropped).toEqual(["1"]);
  });

  it("leaves the reservations standing where that is what was asked", async () => {
    hoisted.holding = [3708];
    await pane();
    await act(async () => {
      hoisted.events?.opened("session-7", "2026-01-01T00:00:00Z", "/work/here");
    });
    await press();
    await answer(1);

    expect(hoisted.handedBack, "a reservation was moved by a press that never asked for it").toEqual([]);
    expect(hoisted.ended).toEqual(["session-7"]);
    expect(dropped).toEqual(["1"]);
  });

  /** Doing half of it would take the place away and lose the very thing the box had just named. */
  it("keeps the place where the hand-back was refused, and says so", async () => {
    hoisted.holding = [3708];
    hoisted.handBackFails = true;
    await pane();
    await act(async () => {
      hoisted.events?.opened("session-7", "2026-01-01T00:00:00Z", "/work/here");
    });
    await press();
    await answer(0);

    expect(document.querySelector(".panedrop__failed")?.textContent).toBe("already reserved");
    expect(dropped, "a refused hand-back took the place away anyway").toEqual([]);
    expect(hoisted.ended).toEqual([]);
  });

  /** A read that cannot answer says nothing is held, and a question raised on a guess would be a
   *  question about nothing. */
  it("falls back to the plain question where the volatile area cannot be read", async () => {
    hoisted.workFails = true;
    await pane();
    await act(async () => {
      hoisted.events?.opened("session-7", "2026-01-01T00:00:00Z", "/work/here");
    });
    await press();

    expect(asked(), "a box was raised over a guess").toBeNull();
    expect(hoisted.asked).toBe(1);
    expect(dropped).toEqual(["1"]);
  });

  it("ends nothing where the program had already exited", async () => {
    await pane();
    await act(async () => {
      hoisted.events?.opened("session-7", "2026-01-01T00:00:00Z", "/work/here");
    });
    await act(async () => { hoisted.events?.closed("session-7"); });
    await press();

    expect(hoisted.ended, "a session the host has forgotten was named anyway").toEqual([]);
    expect(dropped).toEqual(["1"]);
  });
});
