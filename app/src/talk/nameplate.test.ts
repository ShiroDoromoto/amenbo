// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import {
  faceOf,
  FINISHED_HOLD_MS,
  mountNameplate,
  NO_CHANGEOVER,
  nowOf,
  middleElide,
  nowText,
  peekLines,
  sayOf,
  sayText,
  type Dot,
  type Held, standsAsTurn,
} from "./nameplate";
import { statusLabel } from "../core/i18n";
import { NO_SESSIONS, opened, said, unsent, type Sessions } from "./sessions";

const AT = "2026-08-24T09:00:00Z";

function held(over: Partial<Held> = {}): Held {
  return { ref: "#3598", title: "the nameplate", status: "in_progress", ready: true, ...over };
}

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

describe("what the middle of the row says", () => {
  it("names the one task, counts the rest, and says so when there is none", () => {
    const now = (h: Held[]) => nowOf(h, 0, NO_CHANGEOVER, 0).now;
    expect(now([])).toEqual({ kind: "idle" });
    expect(now([held()])).toMatchObject({ kind: "one", ref: "#3598", stopped: false });
    expect(now([held(), held({ ref: "#3599" })])).toMatchObject({ kind: "many", count: 2 });
    // The row counts them; which ones they are is in the panel a hover drops under it.
    expect(nowText(now([held(), held({ ref: "#3599" })]), EN)).toMatchObject({ text: "2 tasks" });
  });

  it("marks a task that has stopped, and never as a turn being handed over", () => {
    const stopped = nowOf([held({ status: "blocked" })], 0, NO_CHANGEOVER, 0).now;
    expect(stopped).toMatchObject({ kind: "one", stopped: true });
    // The pause is the agent saying a person's turn has come. `blocked` is not that, and must not
    // borrow it (`AMB-D-748`).
    expect(nowText(stopped, EN).mark).not.toBe("pause");
    expect(nowText(stopped, EN).mark).toBe("stop");
  });

  it("shows an ending for a moment and then stops showing it", () => {
    const one = [held()];
    let state = nowOf(one, 0, NO_CHANGEOVER, 0);
    expect(state.now.kind).toBe("one");


    state = nowOf([], 1, state.changeover, 1_000);
    expect(state.now).toEqual({ kind: "finished", count: 1 });

    state = nowOf([], 1, state.changeover, 1_000 + FINISHED_HOLD_MS - 1);
    expect(state.now.kind).toBe("finished");

    state = nowOf([], 1, state.changeover, 1_000 + FINISHED_HOLD_MS);
    expect(state.now).toEqual({ kind: "idle" });
  });

  it("does not announce work that was already over when the pane came up", () => {
    // Nothing was looked away from, so there is nothing to catch up on — a pane must not open by
    // reporting an ending from before it existed.
    const first = nowOf([], 3, NO_CHANGEOVER, 0);
    expect(first.now).toEqual({ kind: "idle" });
    // The one after it is a real ending, even though the pane opened on a tally that was not zero.
    expect(nowOf([], 4, first.changeover, 1_000).now).toEqual({ kind: "finished", count: 4 });
  });

  it("drops the ending as soon as the session reserves something again", () => {
    let state = nowOf([], 0, NO_CHANGEOVER, 0);
    state = nowOf([], 1, state.changeover, 0);
    expect(state.now.kind).toBe("finished");
    state = nowOf([held()], 1, state.changeover, 100);
    expect(state.now.kind).toBe("one");
    state = nowOf([], 1, state.changeover, 200);
    expect(state.now).toEqual({ kind: "idle" });
  });
});

