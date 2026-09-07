import { describe, expect, it } from "vitest";
import type { SessionSaidDto } from "../bindings/bindings";
import { closed, declared, NO_SESSIONS, opened, said, sent, unsent } from "./sessions";

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
    expect(one).toMatchObject({ waiting: null, project: null });
    expect(one).not.toHaveProperty("confidence");
  });

  it("moves nothing about a turn when the agent speaks, that being the host's answer", () => {
    // A turn goes up by a word and comes down by a person arriving, and the host writes both
    // (`AMB-D-859`, `AMB-D-860`). What a statement moves here is where the agent is working.
    let map = opened(NO_SESSIONS, { session: "pane-1", startedAt: AT });
    map = said(map, statement({ verb: "waiting", text: "the migration needs a decision" }));
    expect(map.get("pane-1")?.waiting).toBeNull();

    map = declared(map, "pane-1", "the migration needs a decision");
    expect(map.get("pane-1")?.waiting).toBe("the migration needs a decision");

    // The same answer twice is not news, and a watcher woken for it would redraw for nothing.
    expect(declared(map, "pane-1", "the migration needs a decision")).toBe(map);

    map = declared(map, "pane-1", null);
    expect(map.get("pane-1")?.waiting).toBeNull();
  });

  it("follows the agent's folder, and hears about a session it has not been told of", () => {
    // The host emits a statement the moment it is written, which can be before the pane that opened the
    // terminal has finished registering it. Dropping it would lose where the agent said it was.
    let map = said(NO_SESSIONS, statement({ verb: "waiting", text: "which of the two", cwd: "/work/a" }));
    expect(map.get("pane-1")).toMatchObject({ folder: "/work/a", startedAt: AT });

    // `name` moves the folder too: it says where the agent is, and so does every other word.
    map = said(map, statement({ verb: "name", text: "the top fix", cwd: "/work/b" }));
    expect(map.get("pane-1")).toMatchObject({ folder: "/work/b" });
  });

  it("passes over a word this build has never heard of", () => {
    // The vocabulary has shrunk before, and a CLI from before that still posts the old words into a
    // newer window's drop box (`AMB-D-859`). What arrives says where the agent is and nothing else.
    let map = opened(NO_SESSIONS, { session: "pane-1", startedAt: AT });
    map = said(map, statement({ verb: "note", text: "reading the store", cwd: "/work/a" }));
    expect(map.get("pane-1")).toMatchObject({ folder: "/work/a", waiting: null });
  });

  it("holds a turn against the pane it was handed over in, and against no other", () => {
    let map = opened(NO_SESSIONS, { session: "pane-1", startedAt: AT });
    map = opened(map, { session: "pane-2", startedAt: AT });
    const standing = () => [...map.values()].filter((one) => one.waiting !== null).length;
    expect(standing()).toBe(0);

    map = declared(map, "pane-2", "which of the two");
    expect(map.get("pane-2")?.waiting).toBe("which of the two");
    expect(standing()).toBe(1);

    // The other pane's turn coming down is not this one's coming down.
    map = declared(map, "pane-1", null);
    expect(standing()).toBe(1);
    map = declared(map, "pane-2", null);
    expect(standing()).toBe(0);
  });

  it("writes nothing down for a session it is not holding", () => {
    // The host answers for every session in the process, and this window may not be drawing them all.
    expect(declared(NO_SESSIONS, "pane-9", "which of the two").size).toBe(0);
  });

  it("holds the sentence left in the input box, and lets it go the moment the pane speaks", () => {
    let map = opened(NO_SESSIONS, { session: "pane-1", startedAt: AT });
    expect(map.get("pane-1")?.unsent).toBe(false);
    map = unsent(map, "pane-1");
    expect(map.get("pane-1")?.unsent).toBe(true);

    // Every verb of this layer is Amenbo's own command, run in this pane. An agent that says a word
    // of it has plainly been told where it is working — including the one that only names the frame.
    for (const verb of ["name", "waiting"] as const) {
      const spoke = said(unsent(map, "pane-1"), statement({ verb, text: "anything" }));
      expect(spoke.get("pane-1")?.unsent).toBe(false);
    }
  });

  it("lets the sentence go when it is sent, without waiting for a word that may never come", () => {
    // The notice is about the input box, not about the agent. A pane can send the sentence to a
    // program that never runs Amenbo's command — and a notice waiting on that word would stand for
    // the life of the pane, pointing the reader at a keypress that does nothing.
    let map = unsent(opened(NO_SESSIONS, { session: "pane-1", startedAt: AT }), "pane-1");
    map = sent(map, "pane-1");
    expect(map.get("pane-1")?.unsent).toBe(false);
    // And whose turn it is stays untouched: this is one field's news.
    expect(map.get("pane-1")).toMatchObject({ waiting: null });
  });

  it("says nothing about a pane it is not holding", () => {
    // The hand-over gives up only after a minute of looking at the pane, so this cannot be the first
    // thing heard about a session — an id nobody opened is one nothing is recorded for.
    const map = unsent(NO_SESSIONS, "pane-9");
    expect(map.size).toBe(0);
    expect(sent(NO_SESSIONS, "pane-9").size).toBe(0);
  });

  it("keeps nothing of a session whose terminal has closed", () => {
    let map = opened(NO_SESSIONS, { session: "pane-1", startedAt: AT });
    map = declared(map, "pane-1", "a decision");
    map = closed(map, "pane-1");
    expect(map.size).toBe(0);
    // Closing what is already gone changes nothing — the same map comes back.
    expect(closed(map, "pane-1")).toBe(map);
  });
});
