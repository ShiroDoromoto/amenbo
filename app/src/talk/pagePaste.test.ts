// The press a pane steps aside for, so the page pastes into it.
//
// On Windows `Ctrl+V` is `^V` to an emulator, and that is what reached the program — which left a
// pane with no road for a clipboard carrying files at all, because `Ctrl+Shift+V` is Chromium's
// "paste as text" and answers such a clipboard with nothing (`AMB-T-4574`). What is pinned here is
// how narrow the stepping aside is: one machine, one combination, keydown only.
import { describe, expect, it } from "vitest";
import { isPagePaste } from "./terminal";

/** A press, with the parts a test does not care about held down to nothing. */
function press(over: Partial<KeyboardEvent> = {}): KeyboardEvent {
  return {
    type: "keydown",
    key: "v",
    shiftKey: false,
    altKey: false,
    ctrlKey: true,
    metaKey: false,
    ...over,
  } as KeyboardEvent;
}

describe("the press a pane leaves to the page", () => {
  it("is Ctrl and V on Windows", () => {
    expect(isPagePaste(press(), "windows")).toBe(true);
    const caps = isPagePaste(press({ key: "V" }), "windows");
    expect(caps, "the caps of it are the same press").toBe(true);
  });

  it("is nobody's press on the other two machines", () => {
    expect(isPagePaste(press(), "macos"), "there the press is made with meta").toBe(false);
    expect(isPagePaste(press(), "other"), "there the press itself is read").toBe(false);
  });

  it("is not Ctrl+Shift+V — that one the emulator was already passing on", () => {
    expect(isPagePaste(press({ shiftKey: true }), "windows")).toBe(false);
  });

  it("is not the same press with Alt or meta held, which belong to the program", () => {
    expect(isPagePaste(press({ altKey: true }), "windows")).toBe(false);
    expect(isPagePaste(press({ metaKey: true }), "windows")).toBe(false);
  });

  it("is not V on its own", () => {
    expect(isPagePaste(press({ ctrlKey: false }), "windows")).toBe(false);
  });

  it("is not another letter held with Ctrl", () => {
    expect(isPagePaste(press({ key: "c" }), "windows")).toBe(false);
  });

  it("is the press, not the release", () => {
    expect(isPagePaste(press({ type: "keyup" }), "windows")).toBe(false);
  });
});
