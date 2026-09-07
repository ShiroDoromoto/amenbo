// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { faceOf, mountNameplate, type Dot } from "./nameplate";

/** The lamp in front of the name, at rest. These cases are about the name on the row; what the lamp
 *  does has its own (`./plateMoving.test`). */
const STILL: Dot = { frame: "1", face: "out" };

describe("the row on the page", () => {
  it("is one line, and redraws in place", () => {
    const host = document.createElement("div");
    const draw = mountNameplate(host);

    draw({ name: "the migration", dot: STILL });
    expect(host.querySelector(".plate__name")?.textContent).toBe("the migration");

    const row = host.querySelector(".plate");
    draw({ name: "the backup", dot: STILL });
    expect(host.querySelectorAll(".plate")).toHaveLength(1);
    // Redrawn in place: the row is not rebuilt, so nothing under the pointer moves.
    expect(host.querySelector(".plate")).toBe(row);
    expect(host.querySelector(".plate__name")?.textContent).toBe("the backup");
    // The row carries no tooltip of its own: the name in full is in the panel instead, and the
    // machine's own tooltip over that panel would be the same word twice.
    expect(host.querySelector(".plate__name")?.getAttribute("title")).toBeNull();
    expect(host.querySelector(".plate-peek__name")?.textContent).toBe("the backup");
  });

  it("says the name in full where the row had to cut it, and nothing else", () => {
    // The panel is the way back to a name the row elided. It is the name and no second reading of
    // it: a pane's row says what the pane is called, and that is the whole of what it says
    // (`AMB-D-862`).
    const host = document.createElement("div");
    const draw = mountNameplate(host);
    const name = "the migration that moves the store and then puts the pointer back";

    draw({ name, dot: STILL });
    expect(host.querySelector(".plate-peek__name")?.textContent).toBe(name);
    expect(host.querySelector(".plate-peek")?.textContent).toBe(name);
  });

  it("keeps the panel down for a pane nothing has named", () => {
    // An empty box with a border on it reads as something having gone wrong, and a pane with no name
    // has nothing to put in one.
    const host = document.createElement("div");
    const draw = mountNameplate(host);

    draw({ name: null, dot: STILL });
    expect((host.querySelector(".plate-peek") as HTMLElement).hidden).toBe(true);
  });

  it("comes down when there is nothing to label, and comes back with the same row", () => {
    const host = document.createElement("div");
    const draw = mountNameplate(host);

    draw(null);
    const row = host.querySelector(".plate");
    expect(row, "the row was removed rather than hidden").toBeTruthy();
    expect((row as HTMLElement).hidden, "a pane with no session was labelled anyway").toBe(true);
    // The panel goes with the row. A pane that has never had a session has nothing to put in it, and
    // a panel left up would be an empty box with a border on it — dropped by a pointer on the row's
    // place, or by the keyboard reaching the button beside it.
    expect(
      (host.querySelector(".plate-peek") as HTMLElement).hidden,
      "a pane with no session kept an empty panel",
    ).toBe(true);

    draw({ name: null, dot: STILL });
    expect(host.querySelector(".plate")).toBe(row);
    expect((row as HTMLElement).hidden).toBe(false);
  });
});

describe("which face the lamp is on", () => {
  it("is lit while output is arriving, and out when it is not", () => {
    // The stream and nothing else. What silence means is not knowable from outside, so the lamp does
    // not try to say (`AMB-D-858`, `AMB-D-862`).
    expect(faceOf(true)).toBe("lit");
    expect(faceOf(false)).toBe("out");
  });
});
