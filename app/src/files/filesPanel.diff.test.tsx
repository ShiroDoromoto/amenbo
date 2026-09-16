// @vitest-environment jsdom
// What the rows picked out in the rail's git half are holding, read in the column across the panes.
//
// What has to be right here is which question is put to git — the working tree's two halves are two
// different answers about the same path — and that several rows come back as several patches a
// reader can tell apart. And that the face has no layer of its own: Escape from it is already the
// column's own (`AMB-D-815`, `AMB-D-906`, 2-4).
import { describe, expect, it } from "vitest";
import { container, draw, hoisted, press, ROOT, settle, tell } from "./filesPanelKit";
import { t } from "../core/i18n";

/** One file's patch, as git writes it: the head, the two names, and a hunk. */
const patchFor = (path: string, body = "@@ -1 +1 @@\n-was\n+is\n") =>
  `diff --git a/${path} b/${path}\nindex 1111111..2222222 100644\n--- a/${path}\n+++ b/${path}\n${body}`;

/** The column, on the diff face, with the folder the window is on handed down. */
const open = (paths: string[][], staged = false) =>
  draw({ tab: "diff", diff: true, diffPick: { paths, staged }, gitRoot: ROOT });

/** The name over each patch, in the order they are drawn. */
const names = () =>
  [...container.querySelectorAll(".gitdiff__name")].map((one) => one.textContent);

/** The reading column, which is where Escape is pressed. */
const column = () => container.querySelector(".termface__column--side .files")!;

describe("the picked rows' patches in the reading column", () => {
  it("asks about the picked paths, and draws one patch under each name", async () => {
    hoisted.treePatch["false src/lib.rs,README.md"] =
      patchFor("src/lib.rs") + patchFor("README.md", "@@ -2 +2 @@\n+a line\n");
    await open([["src", "lib.rs"], ["README.md"]]);

    expect(hoisted.asked).toContain(`tree:${ROOT}:false:src/lib.rs,README.md`);
    // git's own spelling of each path, read off the patch rather than off what was asked for: git
    // writes nothing for a path it found no change in.
    expect(names()).toEqual(["src/lib.rs", "README.md"]);
    // Each patch under its own name, coloured the way every patch in this window is.
    const added = [...container.querySelectorAll(".patch__line--added")].map((o) => o.textContent);
    expect(added.join("")).toContain("+is");
    expect(added.join("")).toContain("+a line");
  });

  /// The two halves are two questions. A row of the staged list is asked about with `--cached`, and
  /// a row of the changes list without it.
  it("asks the half the rows were picked in", async () => {
    hoisted.treePatch["true src/lib.rs"] = patchFor("src/lib.rs");
    await open([["src", "lib.rs"]], true);
    expect(hoisted.asked).toContain(`tree:${ROOT}:true:src/lib.rs`);
    expect(names()).toEqual(["src/lib.rs"]);
  });

  /// The face stands where a reader pressed for it and the set has since been put down — staging
  /// the last row of five is the ordinary way it happens.
  it("says what to press where nothing is picked, and asks git nothing", async () => {
    await draw({ tab: "diff", diff: true, diffPick: null, gitRoot: ROOT });
    expect(container.textContent).toContain(t("git.diffNone"));
    expect(hoisted.asked.filter((one) => one.startsWith("tree:"))).toEqual([]);
  });

  /// A path picked out that git has nothing to say about — one staged whole, with nothing left in
  /// the working tree.
  it("says so where git wrote no patch", async () => {
    await open([["src", "lib.rs"]]);
    expect(container.textContent).toContain(t("git.diffEmpty"));
    expect(names()).toEqual([]);
  });

  /// A patch left standing while the file under it is written to is the one thing this face must
  /// not show: it is read to decide whether to stage.
  it("asks again when the host says the folder moved", async () => {
    hoisted.treePatch["false src/lib.rs"] = patchFor("src/lib.rs");
    await open([["src", "lib.rs"]]);
    const before = hoisted.asked.filter((one) => one.startsWith("tree:")).length;

    hoisted.treePatch["false src/lib.rs"] = patchFor("src/lib.rs", "@@ -1 +1 @@\n+written again\n");
    tell({ root: ROOT, capped: false, unwatched: false, gone: false });
    await settle();

    expect(hoisted.asked.filter((one) => one.startsWith("tree:")).length).toBeGreaterThan(before);
    const added = [...container.querySelectorAll(".patch__line--added")].map((o) => o.textContent);
    expect(added.join("")).toContain("+written again");
  });

  /// No layer of its own: the history goes three deep and comes back up one press at a time, and
  /// there is nothing under a patch here to come up from. So the presses go to the column — its
  /// width first, then the column itself.
  it("gives its Escapes to the column, having no layer of its own", async () => {
    hoisted.treePatch["false src/lib.rs"] = patchFor("src/lib.rs");
    let closed = 0;
    await draw({
      tab: "diff",
      diff: true,
      diffPick: { paths: [["src", "lib.rs"]], staged: false },
      gitRoot: ROOT,
      onClose: () => { closed += 1; },
    });
    expect(names()).toEqual(["src/lib.rs"]);

    // The column is standing narrow, so the first press is already the one that closes it — and
    // the patch is still drawn on the way, because the face itself never moved.
    await press(column(), "Escape");
    expect(closed).toBe(1);
    expect(names()).toEqual(["src/lib.rs"]);
  });
});
