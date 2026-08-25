// Which of the paths a pane draws can be opened in the face beside it, which is the whole of what
// this module decides.
//
// A path is written by whatever ran in the pane, so it arrives in every shape a path comes in.
// Getting this wrong shows up as a click that opens a file the face cannot answer for — the thing
// `AMB-D-747` is about.
import { describe, expect, it } from "vitest";
import { fileUnder } from "./fileUnder";

const ROOT = "/work/repo";

describe("a path drawn in a pane", () => {
  it("is read against the folder the pane was in", () => {
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
});
