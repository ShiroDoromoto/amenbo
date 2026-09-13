// @vitest-environment jsdom
// What a pane says when the program in it stops.
//
// It says it ended, and for nearly every ending that is all it says: the program's own last words
// are on the screen above, and a pane adding an account of its own would be talking over them.
//
// The exception is an ending Amenbo had a hand in. A pane is given a home of its own so two panes of
// one provider do not share a conversation, and a provider that stops over a file it found nothing
// in names *that* home — a directory thrown away with the pane. A reader who follows the message
// edits a file nobody will read again, so the pane says which of their own files it is
// (`../talk/terminal`).
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PaneEvents } from "../talk/terminal";
import { TerminalPane } from "./TerminalPane";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const hoisted = vi.hoisted(() => ({
  /** What the frame was handed, so the test can play the host: open a session, then end it. */
  events: null as PaneEvents | null,
}));

vi.mock("../talk/agent", () => ({
  mountAgentFrame: (_host: HTMLElement, _lang: string, on: PaneEvents) => {
    hoisted.events = on;
    return Promise.resolve(() => {});
  },
}));
// Stubbed except for what the pane reads while it draws — the height the box under it may take, and
// the reading this is about (`../talk/terminal`).
vi.mock("../talk/terminal", async (actual) => ({
  ...(await actual<typeof import("../talk/terminal")>()),
  endTerminal: vi.fn(async () => {}),
  pasteIntoTerminal: vi.fn(async () => {}),
}));
// The label is a live thing of its own; what it draws is not what this is about.
vi.mock("../talk/plate", () => ({
  mountPlate: () => ({
    opened: () => {}, closed: () => {}, named: () => {},
    focused: () => {}, stop: () => {},
  }),
}));

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  window.innerWidth = 1600;
  hoisted.events = null;
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
      project: 1,
      names: new Map(),
      start: { cwd: "/work/here" },
      autoStart: true,
      focused: true,
      onOpened: () => {},
      onSaid: () => {},
      onPath: () => {},
      onClosed: () => {},
      onDrop: () => {},
      onName: () => {},
      written: "",
      onWrite: () => {},
      onFocus: () => {},
    }));
  });
}

/** The host opens a terminal running `agent`, then the program in it exits with `code`. */
async function ran(agent: string | null, code: number | null): Promise<void> {
  await act(async () => { hoisted.events?.opened("session-7", "/work/here", agent); });
  await act(async () => { hoisted.events?.closed("session-7", code); });
  await act(async () => { await Promise.resolve(); });
}

/** What the pane says about the ending. */
const note = () => container.querySelector(".termface__note")?.textContent ?? "";

describe("what a pane says the program stopped for", () => {
  it("says which of the reader's own files to set, where the provider named the pane's", async () => {
    await pane();
    await ran("gemini-cli", 41);

    expect(note(), "the ending is still said, and the reason is said after it")
      .toContain("The program in this terminal has exited.");
    expect(note()).toContain("~/.gemini/settings.json");
  });

  // The common ending, which is every ending but a handful: the screen above says why, and the pane
  // says only that it happened.
  it("says nothing but that it ended, for an ending the program can answer for itself", async () => {
    await pane();
    await ran("gemini-cli", 0);

    expect(note()).toBe("The program in this terminal has exited.");
  });

  // The reading is of what was running when it stopped, which the pane learns after it is drawn. A
  // handler reading the state it was made beside would read the value from before the terminal
  // opened, and would say nothing here.
  it("reads what was running when it stopped, not what was there when the pane was drawn", async () => {
    await pane();
    await ran("gemini-cli", 41);

    expect(note(), "the pane was drawn before anything said which provider was in it")
      .toContain("~/.gemini/settings.json");
  });
});