describe("the one thing said on the right", () => {
  it("puts a turn being handed over above everything else", () => {
    const sessions = said(sessionWith("note", "reading the store"), {
      session: "pane-1",
      at: AT,
      verb: "waiting",
      text: "which of the two",
    });
    const say = sayOf([held({ ready: false })], sessions.get("pane-1"));
    expect(say).toEqual({ kind: "waiting", text: "which of the two" });
    expect(sayText(say, EN).mark).toBe("pause");
  });

  it("says a premise has broken before it repeats what the agent was doing", () => {
    const sessions = sessionWith("note", "reading the store");
    expect(sayOf([held({ ready: false })], sessions.get("pane-1"))).toEqual({ kind: "premise" });
    expect(sayOf([held()], sessions.get("pane-1"))).toEqual({ kind: "note", text: "reading the store" });
  });

  it("says the opening sentence is unsent, under everything the pane derived or declared", () => {
    const left = unsent(opened(NO_SESSIONS, { session: "pane-1", startedAt: AT }), "pane-1");
    // The pane it is news in: nothing else has been said about this session at all.
    expect(sayOf([], left.get("pane-1"))).toEqual({ kind: "unsent" });
    // A premise the ledger derived is a live fact; the sentence never going in is an old one.
    expect(sayOf([held({ ready: false })], left.get("pane-1"))).toEqual({ kind: "premise" });
    // The mark is the pause: what is left where the row is too narrow for words is "somebody is
    // needed here", which is as true of this as of a turn handed over.
    expect(sayText({ kind: "unsent" }, EN).mark).toBe("pause");
  });

  it("says nothing where nothing was said", () => {
    // Silence is silence. It is not a claim that nothing needs a hand (`AMB-D-748`).
    expect(sayOf([], undefined)).toEqual({ kind: "silent" });
    expect(sayText({ kind: "silent" }, EN)).toEqual({ mark: null, text: "" });
  });
});

/** The lamp in front of the name, at rest. These cases are about the words on the row; what the lamp
 *  does has its own (`./plateMoving.test`). */
const STILL: Dot = { frame: "1", face: "out" };

describe("the whole row, for a reader who asks for it", () => {
  it("says every place in full, in the order the row reads", () => {
    const reason = "which of the two, and the second one moves the store";
    expect(
      peekLines(
        {
          name: "the migration",
          now: { kind: "one", ref: "#3598", title: "the nameplate", stopped: false },
          say: { kind: "waiting", text: reason },
          dot: STILL,
        },
        EN,
      ),
    ).toEqual(["#3598 the nameplate", reason]);
  });

  it("gives the two places that hold more than a line what a mark used to carry", () => {
    // A task that has stopped says so in words, beside the ref it stopped on.
    expect(
      peekLines(
        {
          name: null,
          now: { kind: "one", ref: "#3598", title: "the nameplate", stopped: true },
          say: { kind: "silent" },
          dot: STILL,
        },
        EN,
      ),
    ).toEqual([`#3598 the nameplate — ${statusLabel("blocked", EN)}`]);
    // And several reservations are listed rather than counted.
    expect(
      peekLines(
        {
          name: null,
          now: { kind: "many", count: 2, refs: ["#3598", "#3599"], stopped: 0 },
          say: { kind: "silent" },
          dot: STILL,
        },
        EN,
      ),
    ).toEqual(["2 tasks", "#3598", "#3599"]);
  });

  it("has no line for a place with nothing in it", () => {
    // Silence contributes nothing, so a pane that is only idle has the one line the row does.
    expect(
      peekLines({ name: null, now: { kind: "idle" }, say: { kind: "silent" }, dot: STILL }, EN),
    ).toEqual(["Talking it over"]);
  });
});

describe("a line too long for the place it is drawn in", () => {
  /** Fits by character count — the row is not monospaced, but a test bench can be. */
  const upTo = (columns: number) => (candidate: string) => candidate.length <= columns;

  it("is left alone where it fits", () => {
    expect(middleElide("#3598 the nameplate", upTo(40))).toBe("#3598 the nameplate");
  });

  it("keeps both ends and drops the middle, so two panes on one road read apart", () => {
    const a = "#4423 walk read-a-file-with-the-tree from step 09";
    const b = "#4423 walk read-a-file-with-the-tree from step 15";
    const [cutA, cutB] = [middleElide(a, upTo(24)), middleElide(b, upTo(24))];
    expect(cutA.length).toBeLessThanOrEqual(24);
    expect(cutA.startsWith("#4423 ")).toBe(true);
    expect(cutA.endsWith("09")).toBe(true);
    expect(cutB.endsWith("15")).toBe(true);
    // The whole of the point: cut at the end, both of these would be the same line.
    expect(cutA).not.toBe(cutB);
    expect(a.slice(0, 24)).toBe(b.slice(0, 24));
  });

  it("gives the head a little over half, since the ref it carries has to come out whole", () => {
    // Ten columns for the line, one of them the mark that says it was cut: five before it, four after.
    expect(middleElide("0123456789abcdef", upTo(10))).toBe("01234…cdef");
  });

  it("comes down to the mark alone where there is room for nothing else", () => {
    expect(middleElide("#3598 the nameplate", upTo(1))).toBe("…");
  });
});

