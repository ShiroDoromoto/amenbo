// Which folder a heading is about, which is the only thing telling two sections apart.
//
// A project bound to two repositories that each have an `app` in them draws two trees. If both
// headings say `app`, the reader has no way to know which repository they are looking into — and
// the rows underneath say nothing about it either, being paths from the folder rather than to it.
import { describe, expect, it } from "vitest";
import { sectionsOf } from "./sections";

const there = (...paths: string[]) => paths.map((path) => ({ path, exists: true }));
const names = (paths: string[]) => sectionsOf(there(...paths)).map((one) => one.label);

describe("the folders the face draws", () => {
  it("are in the order their paths sort in, whichever order they arrived in", () => {
    const rows = sectionsOf(there("/work/repo", "/work/amenbo", "/work/plugins"));
    expect(rows.map((one) => one.path)).toEqual(["/work/amenbo", "/work/plugins", "/work/repo"]);
  });

  it("are named by the folder's own name where that is enough", () => {
    expect(names(["/work/amenbo", "/work/plugins"])).toEqual(["amenbo", "plugins"]);
  });

  it("say enough of the path to tell two of the same name apart", () => {
    expect(names(["/work/repoA/app", "/work/repoB/app"])).toEqual(["repoA/app", "repoB/app"]);
  });

  /** Only the ones that clash grow: a path spelled out says less at a glance than one word. */
  it("leaves a name that is already unique alone", () => {
    expect(names(["/work/repoA/app", "/work/repoB/app", "/work/notes"]))
      .toEqual(["notes", "repoA/app", "repoB/app"]);
  });

  it("keeps growing until the names are apart, however far back that is", () => {
    expect(names(["/one/x/app/src", "/two/x/app/src"]))
      .toEqual(["one/x/app/src", "two/x/app/src"]);
  });

  /** Two bindings that are the same path have the same name however far back it is taken, so the
   *  growing has to stop on its own rather than run to the root and back. */
  it("stops when there is nothing left to grow into", () => {
    expect(names(["/work/app", "/work/app"])).toEqual(["work/app", "work/app"]);
  });

  it("names a folder with no segments by the only spelling it has", () => {
    expect(names(["/"])).toEqual(["/"]);
  });

  it("carries whether the folder is there, which is what the section draws instead of rows", () => {
    const rows = sectionsOf([{ path: "/work/gone", exists: false }]);
    expect(rows[0]!.exists).toBe(false);
  });
});
