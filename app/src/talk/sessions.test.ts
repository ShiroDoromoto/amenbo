import { describe, expect, it } from "vitest";
import type { SessionSaidDto } from "../bindings/bindings";
import { closed, NO_SESSIONS, opened, said, sent, unsent } from "./sessions";

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
    expect(one).toMatchObject({ project: null });
    expect(one).not.toHaveProperty("confidence");
  });

  it("follows the agent's folder, and hears about a session it has not been told of", () => {
    // The host emits a statement the moment it is written, which can be before the pane that opened the
    // terminal has finished registering it. Dropping it would lose where the agent said it was.
    let map = said(NO_SESSIONS, statement({ verb: "briefed", cwd: "/work/a" }));
    expect(map.get("pane-1")).toMatchObject({ folder: "/work/a", startedAt: AT });

    // `name` moves the folder too: it says where the agent is, and so does every other word.
    map = said(map, statement({ verb: "name", text: "the top fix", cwd: "/work/b" }));
    expect(map.get("pane-1")).toMatchObject({ folder: "/work/b" });
  });

  it("holds the sentence left in the input box, and lets it go the moment the pane speaks", () => {
    let map = opened(NO_SESSIONS, { session: "pane-1", startedAt: AT });
    expect(map.get("pane-1")?.unsent).toBe(false);
    map = unsent(map, "pane-1");
    expect(map.get("pane-1")?.unsent).toBe(true);

    // Every verb of this layer is Amenbo's own command, run in this pane. An agent that says a word
    // of it has plainly been told where it is working — including the one that only names the frame.
    const spoke = said(unsent(map, "pane-1"), statement({ verb: "name", text: "anything" }));
    expect(spoke.get("pane-1")?.unsent).toBe(false);
  });

  it("lets the sentence go when it is sent, without waiting for a word that may never come", () => {
    // The notice is about the input box, not about the agent. A pane can send the sentence to a
    // program that never runs Amenbo's command — and a notice waiting on that word would stand for
    // the life of the pane, pointing the reader at a keypress that does nothing.
    let map = unsent(opened(NO_SESSIONS, { session: "pane-1", startedAt: AT }), "pane-1");
    map = sent(map, "pane-1");
    expect(map.get("pane-1")?.unsent).toBe(false);
    // And nothing else about the session moves: this is one field's news.
    expect(map.get("pane-1")).toMatchObject({ startedAt: AT, folder: null });
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
    map = unsent(map, "pane-1");
    map = closed(map, "pane-1");
    expect(map.size).toBe(0);
    // Closing what is already gone changes nothing — the same map comes back.
    expect(closed(map, "pane-1")).toBe(map);
  });
});
