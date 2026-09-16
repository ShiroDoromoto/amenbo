// @vitest-environment jsdom
// The history of the folder the window is on, read in the column across the panes.
//
// What has to be right here is the road in and the road back out: three layers stand in one face,
// each press goes one deeper, and Escape takes one off at a time before it reaches the column's own
// two layers (`AMB-D-815`). And the face is not there at all until somebody presses for it — the
// column says nothing about git until then (`AMB-D-905`).
import { describe, expect, it } from "vitest";
import type { GitCommitDto, GitFileDto } from "../bindings/bindings";
import { button, click, container, draw, hoisted, press, ROOT, settle } from "./filesPanelKit";
import { t } from "../core/i18n";

/** One commit as the list draws it, filled in around whatever a test cares about. */
const commit = (about: Partial<GitCommitDto> & { sha: string }): GitCommitDto => ({
  short: about.sha.slice(0, 8),
  parents: ["0".repeat(40)],
  author: "Alice",
  at: "2026-09-16T12:00:00Z",
  subject: "did a thing",
  ...about,
});

/** One path a commit touched. */
const touched = (about: Partial<GitFileDto> & { path: string }): GitFileDto => ({
  from: null,
  added: 3,
  removed: 1,
  ...about,
});

const A = "a".repeat(40);
const B = "b".repeat(40);

/** The column, on the history face, with the folder the window is on handed down. */
const open = () => draw({ tab: "history", history: true, gitRoot: ROOT });

/** The rows of whichever list is up, by what each says it is. */
const rows = () =>
  [...container.querySelectorAll(".githist__subject")].map((one) => one.textContent);

/** The reading column, which is where Escape is pressed. */
const column = () => container.querySelector(".termface__column--side .files")!;

describe("the history in the reading column", () => {
  it("draws the commits newest first, and says which were merges", async () => {
    hoisted.log = [
      commit({ sha: A, subject: "the newest", parents: [B, "c".repeat(40)] }),
      commit({ sha: B, subject: "the one before" }),
    ];
    await open();
    expect(hoisted.asked).toContain(`log:${ROOT}`);
    expect(rows()).toEqual(["the newest", "the one before"]);
    // A merge is told by what it was made on top of: the subject of one is written by whoever
    // merged it and reads like any other.
    const merges = [...container.querySelectorAll(".githist__merge")];
    expect(merges).toHaveLength(1);
    expect(merges[0]?.closest(".githist__row")?.textContent).toContain("the newest");
  });

  /// One face, three layers. Each press goes one deeper into the same tab rather than opening a
  /// second one beside it.
  it("goes a layer deeper on each press: the list, one commit, one of its files", async () => {
    hoisted.log = [commit({ sha: A, subject: "the newest" })];
    hoisted.touched[A] = [touched({ path: "app/src/lib.rs" }), touched({ path: "README.md" })];
    hoisted.patch[`${A} app/src/lib.rs`] = "@@ -1 +1 @@\n-was\n+is\n";
    await open();

    await click(button("the newest"));
    await settle();
    expect(hoisted.asked).toContain(`show:${ROOT}:${A}`);
    // The name is what a row is read by, and where it is stands beside it.
    expect(rows()).toEqual(["lib.rs", "README.md"]);

    await click(button("lib.rs"));
    await settle();
    expect(hoisted.asked).toContain(`diff:${ROOT}:${A}:app/src/lib.rs`);
    const lines = [...container.querySelectorAll(".githist__line")].map((one) => one.textContent);
    expect(lines.join("")).toContain("+is");
    // git's own text, coloured by the character it begins each line with.
    expect(container.querySelector(".githist__line--added")?.textContent).toContain("+is");
    expect(container.querySelector(".githist__line--removed")?.textContent).toContain("-was");
    expect(container.querySelector(".githist__line--hunk")?.textContent).toContain("@@");
  });

  /// One press, one layer — and the column's own two are under the face's, not above them.
  it("takes one layer off on each Escape, and reaches the column only after the last", async () => {
    hoisted.log = [commit({ sha: A, subject: "the newest" })];
    hoisted.touched[A] = [touched({ path: "app/src/lib.rs" })];
    hoisted.patch[`${A} app/src/lib.rs`] = "@@ -1 +1 @@\n+is\n";
    await open();
    await click(button("the newest"));
    await settle();
    await click(button("lib.rs"));
    await settle();
    expect(container.querySelector(".githist__patch")).not.toBeNull();

    await press(column(), "Escape");
    // Back at the commit's own files, not back at the list.
    expect(container.querySelector(".githist__patch")).toBeNull();
    expect(rows()).toEqual(["lib.rs"]);

    await press(column(), "Escape");
    expect(rows()).toEqual(["the newest"]);

    // And only now the column's own layers. The face is still the history — what the next press
    // gives back is the room, and the one after it the column.
    await press(column(), "Escape");
    expect(container.querySelector(".githist__list")).not.toBeNull();
  });

  /// The way back is drawn as well as pressed for: a reader who reaches for the control and a
  /// reader who reaches for the key end up in the same place.
  it("goes back a layer by the control, the way the key does", async () => {
    hoisted.log = [commit({ sha: A, subject: "the newest" })];
    hoisted.touched[A] = [touched({ path: "README.md" })];
    await open();
    await click(button("the newest"));
    await settle();

    await click(container.querySelector(".githist__back"));
    await settle();
    expect(rows()).toEqual(["the newest"]);
  });

  /// The column shows nothing about git until somebody presses for it, so its tab is not in the row
  /// either.
  it("has no tab in the row until the history has been pressed for", async () => {
    hoisted.log = [commit({ sha: A })];
    await draw({ gitRoot: ROOT });
    expect(button(t("git.history"))).toBeUndefined();
    // And nothing was asked for: the call is the reader's who wants it, not everyone's.
    expect(hoisted.asked.some((one) => one.startsWith("log:"))).toBe(false);

    await open();
    expect(button(t("git.history"))).toBeDefined();
  });

  /// A folder that is no repository, and one nobody has committed in yet, answer the same way —
  /// with nothing. The line says so rather than leaving an empty face.
  it("says so where nothing has been committed", async () => {
    hoisted.log = [];
    await open();
    expect(container.textContent).toContain(t("git.noHistory"));
  });

  /// git counts no lines in a file it read as bytes, and says so with a dash rather than a nought.
  it("says a file was read as bytes rather than counting nothing", async () => {
    hoisted.log = [commit({ sha: A, subject: "the newest" })];
    hoisted.touched[A] = [touched({ path: "icon.png", added: null, removed: null })];
    await open();
    await click(button("the newest"));
    await settle();
    expect(container.textContent).toContain(t("git.bytes"));
  });
});
