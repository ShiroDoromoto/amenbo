// @vitest-environment jsdom
// The box under a pane, where a line is written before it is sent (`AMB-D-864`).
//
// What is pinned here is the shape of it: the box stands under the terminal in the same column and is
// there for as long as a terminal is, whatever the keyboard is doing — a box that came and went with
// the focus would change the pane's height twice per visit, and every change of height wakes the
// program inside to repaint (`../talk/terminal`).
//
// And that **what a press means is decided by what is written, not by what is on the screen**. An
// empty box hands the arrows, the tab and Escape to the program; a box with something in it keeps
// them, but for one — the ArrowUp on its first line, which is the way back to a program that is
// asking something. There is no way to ask a terminal whether it is showing a menu, so nothing here
// tries.
import { act, createElement, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PaneEvents } from "../talk/terminal";
import { pauseBeforeTheReturn } from "../talk/terminal";
import { t } from "../core/i18n";
import { TerminalPane } from "./TerminalPane";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const hoisted = vi.hoisted(() => ({
  /** What the frame was handed, so the test can play the host and open a session in the pane. */
  events: null as PaneEvents | null,
  /** What crossed to the host, in the order it crossed. */
  asked: [] as Array<{ cmd: string; args: Record<string, unknown> }>,
  /** What the window is holding for this pane, as it last drew. */
  held: "",
}));

vi.mock("../talk/agent", () => ({
  mountAgentFrame: (host: HTMLElement, _lang: string, on: PaneEvents) => {
    hoisted.events = on;
    // The box the emulator collects typing in. It is kept because the one under the pane must not be
    // mistaken for it — what takes the keyboard on a drop is the terminal's, and only that one.
    host.append(document.createElement("textarea"));
    return Promise.resolve(() => {});
  },
}));
// The terminal's own road is left real: what a written line becomes on the way out is the whole of
// what this is about, so a stub of it would test nothing (`../talk/terminal`).
vi.mock("../talk/terminal", async (actual) => ({
  ...(await actual<typeof import("../talk/terminal")>()),
  endTerminal: vi.fn(async () => {}),
}));
vi.mock("../core/hostDrop", () => ({ watchHostDrop: vi.fn(async () => () => {}) }));
vi.mock("../core/dialog", () => ({
  confirmDialog: vi.fn(async () => true),
  pickFiles: vi.fn(async () => []),
  pickFolders: vi.fn(async () => []),
}));
vi.mock("../core/notice", () => ({ pushNotice: vi.fn() }));
vi.mock("../core/ipc", () => ({
  invoke: vi.fn(async (cmd: string, args: Record<string, unknown>) => {
    hoisted.asked.push({ cmd, args });
  }),
}));
// The label above the pane is a live thing of its own; what it draws is not what this is about.
vi.mock("../talk/plate", () => ({
  mountPlate: () => ({
    opened: () => {}, closed: () => {}, named: () => {},
    focused: () => {}, stop: () => {},
  }),
}));

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  hoisted.events = null;
  hoisted.asked = [];
  hoisted.held = "";
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

/**
 * The window the pane is drawn in, as far as this is about: the one that holds what has been written
 * and hands it back down (`../talk/layout`). It is played rather than stubbed because where the
 * draft lives is half of what is pinned here — a pane holding its own would pass every test below
 * and still lose a sentence on the next page turn.
 */
function Window(
  { autoStart = true, put, working = true }:
    { autoStart?: boolean; put?: (text: string) => void; working?: boolean },
) {
  const [written, setWritten] = useState("");
  // Which pane is being worked in, which is the window's answer and not the pane's: the pane asks
  // for it on a press and reads it back on the render after. A test that held it still could not
  // tell a press that moved the frame from one that landed where it already was.
  const [focused, setFocused] = useState(working);
  put?.(written);
  return createElement(TerminalPane, {
    frame: "1",
    project: 3,
    names: new Map(),
    start: { cwd: "/work/here" },
    autoStart,
    focused,
    written,
    onWrite: (_frame: string, text: string) => setWritten(text),
    onOpened: () => {},
    onSaid: () => {},
    onPath: () => {},
    onClosed: () => {},
    onDrop: () => {},
    onName: () => {},
    onFocus: () => setFocused(true),
  });
}

