// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { tf } from "../core/i18n";
import { faceOf, mountNameplate, type Dot } from "./nameplate";

/** The lamp in front of the name, at rest. These cases are about the name on the row; what the lamp
 *  does has its own (`./plateMoving.test`). */
const STILL: Dot = { hue: 199, face: "out" };

describe("the row on the page", () => {
  it("is one line, and redraws in place", () => {
    const host = document.createElement("div");
    const draw = mountNameplate(host);

    draw({ name: "the migration", dot: STILL, run: null });
    expect(host.querySelector(".plate__name")?.textContent).toBe("the migration");

    const row = host.querySelector(".plate");
    draw({ name: "the backup", dot: STILL, run: null });
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

    draw({ name, dot: STILL, run: null });
    expect(host.querySelector(".plate-peek__name")?.textContent).toBe(name);
    expect(host.querySelector(".plate-peek")?.textContent).toBe(name);
  });

  it("keeps the panel down for a pane nothing has named", () => {
    // An empty box with a border on it reads as something having gone wrong, and a pane with no name
    // has nothing to put in one.
    const host = document.createElement("div");
    const draw = mountNameplate(host);

    draw({ name: null, dot: STILL, run: null });
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

    draw({ name: null, dot: STILL, run: null });
    expect(host.querySelector(".plate")).toBe(row);
    expect((row as HTMLElement).hidden).toBe(false);
  });
});

describe("the row above a run's pane", () => {
  /** Where a run has got to, as the face hands it over (`./nameplate`). */
  const RUN = {
    run: 7,
    seq: 3,
    step: "取る",
    action: "下ごしらえ",
    task: { ref: "AMB-T-5252", title: "ペインのヘッダを描く" },
  };

  it("says the four values, and marks the pane as a run's", () => {
    // All four are Amenbo's own — three off the execution row and one off the ledger — which is the
    // whole of why they can be said at all (`AMB-D-858`). What the step printed is not among them:
    // that is in the terminal under the row.
    const host = document.createElement("div");
    const draw = mountNameplate(host);

    draw({ name: "/work/a", dot: STILL, run: RUN });

    expect(host.querySelector(".plate__auto")?.textContent).toBeTruthy();
    expect((host.querySelector(".plate__auto") as HTMLElement).hidden).toBe(false);
    // Which step, with the spot of the picture it was opened from: two spots standing on the same
    // action run steps of the same names, so the step's own name does not say which this is
    // (`AMB-D-949`).
    expect(host.querySelector(".plate-run__step")?.textContent)
      .toBe(tf("auto.run.inAction", { action: "下ごしらえ", step: "取る" }));
    expect(host.querySelector(".plate-run__seq")?.textContent).toContain("3");
    expect(host.querySelector(".plate-run__no")?.textContent).toContain("7");
    expect(host.querySelector(".plate-run__task")?.textContent).toBe("AMB-T-5252");
    // The row has room for the reference and the panel has room for the title, which is what says
    // which task it is without going to look it up.
    expect(host.querySelector(".plate-peek__task")?.textContent)
      .toBe("AMB-T-5252 ペインのヘッダを描く");
  });

  /// A spot taken off the picture while its run walks on leaves the step with nothing to be inside
  /// of. The row says the step alone rather than a name the picture no longer holds.
  it("says the step alone where the spot it was opened from has gone", () => {
    const host = document.createElement("div");
    const draw = mountNameplate(host);

    draw({ name: "/work/a", dot: STILL, run: { ...RUN, action: null } });

    expect(host.querySelector(".plate-run__step")?.textContent).toBe("取る");
  });

  it("says nothing of a run on a pane that is not one", () => {
    const host = document.createElement("div");
    const draw = mountNameplate(host);

    draw({ name: "the migration", dot: STILL, run: null });

    expect((host.querySelector(".plate-run") as HTMLElement).hidden).toBe(true);
    expect((host.querySelector(".plate__auto") as HTMLElement).hidden).toBe(true);
    // The panel is the name's, exactly as it was: an empty line in it would push it taller for a
    // pane that has no run.
    expect(host.querySelector(".plate-peek")?.textContent).toBe("the migration");
  });

  it("drops the run's line when the pane goes back to having no row", () => {
    const host = document.createElement("div");
    const draw = mountNameplate(host);

    draw({ name: "/work/a", dot: STILL, run: RUN });
    draw(null);

    expect((host.querySelector(".plate-run") as HTMLElement).hidden).toBe(true);
    expect((host.querySelector(".plate-peek") as HTMLElement).hidden).toBe(true);
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
