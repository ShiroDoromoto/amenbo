// Which of the paths a pane draws can be opened in the face beside it, which is the whole of what
// this module decides.
//
// A path is written by whatever ran in the pane, so it arrives in every shape a path comes in.
// Getting this wrong shows up as a click that opens a file the face cannot answer for — the thing
// `AMB-D-747` is about.
import { describe, expect, it } from "vitest";
import { fileAt, fileUnder, fileUnderAny } from "./fileUnder";

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

describe("a path drawn in a pane, against every folder the project is bound to", () => {
  const ROOTS = ["/work/repo", "/work/notes"];

  it("lands in whichever of them it is inside", () => {
    expect(fileUnderAny(ROOTS, null, "/work/notes/a.md"))
      .toEqual({ root: "/work/notes", path: ["a.md"] });
    expect(fileUnderAny(ROOTS, null, "/work/repo/src/main.rs"))
      .toEqual({ root: "/work/repo", path: ["src", "main.rs"] });
  });

  /** A bound folder can be inside another. Both accept the path, and the one with a row to draw for
   *  it is the inner one. */
  it("takes the innermost folder that accepts it", () => {
    const nested = ["/work/repo", "/work/repo/app"];
    expect(fileUnderAny(nested, null, "/work/repo/app/src/main.rs"))
      .toEqual({ root: "/work/repo/app", path: ["src", "main.rs"] });
  });

  it("is read against the folder the pane was in, and lands where that puts it", () => {
    expect(fileUnderAny(ROOTS, "/work/notes", "a.md"))
      .toEqual({ root: "/work/notes", path: ["a.md"] });
  });

  /** With no folder to read it against, a relative path names as many files as there are folders,
   *  and choosing one of them would be the face guessing. */
  it("opens nothing for a relative path with no folder to read it against", () => {
    expect(fileUnderAny(ROOTS, null, "a.md")).toBeNull();
    expect(fileUnderAny(ROOTS, null, "./a.md")).toBeNull();
  });

  it("opens nothing where it lands outside all of them", () => {
    expect(fileUnderAny(ROOTS, "/work/repo", "../../etc/passwd")).toBeNull();
    expect(fileUnderAny([], "/work/repo", "/work/repo/a.md")).toBeNull();
  });
});

describe("the whole path a row names", () => {
  it("is written with the slash the folder it is under is written with", () => {
    expect(fileAt(ROOT, ["src", "main.rs"])).toBe("/work/repo/src/main.rs");
    // A folder with no slash but a backslash is a Windows path, and a shell there is handed one.
    expect(fileAt("C:\\work\\repo", ["src", "main.rs"])).toBe("C:\\work\\repo\\src\\main.rs");
  });

  it("is the bound folder itself where the row is its own", () => {
    expect(fileAt(ROOT, [])).toBe(ROOT);
    // A folder recorded with a trailing slash names the same folder, and joining it raw would put
    // two of them in the middle of the path.
    expect(fileAt("/work/repo/", ["a.md"])).toBe("/work/repo/a.md");
  });

  /** The two directions answer about the same file: what `fileUnder` reads out of a whole path is
   *  what this puts back into one. */
  it("is read back to the segments it was made of", () => {
    expect(fileUnder(ROOT, null, fileAt(ROOT, ["src", "main.rs"]))).toEqual(["src", "main.rs"]);
  });
});
