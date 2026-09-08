// The road out of the box under a pane: what a written line becomes, and which presses leave the box
// for the program instead of staying in it (`AMB-D-864`).
//
// A line a person wrote and pressed send on is not a path handed to a pane. The path is left where it
// can be read and nothing is submitted (`AMB-D-793`); the line is the person's own act, so it goes
// with the return that sends it — and the opening sentence the pane may still owe rides out behind it,
// the same way it rides out behind an Enter pressed in the terminal itself (`AMB-D-805`).
//
// Which presses leave the box is decided by what is written in it and by nothing on the screen. An
// empty box has no history to walk, no word to complete and nothing to escape from; a box holding a
// half-written sentence has all three, and keeps them.
import { describe, expect, it, vi } from "vitest";
import { boxHeight, passedOn, pressIntoTerminal, sendIntoTerminal } from "./terminal";

const hoisted = vi.hoisted(() => ({
  /** What crossed to the host, in the order it crossed. */
  asked: [] as Array<{ cmd: string; args: Record<string, unknown> }>,
}));

vi.mock("../core/ipc", () => ({
  invoke: vi.fn(async (cmd: string, args: Record<string, unknown>) => {
    hoisted.asked.push({ cmd, args });
  }),
}));

/** A press, as a page or React reports one. Nothing is held with it unless the test says so. */
function press(key: string, held: Partial<Record<"altKey" | "ctrlKey" | "metaKey" | "shiftKey", boolean>> = {}) {
  return { key, altKey: false, ctrlKey: false, metaKey: false, shiftKey: false, ...held };
}

describe("sending a line a person wrote", () => {
  it("pastes it, sends the return behind it, and asks for the sentence the pane may owe", async () => {
    hoisted.asked = [];

    await sendIntoTerminal("session-7", "run the tests");

    expect(hoisted.asked).toEqual([
      { cmd: "pty_write", args: { session: "session-7", data: "\x1b[200~run the tests\x1b[201~" } },
      { cmd: "pty_write", args: { session: "session-7", data: "\r" } },
      { cmd: "pty_brief", args: { session: "session-7" } },
    ]);
  });

  it("keeps the newlines in one message rather than sending that many lines", async () => {
    hoisted.asked = [];

    await sendIntoTerminal("session-7", "first line\nsecond line");

    expect(hoisted.asked[0]?.args.data, "the paste stopped wrapping what was written")
      .toBe("\x1b[200~first line\nsecond line\x1b[201~");
  });
});

describe("handing a press on to the program", () => {
  it("writes it as it stands, with nothing wrapped round it", async () => {
    hoisted.asked = [];

    await pressIntoTerminal("session-7", "\x1b[A");

    expect(hoisted.asked).toEqual([
      { cmd: "pty_write", args: { session: "session-7", data: "\x1b[A" } },
    ]);
  });
});

describe("which presses an empty box hands on", () => {
  it("hands on the four that have nothing to do in a box with nothing in it", () => {
    expect(passedOn(press("ArrowUp"))).toBe("\x1b[A");
    expect(passedOn(press("ArrowDown"))).toBe("\x1b[B");
    expect(passedOn(press("Tab"))).toBe("\t");
    expect(passedOn(press("Escape"))).toBe("\x1b");
  });

  it("hands on Ctrl+C, which is what stops what is running", () => {
    expect(passedOn(press("c", { ctrlKey: true }))).toBe("\x03");
    expect(passedOn(press("C", { ctrlKey: true }))).toBe("\x03");
  });

  it("keeps the rest of the control keys, which are a text box's own", () => {
    expect(passedOn(press("a", { ctrlKey: true })), "select-all left the box").toBeNull();
    expect(passedOn(press("e", { ctrlKey: true })), "end-of-line left the box").toBeNull();
  });

  it("keeps a press held with anything else, which means something the box is not being asked", () => {
    expect(passedOn(press("ArrowUp", { shiftKey: true })), "selecting upwards left the box").toBeNull();
    expect(passedOn(press("ArrowUp", { metaKey: true }))).toBeNull();
    expect(passedOn(press("ArrowUp", { altKey: true }))).toBeNull();
    expect(passedOn(press("c", { ctrlKey: true, shiftKey: true }))).toBeNull();
  });

  it("keeps every ordinary press, which is what the box is for", () => {
    expect(passedOn(press("a"))).toBeNull();
    expect(passedOn(press("Enter"))).toBeNull();
    expect(passedOn(press("Backspace"))).toBeNull();
    expect(passedOn(press("ArrowLeft")), "moving inside the line left the box").toBeNull();
  });
});

describe("how tall the box under a pane is", () => {
  // One line is 20, the pane stands at 400 and draws 20 rows — so a row is 20 too, and the floor of
  // 8 rows leaves 240 of the pane to give up.
  const pane = { standing: 20, line: 20, pane: 400, rows: 20 };

  it("is one line while nothing is written in it", () => {
    expect(boxHeight({ ...pane, content: 20 })).toBe(20);
  });

  it("grows to what is written", () => {
    expect(boxHeight({ ...pane, content: 60 })).toBe(60);
  });

  it("comes back down when what was written goes", () => {
    expect(boxHeight({ ...pane, standing: 60, content: 20 })).toBe(20);
  });

  it("stops at its own cap, so a document scrolls inside it rather than filling the pane", () => {
    expect(boxHeight({ ...pane, content: 2000 })).toBe(200);
  });

  it("stops earlier where the terminal would be cut below its floor", () => {
    // A short pane: 100 tall over 10 rows, so 8 rows are 80 and only 20 can be given up.
    expect(boxHeight({ standing: 20, line: 20, pane: 100, rows: 10, content: 2000 })).toBe(40);
  });

  it("keeps one line even where the terminal has nothing to give up", () => {
    expect(boxHeight({ standing: 20, line: 20, pane: 80, rows: 8, content: 200 })).toBe(20);
  });

  it("answers the same height again once it has grown, rather than creeping up", () => {
    // What the two share does not change: the box took 40 and the pane gave 40.
    const grown = boxHeight({ standing: 20, line: 20, pane: 100, rows: 10, content: 2000 });
    expect(boxHeight({ standing: grown, line: 20, pane: 100 - (grown - 20), rows: 8, content: 2000 }))
      .toBe(grown);
  });

  it("is held by its own cap alone before the pane has said how many rows it draws", () => {
    expect(boxHeight({ standing: 20, line: 20, pane: 400, rows: 0, content: 2000 })).toBe(200);
  });
});
