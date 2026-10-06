// @vitest-environment jsdom
// Putting the keyboard on a terminal without moving the row the pages are laid in.
//
// The box the emulator collects typing in is kept far off to the left of the terminal, so a plain
// focus scrolls the row back to the first page whichever page the pane is on. jsdom scrolls nothing,
// so what is checked is that the focus is asked for without it.
import { describe, expect, it, vi } from "vitest";

import { focusTerminal } from "./terminal";

describe("putting the keyboard on a terminal", () => {
  it("focuses the emulator's box without scrolling to it", () => {
    const host = document.createElement("div");
    const box = document.createElement("textarea");
    host.append(box);
    document.body.append(host);
    const focus = vi.spyOn(box, "focus");
    focusTerminal(host);
    expect(focus).toHaveBeenCalledWith({ preventScroll: true });
    expect(document.activeElement).toBe(box);
    host.remove();
  });
});
