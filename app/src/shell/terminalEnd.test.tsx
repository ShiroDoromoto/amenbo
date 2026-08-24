// @vitest-environment jsdom
// The only way out of a terminal, and the two states it must not be offered in.
//
// A pane going away never ends a session — that is a pane moving, and the session outlives it
// (`AMB-D-753`) — so this control is the whole of how a person stops something. What is pinned here is
// that it names the session that is actually running and appears exactly while one is: a way out drawn
// over an empty slot ends nothing, and one still drawn after the program exited would name a session
// the host has already forgotten.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PaneEvents } from "../talk/terminal";
import { TerminalPane } from "./TerminalPane";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const hoisted = vi.hoisted(() => ({
  /** What the frame was handed, so the test can play the host: open a session, then end it. */
  events: null as PaneEvents | null,
  /** The sessions the way out asked the host to end. */
  ended: [] as string[],
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
// The label is a live thing of its own; what it draws is not what this is about.
vi.mock("../talk/plate", () => ({
  mountPlate: () => ({
    opened: () => {}, said: () => {}, closed: () => {}, named: () => {},
    focused: () => {}, stop: () => {},
  }),
}));

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  hoisted.events = null;
  hoisted.ended = [];
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

/** A pane that starts a terminal as soon as it is drawn. */
async function pane(): Promise<void> {
  await act(async () => {
    root.render(createElement(TerminalPane, {
      frame: "1",
      names: new Map(),
      start: { cwd: "/work/here" },
      autoStart: true,
      focused: true,
      onOpened: () => {},
      onChose: () => {},
      onSaid: () => {},
      onPath: () => {},
      onClosed: () => {},
      onName: () => {},
      onFocus: () => {},
      onWaiting: () => {},
    }));
  });
}

const wayOut = () => container.querySelector<HTMLButtonElement>(".slot__end");

describe("ending the terminal in a pane", () => {
  it("is not offered before one is running", async () => {
    await pane();
    expect(wayOut(), "a way out was drawn over a pane with nothing in it").toBeNull();
  });

  it("names the session that is running, and ends nothing else", async () => {
    await pane();
    await act(async () => {
      hoisted.events?.opened("session-7", "2026-01-01T00:00:00Z", "/work/here");
    });

    expect(wayOut(), "no way out while a terminal was running").not.toBeNull();
    await act(async () => { wayOut()?.click(); });
    expect(hoisted.ended).toEqual(["session-7"]);
  });

  it("goes away once the program has ended, which is when there is nothing left to end", async () => {
    await pane();
    await act(async () => {
      hoisted.events?.opened("session-7", "2026-01-01T00:00:00Z", "/work/here");
    });
    await act(async () => { hoisted.events?.closed("session-7"); });

    expect(wayOut(), "the way out outlived the session it named").toBeNull();
  });
});
