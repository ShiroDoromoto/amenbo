import { describe, expect, it } from "vitest";
import { NOTHING_TYPED, typed, type Typing } from "./frames";

/** Every keystroke in turn, as the emulator hands them over one press at a time. */
function press(keys: string[]): Typing {
  return keys.reduce(typed, NOTHING_TYPED);
}

describe("the first line a person types into a pane", () => {
  it("is the line they sent, not the keys they pressed", () => {
    expect(press(["m", "a", "k", "e", " ", "v", "e", "r", "i", "f", "y", "\r"]))
      .toMatchObject({ line: "make verify", sent: true });
  });

  it("has the editing they did to it applied", () => {
    expect(press(["c", "a", "r", "h", "\x7f", "g", "o", "\r"])).toMatchObject({ line: "cargo", sent: true });
    expect(press(["a", "\b", "b", "\r"])).toMatchObject({ line: "b", sent: true });
  });

  it("passes over the keys that are not characters", () => {
    // An arrow key is an escape sequence and Ctrl-C is a byte below space. Neither belongs in a name,
    // and neither sends the line — nor does any part of the sequence leak into it.
    expect(press(["\x1b", "[", "A", "\x03", "l", "s"])).toMatchObject({ line: "ls", sent: false });
    // A paste arrives inside its own brackets, which are sequences of exactly the same shape.
    expect(typed(NOTHING_TYPED, "\x1b[200~make verify\x1b[201~\r"))
      .toMatchObject({ line: "make verify", sent: true });
  });

  it("does not end the line on the newline inside one", () => {
    // Shift-Enter is sent as `ESC` and a carriage return (`./terminal`), which is a sequence of two
    // and passes over like any other. A name is what a person sent, and a line they are still writing
    // has not been sent.
    expect(press(["l", "s", "\x1b", "\r", "-", "a"])).toMatchObject({ line: "ls-a", sent: false });
  });

  it("starts again once a line has been sent", () => {
    const sent = press(["l", "s", "\r"]);
    expect(typed(sent, "p")).toMatchObject({ line: "p", sent: false });
  });

  it("comes whole out of a paste as easily as out of typing", () => {
    expect(typed(NOTHING_TYPED, "  make verify  \r")).toMatchObject({ line: "make verify", sent: true });
  });
});
