// What "pointed at" can and cannot be opened, which is the whole of what this module decides.
//
// A target is written by an agent typing at a shell, so it arrives in every shape a path comes in,
// and it may not be a path at all. Getting this wrong shows up as a row that looks like a link and
// opens nothing — the thing `AMB-D-747` is about.
import { describe, expect, it } from "vitest";
import type { SessionSaidDto } from "../bindings/bindings";
import {
  fileUnder, isRef, isUrl, markRead, newestPoint, pointWaits, tookPoint, tookShown, unread,
} from "./pointed";

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

describe("whether the files half has something to call about", () => {
  const two = () => {
    let held = tookPoint(new Map(), said({ verb: "point", target: "a.md", at: "1" }));
    held = tookPoint(held, said({ verb: "point", target: "b.md", at: "2" }));
    return held.get("s1")!;
  };

  it("says nothing where the session pointed at nothing", () => {
    expect(newestPoint([])).toBeNull();
    expect(pointWaits(new Map(), "s1", null)).toBe(false);
  });

  it("waits from the first thing pointed at until the half has been seen", () => {
    const points = two();
    expect(newestPoint(points)).toBe("2");
    expect(pointWaits(new Map(), "s1", "2")).toBe(true);
    const shown = tookShown(new Map(), "s1", "2");
    expect(pointWaits(shown, "s1", "2")).toBe(false);
  });

  it("stays quiet once seen, however often the panel is closed and opened", () => {
    const shown = tookShown(new Map(), "s1", "2");
    // Nothing has been pointed at since, so nothing calls — the badge answers "something came up
    // while you were away", not "something is still over there".
    expect(pointWaits(shown, "s1", "2")).toBe(false);
    // The same answer taken twice is the same map, so nothing downstream re-renders on it.
    expect(tookShown(shown, "s1", "2")).toBe(shown);
  });

  it("calls again the moment something new is pointed at", () => {
    let held = tookPoint(new Map(), said({ verb: "point", target: "a.md", at: "1" }));
    const shown = tookShown(new Map(), "s1", newestPoint(held.get("s1")!)!);
    held = tookPoint(held, said({ verb: "point", target: "b.md", at: "2" }));
    expect(pointWaits(shown, "s1", newestPoint(held.get("s1")!))).toBe(true);
  });

  it("is answered per session, so one pane's being read says nothing about another's", () => {
    const shown = tookShown(new Map(), "s1", "2");
    expect(pointWaits(shown, "s2", "3")).toBe(true);
  });

  it("does not count opening a row as having been shown the half", () => {
    // `read` is one row somebody clicked; this is the person having been on the half at all.
    let held = tookPoint(new Map(), said({ verb: "point", target: "a.md", at: "1" }));
    held = markRead(held, "s1", "1");
    expect(pointWaits(new Map(), "s1", newestPoint(held.get("s1")!))).toBe(true);
  });
});
