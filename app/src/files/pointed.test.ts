// What "pointed at" can and cannot be opened, which is the whole of what this module decides.
//
// A target is written by an agent typing at a shell, so it arrives in every shape a path comes in,
// and it may not be a path at all. Getting this wrong shows up as a row that looks like a link and
// opens nothing — the thing `AMB-D-747` is about.
import { describe, expect, it } from "vitest";
import type { SessionSaidDto } from "../bindings/bindings";
import { fileUnder, isRef, isUrl, markRead, tookPoint, unread } from "./pointed";

const ROOT = "/work/repo";

function said(over: Partial<SessionSaidDto> & Pick<SessionSaidDto, "verb">): SessionSaidDto {
  return { session: "s1", at: "2026-08-23T10:00:00Z", ...over };
}

describe("a path an agent typed", () => {
  it("is read against the folder the agent was in", () => {
    expect(fileUnder(ROOT, "/work/repo/src", "main.rs")).toEqual(["src", "main.rs"]);
    expect(fileUnder(ROOT, "/work/repo/src", "./main.rs")).toEqual(["src", "main.rs"]);
    // No folder said: the root is the only other thing it could be read against.
    expect(fileUnder(ROOT, null, "notes/a.md")).toEqual(["notes", "a.md"]);
  });

  it("is taken as written when it is absolute", () => {
    expect(fileUnder(ROOT, "/somewhere/else", "/work/repo/notes/a.md")).toEqual(["notes", "a.md"]);
  });

  it("resolves the way up before judging where it lands", () => {
    expect(fileUnder(ROOT, "/work/repo/src", "../notes/a.md")).toEqual(["notes", "a.md"]);
    // A detour that comes back inside has landed inside: where it lands is the question, not how it
    // was spelled. What is sent to the host is the resolved path, which carries no way up at all.
    expect(fileUnder(ROOT, "/work/repo", "../repo/notes/a.md")).toEqual(["notes", "a.md"]);
  });

  it("is nothing at all when it lands outside the folder", () => {
    for (const target of ["../secret.txt", "/etc/passwd", "../../elsewhere/a.md"]) {
      expect(fileUnder(ROOT, "/work/repo", target), target).toBeNull();
    }
    // The folder itself is not a file, and neither is a sibling that merely starts the same way.
    expect(fileUnder(ROOT, null, "/work/repo")).toBeNull();
    expect(fileUnder(ROOT, null, "/work/repo-2/a.md")).toBeNull();
  });

  it("is not a path when it names a record or somewhere on the web", () => {
    expect(fileUnder(ROOT, null, "AMB-T-12")).toBeNull();
    expect(fileUnder(ROOT, null, "https://example.com/x")).toBeNull();
    expect(isRef("AMB-D-748")).toBe(true);
    expect(isRef("AMB-X-1")).toBe(false);
    expect(isUrl("https://example.com")).toBe(true);
    expect(isUrl("notes/a.md")).toBe(false);
  });
});

describe("what a session pointed at", () => {
  it("keeps the newest first, per session", () => {
    let held = tookPoint(new Map(), said({ verb: "point", target: "a.md", why: "first", at: "1" }));
    held = tookPoint(held, said({ verb: "point", target: "b.md", why: "second", at: "2" }));
    held = tookPoint(held, said({ verb: "point", session: "s2", target: "c.md", why: "other", at: "3" }));
    expect(held.get("s1")?.map((one) => one.target)).toEqual(["b.md", "a.md"]);
    expect(held.get("s2")?.map((one) => one.target)).toEqual(["c.md"]);
  });

  it("takes nothing from the other verbs, or from a point with nothing to point at", () => {
    let held = tookPoint(new Map(), said({ verb: "note", text: "working" }));
    held = tookPoint(held, said({ verb: "waiting", text: "your turn" }));
    held = tookPoint(held, said({ verb: "point", why: "no target" }));
    expect(held.size).toBe(0);
  });

  it("counts what nobody opened, and stops counting one that was", () => {
    let held = tookPoint(new Map(), said({ verb: "point", target: "a.md", at: "1" }));
    held = tookPoint(held, said({ verb: "point", target: "b.md", at: "2" }));
    expect(unread(held.get("s1")!)).toBe(2);
    held = markRead(held, "s1", "1");
    expect(unread(held.get("s1")!)).toBe(1);
    // A row that was opened stays on the list: what was pointed at was pointed at.
    expect(held.get("s1")).toHaveLength(2);
  });
});