/** A pane on frame 1, working in `/work/here` — the pane being worked in unless `working` says not. */
async function pane(autoStart = true, working = true): Promise<void> {
  await act(async () => {
    root.render(createElement(Window, { autoStart, working, put: (text) => { hoisted.held = text; } }));
  });
}

/** The host opens a terminal in the pane, which is what puts the box up. */
async function opened(): Promise<void> {
  await act(async () => { hoisted.events?.opened("session-7", "/work/here", null); });
  await act(async () => { await Promise.resolve(); });
}

/** The host says the program in the pane has exited. */
async function closed(): Promise<void> {
  await act(async () => { hoisted.events?.closed("session-7"); });
  await act(async () => { await Promise.resolve(); });
}

/** The box a line is written in, while the pane draws one. */
const box = () => container.querySelector<HTMLTextAreaElement>(".compose__box");
/** The mark that says which of the two the keyboard is answering to. */
const mark = () => container.querySelector<HTMLElement>(".compose__mark");
/** The press that sends what is written. */
const sendBtn = () => container.querySelector<HTMLButtonElement>(".compose__send");
/** The box the emulator collects typing in, which is the terminal's own. */
const typing = () => container.querySelector<HTMLTextAreaElement>(".termface__face textarea");
/** The pane itself, which is what a press on any part of it reaches on its way up. */
const slot = () => container.querySelector<HTMLElement>(".slot");

/** Press on `el` the way a person putting the pointer down on it does. */
async function pressOn(el: Element | null): Promise<void> {
  await act(async () => { el?.dispatchEvent(new MouseEvent("mousedown", { bubbles: true })); });
  await act(async () => { await Promise.resolve(); });
}