describe("the row on the page", () => {
  it("is one line, and redraws in place", () => {
    const host = document.createElement("div");
    const draw = mountNameplate(host);

    draw({ name: "the migration", now: { kind: "idle" }, say: { kind: "silent" }, dot: STILL }, EN);
    expect(host.querySelector(".plate__name")?.textContent).toBe("the migration");
    expect(host.querySelector(".plate__now")?.textContent).toBe("Talking it over");

    const row = host.querySelector(".plate");
    draw(
      {
        name: "the migration",
        now: { kind: "one", ref: "#3598", title: "the nameplate", stopped: false },
        dot: STILL, say: { kind: "waiting", text: "which of the two" },
      },
      EN,
    );
    expect(host.querySelectorAll(".plate")).toHaveLength(1);
    // Redrawn in place: the row is not rebuilt, so nothing under the pointer moves.
    expect(host.querySelector(".plate")).toBe(row);
    expect(host.querySelector(".plate__now")?.textContent).toBe("#3598 the nameplate");
    expect(host.querySelector(".plate__say")?.textContent).toBe("which of the two");
    expect(row?.getAttribute("data-say")).toBe("waiting");
    // No place on the row carries a tooltip of its own: the whole of it is in the panel instead, and
    // the machine's own tooltip over that panel would be the same sentence twice.
    expect(host.querySelector(".plate__mark--say")?.getAttribute("title")).toBeNull();
    expect(host.querySelector(".plate__say")?.getAttribute("title")).toBeNull();
    expect(host.querySelector(".plate-peek__name")?.textContent).toBe("the migration");
    expect(host.querySelector(".plate-peek")?.textContent)
      .toContain("#3598 the nameplate\nwhich of the two");
  });

  it("comes down when there is nothing to label, and comes back with the same row", () => {
    const host = document.createElement("div");
    const draw = mountNameplate(host);

    draw(null, EN);
    const row = host.querySelector(".plate");
    expect(row, "the row was removed rather than hidden").toBeTruthy();
    expect((row as HTMLElement).hidden, "a pane with no session was labelled anyway").toBe(true);

    draw({ name: null, now: { kind: "idle" }, say: { kind: "silent" }, dot: STILL }, EN);
    expect(host.querySelector(".plate")).toBe(row);
    expect((row as HTMLElement).hidden).toBe(false);
  });
});

describe("what counts as a turn standing", () => {
  it("is what the row leads with when a person is needed, and nothing else", () => {
    // The three a person is needed for: the agent handing a turn over, the ledger saying what the
    // pane holds is no longer ready, and the opening sentence still sitting in the input box.
    expect(standsAsTurn({ kind: "waiting", text: "which of the two" })).toBe(true);
    expect(standsAsTurn({ kind: "premise" })).toBe(true);
    // Nothing at all happens in a pane whose opening sentence is still sitting in its input box, and
    // one keypress is the whole of what it is waiting for.
    expect(standsAsTurn({ kind: "unsent" })).toBe(true);
    // The two that are not. Silence least of all: it is not a claim about anything (`AMB-D-748`).
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
    expect(faceOf({ kind: "premise" }, false)).toBe("calling");
    expect(faceOf({ kind: "unsent" }, false)).toBe("calling");
  });

  it("puts the turn over the stream, because that is the one to act on", () => {
    // A blocker opening on a task the pane holds while its build prints away: both are true, and the
    // lamp says the one a person is meant to do something about.
    expect(faceOf({ kind: "premise" }, true)).toBe("calling");
  });
});
