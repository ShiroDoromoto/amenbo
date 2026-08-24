// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import {
  FINISHED_HOLD_MS,
  mountNameplate,
  NO_CHANGEOVER,
  nowOf,
  nowText,
  sayOf,
  sayText,
  type Dot,
  type Held, standsAsTurn,
} from "./nameplate";
import { NO_SESSIONS, opened, said, type Sessions } from "./sessions";

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
    // The breakdown is one hover away rather than stacked on a row that has one line.
    expect(nowText(now([held(), held({ ref: "#3599" })]), EN)).toMatchObject({
      text: "2 tasks",
      title: "#3598\n#3599",
    });
  });

  it("marks a task that has stopped, and never as a turn being handed over", () => {
    const stopped = nowOf([held({ status: "blocked" })], 0, NO_CHANGEOVER, 0).now;
    expect(stopped).toMatchObject({ kind: "one", stopped: true });
    // ⏸ is the agent saying a person's turn has come. `blocked` is not that, and must not borrow it
    // (`AMB-D-748`).
    expect(nowText(stopped, EN).mark).not.toBe("⏸");
    expect(nowText(stopped, EN).mark).toBe("⏹");
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
    expect(sayText(say, EN).mark).toBe("⏸");
  });

  it("says a premise has broken before it repeats what the agent was doing", () => {
    const sessions = sessionWith("note", "reading the store");
    expect(sayOf([held({ ready: false })], sessions.get("pane-1"))).toEqual({ kind: "premise" });
    expect(sayOf([held()], sessions.get("pane-1"))).toEqual({ kind: "note", text: "reading the store" });
  });

  it("says nothing where nothing was said", () => {
    // Silence is silence. It is not a claim that nothing needs a hand (`AMB-D-748`).
    expect(sayOf([], undefined)).toEqual({ kind: "silent" });
    expect(sayText({ kind: "silent" }, EN)).toEqual({ mark: "", text: "", title: "" });
  });

  it("keeps the whole of what was said for a reader who asks, since the row may not have shown it", () => {
    // The row gives this place what is left of one line, and a narrow pane leaves it nothing at all
    // (`../styles/global.css`) — so the words are kept where a hover can reach them, the way the
    // breakdown of several reservations is.
    const reason = "which of the two, and the second one moves the store";
    expect(sayText({ kind: "waiting", text: reason }, EN).title).toBe(reason);
    expect(sayText({ kind: "note", text: "reading the store" }, EN).title).toBe("reading the store");
    // Nothing the agent said, nothing to hold back: these two are the row's own words.
    expect(sayText({ kind: "premise" }, EN).title).toBe("");
    expect(sayText({ kind: "quiet", minutes: 7 }, EN).title).toBe("");
  });
});

/** The mark in front of the name, at rest. These cases are about the words on the row; what the dot
 *  does has its own (`./moving`). */
const STILL: Dot = { frame: "1", moving: false };

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
    // The mark keeps the whole of it too: where the pane is narrow the words are not drawn, and the
    // mark is what is left to ask.
    expect(host.querySelector(".plate__mark--say")?.getAttribute("title")).toBe("which of the two");
    expect(host.querySelector(".plate__say")?.getAttribute("title")).toBe("which of the two");
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
    // The two a person is needed for: the agent handing a turn over, and the ledger saying what the
    // pane holds is no longer ready.
    expect(standsAsTurn({ kind: "waiting", text: "which of the two" })).toBe(true);
    expect(standsAsTurn({ kind: "premise" })).toBe(true);
    // The two that are not. Silence least of all: it is not a claim about anything (`AMB-D-748`).
    expect(standsAsTurn({ kind: "note", text: "running the tests" })).toBe(false);
    expect(standsAsTurn({ kind: "silent" })).toBe(false);
  });
});
