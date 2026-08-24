// The one press a pane answers for instead of passing on.
//
// Everything else a person types goes through untouched, which is what makes the terminal a terminal.
// Shift-Enter is the exception because it cannot go through: an emulator gives Enter a carriage return
// and gives Shift-Enter the same one, so an agent that reads the two as "another line" and "send it"
// is handed a press it cannot tell apart. What is pinned here is the narrowness of the exception —
// every other combination is still the program's.
import { describe, expect, it } from "vitest";
import { isNewline, NEWLINE } from "./terminal";

/** A press, with the parts a test does not care about held down to nothing. */
function press(over: Partial<KeyboardEvent> = {}): KeyboardEvent {
  return {
    type: "keydown",
    key: "Enter",
    shiftKey: true,
    altKey: false,
    ctrlKey: false,
    metaKey: false,
    ...over,
  } as KeyboardEvent;
}

describe("the press a pane answers for", () => {
  it("is Shift and Enter", () => {
    expect(isNewline(press())).toBe(true);
    expect(NEWLINE, "what is sent is not the form the programs read").toBe("\x1b\r");
  });

  it("is not Enter on its own — that is the press that sends", () => {
    expect(isNewline(press({ shiftKey: false }))).toBe(false);
  });

  it("is not Shift and Enter with anything else held", () => {
    // What Alt or Ctrl with Enter means is the program's, and a pane that answered for those would be
    // deciding it.
    expect(isNewline(press({ altKey: true }))).toBe(false);
    expect(isNewline(press({ ctrlKey: true }))).toBe(false);
    expect(isNewline(press({ metaKey: true }))).toBe(false);
  });

  it("is not any other key, however it is held", () => {
    expect(isNewline(press({ key: "a" }))).toBe(false);
    expect(isNewline(press({ key: "ArrowUp" }))).toBe(false);
  });

  it("is the press and not the letting go — one press must not send two", () => {
    expect(isNewline(press({ type: "keyup" }))).toBe(false);
  });
});