/** Write `text` in the box, the way a person does — into the box that holds the keyboard. */
async function write(text: string): Promise<void> {
  const field = box()!;
  await act(async () => { field.focus(); });
  await act(async () => {
    const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")?.set;
    setter?.call(field, text);
    field.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

/** Put the caret at `at` in the box, the way moving about in a written line does. */
function caretAt(at: number): void {
  const field = box()!;
  field.setSelectionRange(at, at);
}

/** Press a key in the box, and let whatever it set off settle. */
async function pressed(key: string, held: KeyboardEventInit = {}): Promise<boolean> {
  const event = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...held });
  await act(async () => { box()?.dispatchEvent(event); });
  await act(async () => { await Promise.resolve(); });
  return event.defaultPrevented;
}

/** Wait out the pause a send leaves between the paste and the return (`../talk/terminal`). The pane
 *  these tests open runs no agent, so what it waits is the wait for a provider nobody measured
 *  (`AMB-D-879`). */
async function sent(): Promise<void> {
  const pause = pauseBeforeTheReturn(null);
  await act(async () => { await new Promise((over) => setTimeout(over, pause + 5)); });
}

/** What was written to the terminal, in the order it went. */
const wrote = () => hoisted.asked.filter((one) => one.cmd === "pty_write").map((one) => one.args.data);

/** What is held down to send what is written (`AMB-D-876`). It is the control key here because jsdom
 *  reports no user agent this app places on macOS, which is the same answer Windows and Linux get. */
const SEND = { ctrlKey: true };

describe("the box under a pane", () => {
  it("is not there before a terminal is", async () => {
    await pane(false);
    expect(box(), "a place with nothing running in it offered somewhere to write").toBeNull();
  });

  it("stands under the terminal for as long as one is running", async () => {
    await pane();
    await opened();

    expect(box(), "a running pane had nowhere to write a line").not.toBeNull();
    // The slot is one column: the row, the terminal, the box. The box being last is what makes the
    // terminal give room up rather than be covered.
    const slot = container.querySelector(".slot")!;
    expect(slot.lastElementChild?.className).toContain("compose");
  });

  it("goes with the terminal, because there is nothing left to write to", async () => {
    await pane();
    await opened();
    await closed();

    expect(box(), "a line could be written to a program that had exited").toBeNull();
  });

  it("is one line high whether or not anything is written in it", async () => {
    await pane();
    await opened();

    expect(box()?.rows).toBe(1);
    await write("a line long enough to have wrapped");
    expect(box()?.rows, "the box grew with what was written").toBe(1);
  });
});

describe("sending what was written", () => {
  it("goes out on the send press, as a paste with the return behind it", async () => {
    await pane();
    await opened();
    await write("run the tests");

    expect(await pressed("Enter", SEND), "the newline was typed into the box as well").toBe(true);
    await sent();
    expect(wrote()).toEqual(["\x1b[200~run the tests\x1b[201~", "\r"]);
  });

  it("asks for the sentence the pane may still owe, behind the person's own line", async () => {
    await pane();
    await opened();
    await write("run the tests");
    await pressed("Enter", SEND);
    await sent();

    expect(hoisted.asked.map((one) => one.cmd)).toEqual(["pty_write", "pty_write", "pty_brief"]);
  });

  // The whole of what `AMB-D-876` moved: a long instruction is written with Enter like anything
  // else, and nothing crosses until the send press is made.
  it("is another line on Enter, and on Shift-Enter, and nothing crosses", async () => {
    await pane();
    await opened();
    await write("first line");

    expect(await pressed("Enter"), "the newline was taken from the box").toBe(false);
    expect(await pressed("Enter", { shiftKey: true }), "the newline was taken from the box").toBe(false);
    expect(wrote(), "half a sentence went out").toEqual([]);
    expect(box()?.value, "the box was emptied by a press that sends nothing").toBe("first line");
  });

  it("empties the box once the line has gone", async () => {
    await pane();
    await opened();
    await write("run the tests");

    await pressed("Enter", SEND);
    await sent();

    expect(box()?.value, "the next sentence would have been the tail of this one").toBe("");
    expect(hoisted.held, "the window went on holding a line that had been sent").toBe("");
  });

  it("leaves what was written where it is when the send is refused", async () => {
    await pane();
    await opened();
    await write("run the tests");
    hoisted.asked = [];
    // The terminal ended between the writing and the send, which is the whole of what can refuse.
    const ipc = await import("../core/ipc");
    vi.mocked(ipc.invoke).mockRejectedValueOnce(new Error("that terminal is not there any more"));

    await pressed("Enter", SEND);

    expect(box()?.value, "the only copy of what was written was thrown away").toBe("run the tests");
  });

  it("is what the window hands down, so a pane put up again draws it", async () => {
    await pane();
    await opened();
    await write("half a sentence");

    expect(hoisted.held, "the pane kept the draft to itself").toBe("half a sentence");
    expect(box()?.value).toBe("half a sentence");
  });

  it("sends nothing while nothing is written", async () => {
    await pane();
    await opened();

    await pressed("Enter", SEND);

    expect(wrote(), "an empty line was sent").toEqual([]);
    expect(sendBtn()?.disabled, "the press to send was alive with nothing to send").toBe(true);
  });

  it("goes out on the press beside the box too", async () => {
    await pane();
    await opened();
    await write("run the tests");

    expect(sendBtn()?.disabled).toBe(false);
    await act(async () => { sendBtn()?.click(); });
    await act(async () => { await Promise.resolve(); });
    await sent();

    expect(wrote()).toEqual(["\x1b[200~run the tests\x1b[201~", "\r"]);
  });
});

describe("where a press goes", () => {
  it("hands the arrows on while nothing is written", async () => {
    await pane();
    await opened();

    expect(await pressed("ArrowUp"), "the press stayed in the box").toBe(true);
    expect(wrote()).toEqual(["\x1b[A"]);
  });

  it("keeps them once something is written", async () => {
    await pane();
    await opened();
    await write("half a sentence");

    expect(await pressed("ArrowDown"), "the box gave up the line it was walking").toBe(false);
    expect(wrote(), "a press meant for the box reached the program").toEqual([]);
  });

  it("goes to the terminal on the first line's ArrowUp, keyboard and press together", async () => {
    await pane();
    await opened();
    await write("half a sentence");

    expect(await pressed("ArrowUp"), "the press stayed in the box").toBe(true);
    expect(wrote(), "the way out reached the program as something else").toEqual(["\x1b[A"]);
    expect(document.activeElement, "the keyboard stayed in the box the press left").toBe(typing());
    expect(box()?.value, "what was written was thrown away on the way out").toBe("half a sentence");
    expect(mark()?.title, "the mark went on naming a box the presses had left").toBe(t("face.composePasses"));
  });

  it("walks up through what is written until the first line, and leaves from there", async () => {
    await pane();
    await opened();
    await write("first line\nsecond line");

    expect(await pressed("ArrowUp"), "the second line's ArrowUp was taken from the box").toBe(false);
    expect(wrote(), "a press meant for the box reached the program").toEqual([]);

    caretAt(0);
    expect(await pressed("ArrowUp")).toBe(true);
    expect(wrote()).toEqual(["\x1b[A"]);
  });

  it("hands Ctrl+C on while nothing is written, which is what stops what is running", async () => {
    await pane();
    await opened();

    await pressed("c", { ctrlKey: true });

    expect(wrote()).toEqual(["\x03"]);
  });

  // The one exception to "what is written decides where a press goes" (`AMB-D-876`): a person
  // reaches for these because something went wrong, and by then the next line is half typed.
  it("hands the stopping keys on from a written box too, and leaves the line where it is", async () => {
    await pane();
    await opened();
    await write("half a sentence");

    expect(await pressed("Escape"), "the box swallowed the press that stops what is running").toBe(true);
    expect(await pressed("c", { ctrlKey: true })).toBe(true);

    expect(wrote()).toEqual(["\x1b", "\x03"]);
    expect(box()?.value, "stopping something threw away what was being written").toBe("half a sentence");
  });

  // Stopping is not leaving the sentence: the ArrowUp on the first line is the road that moves the
  // keyboard, and these two are not it.
  it("leaves the keyboard in the box the stopping keys were pressed in", async () => {
    await pane();
    await opened();
    await write("half a sentence");
    box()?.focus();

    await pressed("Escape");

    expect(document.activeElement, "the keyboard went with the press").toBe(box());
    expect(mark()?.title, "the mark stopped naming the box a person was still writing in")
      .toBe(t("face.composeKeeps"));
  });

  it("says which of the two the keyboard is answering to, and changes as the keyboard moves", async () => {
    await pane();
    await opened();

    expect(mark()?.title).toBe(t("face.composePasses"));
    await write("half a sentence");
    expect(mark()?.title).toBe(t("face.composeKeeps"));
  });

  it("names the box as soon as the keyboard is in it, nothing written yet", async () => {
    await pane();
    await opened();

    // A person clicking into the box before they have typed anything. Every character they are about
    // to type is the box's, so the mark that names the terminal would be untrue from here on.
    await act(async () => { box()?.focus(); });

    expect(mark()?.title, "the mark named the terminal while the box held the keyboard")
      .toBe(t("face.composeKeeps"));
    // And what the empty box does hand on is unchanged: the mark says where a press goes, and it is
    // not what decides it.
    expect(await pressed("ArrowUp"), "the press stayed in the box").toBe(true);
    expect(wrote(), "the history an empty box hands on never reached the program").toEqual(["\x1b[A"]);
  });

  it("names the terminal again once the keyboard goes there, line still written", async () => {
    await pane();
    await opened();
    await write("half a sentence");

    // A person clicking into the terminal, which takes the keyboard and leaves the line alone.
    await act(async () => { box()?.blur(); });

    expect(mark()?.title, "the mark named a box the presses no longer reach").toBe(t("face.composePasses"));
    expect(box()?.value, "the line went with the keyboard").toBe("half a sentence");
  });

  it("is not the box the emulator collects typing in", async () => {
    await pane();
    await opened();

    expect(typing(), "the terminal's own box went missing").not.toBeNull();
    expect(box(), "the two boxes were taken for one").not.toBe(typing());
  });
});

describe("a press that moves the pane being worked in", () => {
  it("puts the keyboard in the box", async () => {
    await pane(true, false);
    await opened();

    await pressOn(slot());

    expect(document.activeElement, "the frame moved and the keyboard stayed where it was")
      .toBe(box());
  });

  it("leaves the keyboard alone inside the pane already being worked in", async () => {
    await pane();
    await opened();
    // A person pressing the terminal, which is how the keyboard is handed to the program in it.
    await act(async () => { typing()?.focus(); });

    await pressOn(typing());

    expect(document.activeElement, "a press in the pane took the keyboard off the program")
      .toBe(typing());
  });

  it("leaves the keyboard alone where the pane has nothing running in it", async () => {
    await pane(false, false);
    const elsewhere = document.createElement("textarea");
    document.body.append(elsewhere);
    elsewhere.focus();

    await pressOn(slot());

    expect(box(), "a place with nothing running in it drew somewhere to write").toBeNull();
    expect(document.activeElement, "the keyboard was dropped on the page").toBe(elsewhere);
    elsewhere.remove();
  });
});
