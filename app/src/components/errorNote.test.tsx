// @vitest-environment jsdom
// The note that says what went wrong (`AMB-D-686`).
//
// What this guards: three things travel together — the mark, the alert role that tells a reader who is
// looking elsewhere, and the colour the mark takes from the words. A note that keeps the look and drops
// the role is the failure this component exists to make impossible, and it is invisible on screen.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { ErrorNote } from "./ErrorNote";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the note that says what went wrong", () => {
  it("carries the mark and the alert role together", async () => {
    await act(async () => { root.render(createElement(ErrorNote, { children: "the folder is gone" })); });

    const note = container.querySelector(".errortext");
    expect(note?.getAttribute("role")).toBe("alert");
    expect(note?.textContent).toContain("the folder is gone");
    // Drawn, not typed: the mark is the icon set's, so it is one drawing for every note on every screen.
    expect(note?.querySelector("svg.icon")).not.toBeNull();
  });

  it("says a settings row's failure in the row's own voice", async () => {
    await act(async () => {
      root.render(createElement(ErrorNote, { tone: "quiet", children: "the name could not be saved" }));
    });

    const note = container.querySelector(".errortext");
    expect(note?.classList.contains("errortext--quiet")).toBe(true);
    expect(note?.getAttribute("role")).toBe("alert"); // quieter to look at, said just as loudly
  });
});
