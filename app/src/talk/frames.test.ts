import { describe, expect, it } from "vitest";
import {
  folderName, frameLabel, NOTHING_TYPED, pressedKey, typed, type Pressed, type Typing,
} from "./frames";

/** A press, with the parts a test does not care about held down to nothing. */
function press(over: Partial<KeyboardEvent> = {}): KeyboardEvent {
  return {
    key: "a",
    isComposing: false,
    shiftKey: false,
    altKey: false,
    ctrlKey: false,
    metaKey: false,
    ...over,
  } as KeyboardEvent;
}

/** Every key in turn, as the emulator hands them over one press at a time. */
function pressing(keys: (string | Partial<KeyboardEvent>)[]): Typing {
  return keys.reduce<Typing>((so_far, key) => {
    const did = pressedKey(press(typeof key === "string" ? { key } : key));
    return did === null ? so_far : typed(so_far, did);
  }, NOTHING_TYPED);
}

describe("the first line a person types into a pane", () => {
  it("is the line they sent, not the keys they pressed", () => {
    expect(pressing([..."make verify", "Enter"])).toEqual({ line: "make verify", sent: true });
  });

  it("has the editing they did to it applied", () => {
    expect(pressing([..."carh", "Backspace", ..."go", "Enter"])).toEqual({ line: "cargo", sent: true });
  });

  it("passes over the presses that are not characters", () => {
    // An arrow key is a key with a name rather than a character, and Ctrl-C is a key held as a
    // command. Neither belongs in a name, and neither sends the line.
    expect(pressing(["ArrowUp", "Tab", "Escape", { key: "c", ctrlKey: true }, ..."ls"]))
      .toEqual({ line: "ls", sent: false });
  });

  it("does not end the line on the newline inside one", () => {
    // Shift-Enter is the pane writing a newline into the line being typed (`./terminal`). A name is
    // what a person sent, and a line they are still writing has not been sent.
    expect(pressing([..."ls", { key: "Enter", shiftKey: true }, ..."-a"]))
      .toEqual({ line: "ls-a", sent: false });
  });

  it("starts again once a line has been sent", () => {
    const sent = pressing([..."ls", "Enter"]);
    expect(typed(sent, { kind: "text", text: "p" })).toEqual({ line: "p", sent: false });
  });

  it("takes what an input method wrote as the one thing it settled", () => {
    // The keys of a composition are the input method's; what the person wrote is the text it ends
    // with, and it arrives whole (`./terminal`).
    expect(pressedKey(press({ key: "Process", isComposing: true }))).toBeNull();
    expect(pressedKey(press({ key: "Enter", isComposing: true }))).toBeNull();
    const composed: Pressed = { kind: "text", text: "移行の続き" };
    expect(typed(typed(NOTHING_TYPED, composed), { kind: "send" }))
      .toEqual({ line: "移行の続き", sent: true });
  });

  it("counts a character and not a code unit, so taking one back takes the whole of it", () => {
    expect(pressedKey(press({ key: "🐜" }))).toEqual({ kind: "text", text: "🐜" });
    expect(typed({ line: "ls 🐜", sent: false }, { kind: "erase" })).toEqual({ line: "ls ", sent: false });
  });
});

describe("what the terminal sends is not a name", () => {
  // The pane's own answer to a program asking what colour it is drawn in used to reach the name, and
  // a nameplate read `10;rgb:ecec/e9e9/…` (`AMB-T-3668`). It never was a press, and only presses name
  // a frame now — so there is nothing on that stream for a program to name a pane with.
  it("never becomes a press", () => {
    const reply = "\x1b]10;rgb:ecec/e9e9/e1e1\x1b\\";
    for (const char of reply) expect(pressedKey(press({ key: char }))).not.toEqual({ kind: "send" });
    // The one thing in it that would have ended a line is the escape, which is a key with a name.
    expect(pressedKey(press({ key: "Escape" }))).toBeNull();
  });
});

describe("what a pane is called before anything has named it", () => {
  it("is the folder it works in", () => {
    expect(folderName("/work/amenbo")).toBe("amenbo");
    expect(folderName("C:\\work\\amenbo")).toBe("amenbo");
    expect(folderName("/work/repo/")).toBe("repo");
    expect(folderName("/")).toBeNull();
    expect(folderName(null)).toBeNull();
  });

  it("gives way to the name the moment there is one", () => {
    const names = new Map([["1", "the migration"]]);
    expect(frameLabel(names, "1", "/work/amenbo")).toBe("the migration");
    expect(frameLabel(names, "2", "/work/amenbo")).toBe("amenbo");
    expect(frameLabel(names, "2", null)).toBeNull();
  });
});
