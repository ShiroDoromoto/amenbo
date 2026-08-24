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
}));
vi.mock("../core/dialog", () => ({
  confirmDialog: vi.fn(async () => {
    hoisted.asked++;
    return hoisted.agrees;
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
  hoisted.events = null;
  hoisted.ended = [];
  hoisted.agrees = true;
  hoisted.asked = 0;
  dropped = [];
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
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
