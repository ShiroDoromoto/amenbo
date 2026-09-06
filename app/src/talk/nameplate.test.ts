// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import {
  faceOf,
  mountNameplate,
  peekLines,
  sayOf,
  sayText,
  type Dot,
  standsAsTurn,
} from "./nameplate";
import { NO_SESSIONS, opened, said, unsent, type Sessions } from "./sessions";

const AT = "2026-08-24T09:00:00Z";

/** A session with one statement made in it. */
function sessionWith(verb: "note" | "waiting", text: string): Sessions {
  return said(opened(NO_SESSIONS, { session: "pane-1", startedAt: AT }), {
    session: "pane-1",
    at: AT,
    verb,
    text,
  });
}

/** The language everything here is read in. The window is handed one rather than guessing at it. */
const EN = "en" as const;

describe("the one thing said on the right", () => {
  it("puts a turn being handed over above everything else", () => {
    const sessions = said(sessionWith("note", "reading the store"), {
      session: "pane-1",
      at: AT,
      verb: "waiting",
      text: "which of the two",
    });
    const say = sayOf(sessions.get("pane-1"));
    expect(say).toEqual({ kind: "waiting", text: "which of the two" });
    expect(sayText(say, EN).mark).toBe("pause");
  });

  it("repeats what the agent was doing where it said nothing better", () => {
    const sessions = sessionWith("note", "reading the store");
    expect(sayOf(sessions.get("pane-1"))).toEqual({ kind: "note", text: "reading the store" });
  });

  it("says the opening sentence is unsent, under the turn the agent handed over", () => {
    const left = unsent(opened(NO_SESSIONS, { session: "pane-1", startedAt: AT }), "pane-1");
    // The pane it is news in: nothing else has been said about this session at all.
    expect(sayOf(left.get("pane-1"))).toEqual({ kind: "unsent" });
    // A turn handed over is the agent saying so now; the sentence never going in is an old fact.
    const handed = said(left, { session: "pane-1", at: AT, verb: "waiting", text: "which of the two" });
    expect(sayOf(handed.get("pane-1"))).toEqual({ kind: "waiting", text: "which of the two" });
    // The mark is the pause: what is left where the row is too narrow for words is "somebody is
    // needed here", which is as true of this as of a turn handed over.
    expect(sayText({ kind: "unsent" }, EN).mark).toBe("pause");
  });

  it("says nothing where nothing was said", () => {
    // Silence is silence. It is not a claim that nothing needs a hand (`AMB-D-858`).
    expect(sayOf(undefined)).toEqual({ kind: "silent" });
    expect(sayText({ kind: "silent" }, EN)).toEqual({ mark: null, text: "" });
  });
});

/** The lamp in front of the name, at rest. These cases are about the words on the row; what the lamp
 *  does has its own (`./plateMoving.test`). */
const STILL: Dot = { frame: "1", face: "out" };

describe("the whole row, for a reader who asks for it", () => {
  it("says the place in full, where the row had to cut it", () => {
    const reason = "which of the two, and the second one moves the store";
    expect(
      peekLines(
        { name: "the migration", say: { kind: "waiting", text: reason }, dot: STILL },
        EN,
      ),
    ).toEqual([reason]);
  });

  it("has no line for a place with nothing in it", () => {
    // Silence contributes nothing, and the name is drawn on its own line of the panel.
    expect(peekLines({ name: null, say: { kind: "silent" }, dot: STILL }, EN)).toEqual([]);
  });
});

describe("the row on the page", () => {
  it("is one line, and redraws in place", () => {
    const host = document.createElement("div");
    const draw = mountNameplate(host);

    draw({ name: "the migration", say: { kind: "silent" }, dot: STILL }, EN);
    expect(host.querySelector(".plate__name")?.textContent).toBe("the migration");
    expect(host.querySelector(".plate__say")?.textContent).toBe("");

    const row = host.querySelector(".plate");
    draw(
      { name: "the migration", dot: STILL, say: { kind: "waiting", text: "which of the two" } },
      EN,
    );
    expect(host.querySelectorAll(".plate")).toHaveLength(1);
    // Redrawn in place: the row is not rebuilt, so nothing under the pointer moves.
    expect(host.querySelector(".plate")).toBe(row);
    expect(host.querySelector(".plate__say")?.textContent).toBe("which of the two");
    expect(row?.getAttribute("data-say")).toBe("waiting");
    // No place on the row carries a tooltip of its own: the whole of it is in the panel instead, and
    // the machine's own tooltip over that panel would be the same sentence twice.
    expect(host.querySelector(".plate__mark--say")?.getAttribute("title")).toBeNull();
    expect(host.querySelector(".plate__say")?.getAttribute("title")).toBeNull();
    expect(host.querySelector(".plate-peek__name")?.textContent).toBe("the migration");
    expect(host.querySelector(".plate-peek")?.textContent).toContain("which of the two");
  });

  it("comes down when there is nothing to label, and comes back with the same row", () => {
    const host = document.createElement("div");
    const draw = mountNameplate(host);

    draw(null, EN);
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

    draw({ name: null, say: { kind: "silent" }, dot: STILL }, EN);
    expect(host.querySelector(".plate")).toBe(row);
    expect((row as HTMLElement).hidden).toBe(false);
  });
});

describe("what counts as a turn standing", () => {
  it("is what the row leads with when a person is needed, and nothing else", () => {
    // The two a person is needed for: the agent handing a turn over, and the opening sentence still
    // sitting in the input box.
    expect(standsAsTurn({ kind: "waiting", text: "which of the two" })).toBe(true);
    // Nothing at all happens in a pane whose opening sentence is still sitting in its input box, and
    // one keypress is the whole of what it is waiting for.
    expect(standsAsTurn({ kind: "unsent" })).toBe(true);
    // The two that are not. Silence least of all: it is not a claim about anything (`AMB-D-858`).
    expect(standsAsTurn({ kind: "note", text: "running the tests" })).toBe(false);
    expect(standsAsTurn({ kind: "silent" })).toBe(false);
  });
});

describe("which face the lamp is on", () => {
  it("is lit while output is arriving, and out when it is not", () => {
    expect(faceOf({ kind: "silent" }, true)).toBe("lit");
    expect(faceOf({ kind: "silent" }, false)).toBe("out");
  });

  it("does not light for what the agent merely said it was doing", () => {
    // A note is the session talking about its own work. Nothing is being asked of anybody.
    expect(faceOf({ kind: "note", text: "running the tests" }, false)).toBe("out");
    expect(faceOf({ kind: "quiet", minutes: 12 }, false)).toBe("out");
  });

  it("calls for both of the reasons the row leads with", () => {
    expect(faceOf({ kind: "waiting", text: "which of the two" }, false)).toBe("calling");
    expect(faceOf({ kind: "unsent" }, false)).toBe("calling");
  });

  it("puts the turn over the stream, because that is the one to act on", () => {
    // A turn handed over while the build prints away: both are true, and the lamp says the one a
    // person is meant to do something about.
    expect(faceOf({ kind: "waiting", text: "which of the two" }, true)).toBe("calling");
  });
});
