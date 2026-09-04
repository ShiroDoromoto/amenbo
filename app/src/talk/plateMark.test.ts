// @vitest-environment jsdom
// The marks on the row are the application's own icons and not characters (`AMB-D-686`). What is
// pinned here is the part a reader cannot see in `nameplate.ts`: that the drawing actually lands in
// the markup, that a place with no mark stays empty so the stylesheet folds it away, and that a
// redraw with the same mark leaves the drawing where it was.
import { describe, expect, it } from "vitest";
import { mountNameplate, type Plate } from "./nameplate";

const DOT = { frame: "pane-1", face: "out" } as const;

/** A row saying whatever is handed in, on a session holding nothing. */
function plate(say: Plate["say"], now: Plate["now"] = { kind: "idle" }): Plate {
  return { name: "the repo", dot: DOT, now, say };
}

const marks = (host: HTMLElement) =>
  [...host.querySelectorAll(".plate__mark")].map((one) =>
    one.firstElementChild?.getAttribute("data-icon") ?? null);

describe("the marks on a pane's row", () => {
  it("draws them as icons rather than as characters", () => {
    const host = document.createElement("div");
    const draw = mountNameplate(host);
    draw(plate({ kind: "waiting", text: "which of the two" }), "en");

    expect(marks(host)).toEqual([null, "pause"]);
    // The whole of it is a drawing: a glyph left beside the icon would be the old mark surviving.
    expect(host.querySelector(".plate__mark--say")!.textContent).toBe("");
    expect(host.querySelector('.plate__mark--say svg')!.getAttribute("viewBox")).toBe("0 0 24 24");
  });

  it("leaves the place empty where the part has no mark, so the row closes over it", () => {
    const host = document.createElement("div");
    const draw = mountNameplate(host);
    draw(plate({ kind: "note", text: "reading the store" }), "en");

    // `:empty` is what takes the place out of the row, so nothing may be left standing in it.
    expect(host.querySelector(".plate__mark--say")!.childElementCount).toBe(0);
    expect(marks(host)).toEqual([null, null]);
  });

  it("swaps the drawing when the mark changes, and leaves it alone when it does not", () => {
    const host = document.createElement("div");
    const draw = mountNameplate(host);
    draw(plate({ kind: "waiting", text: "which of the two" }), "en");
    const first = host.querySelector(".plate__mark--say")!.firstElementChild;

    draw(plate({ kind: "waiting", text: "or the other" }), "en");
    expect(host.querySelector(".plate__mark--say")!.firstElementChild, "the drawing was rebuilt")
      .toBe(first);

    draw(plate({ kind: "premise" }), "en");
    expect(marks(host)).toEqual([null, "warning"]);
  });

  it("marks a stopped task in the middle, where what it is about is said", () => {
    const host = document.createElement("div");
    const draw = mountNameplate(host);
    draw(plate({ kind: "silent" }, { kind: "one", ref: "AMB-T-1", title: "a task", stopped: true }), "en");

    expect(marks(host)).toEqual(["stop", null]);
  });
});
