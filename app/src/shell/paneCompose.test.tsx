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
// them. There is no way to ask a terminal whether it is showing a menu, so nothing here tries.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PaneEvents } from "../talk/terminal";
import { t } from "../core/i18n";
import { TerminalPane } from "./TerminalPane";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const hoisted = vi.hoisted(() => ({
  /** What the frame was handed, so the test can play the host and open a session in the pane. */
  events: null as PaneEvents | null,
  /** What crossed to the host, in the order it crossed. */
  asked: [] as Array<{ cmd: string; args: Record<string, unknown> }>,
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
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

/** A pane on frame 1, working in `/work/here`. */
async function pane(autoStart = true): Promise<void> {
  await act(async () => {
    root.render(createElement(TerminalPane, {
      frame: "1",
      project: 3,
      names: new Map(),
      start: { cwd: "/work/here" },
      autoStart,
      focused: true,
      onOpened: () => {},
      onSaid: () => {},
      onPath: () => {},
      onClosed: () => {},
      onDrop: () => {},
      onName: () => {},
      onFocus: () => {},
    }));
  });
}

/** The host opens a terminal in the pane, which is what puts the box up. */
async function opened(): Promise<void> {
  await act(async () => { hoisted.events?.opened("session-7", "/work/here"); });
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

/** Write `text` in the box, the way a person does. */
async function write(text: string): Promise<void> {
  const field = box()!;
  await act(async () => {
    const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")?.set;
    setter?.call(field, text);
    field.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

/** Press a key in the box, and let whatever it set off settle. */
async function pressed(key: string, held: KeyboardEventInit = {}): Promise<boolean> {
  const event = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...held });
  await act(async () => { box()?.dispatchEvent(event); });
  await act(async () => { await Promise.resolve(); });
  return event.defaultPrevented;
}

/** What was written to the terminal, in the order it went. */
const wrote = () => hoisted.asked.filter((one) => one.cmd === "pty_write").map((one) => one.args.data);

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
  it("goes out on Enter, as a paste with the return behind it", async () => {
    await pane();
    await opened();
    await write("run the tests");

    expect(await pressed("Enter"), "the newline was typed into the box as well").toBe(true);
    expect(wrote()).toEqual(["\x1b[200~run the tests\x1b[201~", "\r"]);
  });

  it("asks for the sentence the pane may still owe, behind the person's own line", async () => {
    await pane();
    await opened();
    await write("run the tests");
    await pressed("Enter");

    expect(hoisted.asked.map((one) => one.cmd)).toEqual(["pty_write", "pty_write", "pty_brief"]);
  });

  it("is another line on Shift-Enter, and nothing crosses", async () => {
    await pane();
    await opened();
    await write("first line");

    expect(await pressed("Enter", { shiftKey: true }), "the newline was taken from the box").toBe(false);
    expect(wrote(), "half a sentence went out").toEqual([]);
  });

  it("sends nothing while nothing is written", async () => {
    await pane();
    await opened();

    await pressed("Enter");

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

    expect(await pressed("ArrowUp"), "the box gave up the line it was walking").toBe(false);
    expect(wrote(), "a press meant for the box reached the program").toEqual([]);
  });

  it("hands Ctrl+C on while nothing is written, which is what stops what is running", async () => {
    await pane();
    await opened();

    await pressed("c", { ctrlKey: true });

    expect(wrote()).toEqual(["\x03"]);
  });

  it("says which of the two the keyboard is answering to, and changes as the box does", async () => {
    await pane();
    await opened();

    expect(mark()?.title).toBe(t("face.composePasses"));
    await write("half a sentence");
    expect(mark()?.title).toBe(t("face.composeKeeps"));
  });

  it("is not the box the emulator collects typing in", async () => {
    await pane();
    await opened();

    expect(typing(), "the terminal's own box went missing").not.toBeNull();
    expect(box(), "the two boxes were taken for one").not.toBe(typing());
  });
});
