import { describe, expect, it } from "vitest";
import type { SessionSaidDto } from "../bindings/bindings";
import { closed, NO_SESSIONS, opened, said, seen, unsent } from "./sessions";

const AT = "2026-08-24T09:00:00Z";

function statement(over: Partial<SessionSaidDto> & Pick<SessionSaidDto, "verb">): SessionSaidDto {
  return { session: "pane-1", at: AT, ...over };
}

describe("the sessions the window is running", () => {
  it("knows exactly what it started, and nothing more", () => {
    const one = opened(NO_SESSIONS, { session: "pane-1", startedAt: AT, folder: "/work/a", agent: "claude-code" })
      .get("pane-1");
    expect(one).toMatchObject({ folder: "/work/a", agent: "claude-code", startedAt: AT });
    // The rest is not guessed at — it waits to be said.
    expect(one).toMatchObject({ note: null, waiting: null, seen: null, project: null });
    expect(one).not.toHaveProperty("confidence");
  });

  it("takes a turn from the agent and gives it back when the agent goes back to work", () => {
    let map = opened(NO_SESSIONS, { session: "pane-1", startedAt: AT });
    map = said(map, statement({ verb: "waiting", text: "the migration needs a decision" }));
    expect(map.get("pane-1")?.waiting).toBe("the migration needs a decision");

    map = said(map, statement({ verb: "note", text: "reading the store" }));
    expect(map.get("pane-1")).toMatchObject({ waiting: null, note: "reading the store" });

    map = said(map, statement({ verb: "waiting", text: "which of the two" }));
    map = said(map, statement({ verb: "finished", text: "it landed" }));
    expect(map.get("pane-1")).toMatchObject({ waiting: null, note: "it landed" });
  });

  it("follows the agent's folder, and hears about a session it has not been told of", () => {
    // The host emits a statement the moment it is written, which can be before the pane that opened the
    // terminal has finished registering it. Dropping it would lose the first thing the agent said.
    let map = said(NO_SESSIONS, statement({ verb: "note", text: "started", cwd: "/work/a" }));
    expect(map.get("pane-1")).toMatchObject({ folder: "/work/a", startedAt: AT, note: "started" });

    // `name` moves the folder and nothing else: it says where the agent is, not what it is doing.
    map = said(map, statement({ verb: "name", text: "the top fix", cwd: "/work/b" }));
    expect(map.get("pane-1")).toMatchObject({ folder: "/work/b", note: "started" });
  });

  it("puts the question of having looked back when a new turn comes", () => {
    let map = opened(NO_SESSIONS, { session: "pane-1", startedAt: AT });
    map = said(map, statement({ verb: "waiting", text: "a decision" }));
    map = seen(map, "pane-1", "2026-08-24T09:05:00Z");
    expect(map.get("pane-1")?.seen).toBe("2026-08-24T09:05:00Z");

    map = said(map, statement({ verb: "waiting", text: "another decision" }));
    expect(map.get("pane-1")?.seen).toBeNull();
  });

  it("holds a turn against the pane that said it, and lets it go when that pane goes back to work", () => {
    let map = opened(NO_SESSIONS, { session: "pane-1", startedAt: AT });
    map = opened(map, { session: "pane-2", startedAt: AT });
    const waiting = () => [...map.values()].filter((one) => one.waiting !== null).length;
    expect(waiting()).toBe(0);

    // A pane hard at work says a great deal. None of it is a turn.
    map = said(map, statement({ verb: "note", text: "running the tests" }));
    expect(waiting()).toBe(0);

    map = said(map, statement({ session: "pane-2", verb: "waiting", text: "which of the two" }));
    expect(map.get("pane-2")?.waiting).toBe("which of the two");
    // The other pane going back to work does not answer pane-2's turn.
    map = said(map, statement({ verb: "finished", text: "green" }));
    expect(map.get("pane-2")?.waiting).toBe("which of the two");

    map = said(map, statement({ session: "pane-2", verb: "note", text: "on it" }));
    expect(waiting()).toBe(0);
  });

  it("holds the sentence left in the input box, and lets it go the moment the pane speaks", () => {
    let map = opened(NO_SESSIONS, { session: "pane-1", startedAt: AT });
    expect(map.get("pane-1")?.unsent).toBe(false);
    map = unsent(map, "pane-1");
    expect(map.get("pane-1")?.unsent).toBe(true);

    // Every verb of this layer is Amenbo's own command, run in this pane. An agent that says a word
    // of it has plainly been told where it is working — including the one that only names the frame.
    for (const verb of ["name", "note", "waiting", "finished"] as const) {
      const spoke = said(unsent(map, "pane-1"), statement({ verb, text: "anything" }));
      expect(spoke.get("pane-1")?.unsent).toBe(false);
    }
  });

  it("says nothing about a pane it is not holding", () => {
    // The hand-over gives up only after a minute of looking at the pane, so this cannot be the first
    // thing heard about a session — an id nobody opened is one nothing is recorded for.
    const map = unsent(NO_SESSIONS, "pane-9");
    expect(map.size).toBe(0);
  });

  it("keeps nothing of a session whose terminal has closed", () => {
    let map = opened(NO_SESSIONS, { session: "pane-1", startedAt: AT });
    map = said(map, statement({ verb: "waiting", text: "a decision" }));
    map = closed(map, "pane-1");
    expect(map.size).toBe(0);
    // Closing what is already gone changes nothing — the same map comes back.
    expect(closed(map, "pane-1")).toBe(map);
  });
});
