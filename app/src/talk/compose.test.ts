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
// half-written sentence has all three, and keeps them — except the ArrowUp on its first line, which
// is the one road out and takes the keyboard with it.
import { describe, expect, it, vi } from "vitest";
import { leavesForTerminal, passedOn, pressIntoTerminal, sendIntoTerminal } from "./terminal";

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

describe("the way out of a box with something written in it", () => {
  /** Where the caret sits when a person has just written `text` and not moved. */
  const end = (text: string) => text.length;

  it("hands on the ArrowUp pressed on the first line, which a box does nothing with", () => {
    expect(leavesForTerminal(press("ArrowUp"), "half a sentence", end("half a sentence"))).toBe("\x1b[A");
    expect(leavesForTerminal(press("ArrowUp"), "half a sentence", 0), "the caret at the front").toBe("\x1b[A");
  });

  it("keeps it below the first line, where it walks up through what is written", () => {
    const written = "first line\nsecond line";

    expect(leavesForTerminal(press("ArrowUp"), written, end(written)), "the second line lost its ArrowUp")
      .toBeNull();
    // The newline itself is still the first line: the caret in front of it has nothing above it.
    expect(leavesForTerminal(press("ArrowUp"), written, "first line".length)).toBe("\x1b[A");
  });

  it("is not this road while the box is empty, where every press of the four goes on anyway", () => {
    expect(leavesForTerminal(press("ArrowUp"), "", 0)).toBeNull();
  });

  it("is the ArrowUp alone — not a press held with something, and not another key", () => {
    expect(leavesForTerminal(press("ArrowUp", { shiftKey: true }), "a line", 6), "selecting upwards left")
      .toBeNull();
    expect(leavesForTerminal(press("ArrowUp", { metaKey: true }), "a line", 6)).toBeNull();
    expect(leavesForTerminal(press("ArrowDown"), "a line", 6), "the way out went downwards").toBeNull();
    expect(leavesForTerminal(press("Escape"), "a line", 6)).toBeNull();
  });
});
