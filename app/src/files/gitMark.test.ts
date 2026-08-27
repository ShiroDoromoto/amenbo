// What a row wears, and what a folded folder wears for the things it is hiding (`AMB-D-795`).
import { describe, expect, it } from "vitest";
import type { GitEntryDto } from "../bindings/bindings";
import { gitMarks } from "./gitMark";

/** One of git's rows. `index` is the letter the mark is read off. */
function row(path: string[], index: string, isDir = false): GitEntryDto {
  return { path, index, worktree: " ", isDir };
}

describe("what a folded folder wears", () => {
  it("is the mark of what is under it, and nothing once it is open", () => {
    const marks = gitMarks([row(["src", "main.rs"], " ")]);

    expect(marks(["src"], true), "the folder said nothing about what it hides").toBe("modified");
    // Open, the rows inside say it themselves — a colour running from the bound folder down to one
    // changed file would only ever mean "somewhere, something".
    expect(marks(["src"], false)).toBeNull();
    expect(marks(["src"]), "a file is only ever itself").toBeNull();
    expect(marks(["src", "main.rs"])).toBe("modified");
  });

  it("is the furthest-from-recorded of them where they disagree", () => {
    const marks = gitMarks([
      row(["src", "one.rs"], " "),
      row(["src", "two.rs"], "A"),
      row(["src", "three.rs"], "?"),
      row(["src", "four.rs"], " "),
      row(["src", "five.rs"], " "),
    ]);

    // Not the commonest — three changed files would otherwise hide the one nothing has recorded,
    // which is the only one of them that is gone if nobody notices it.
    expect(marks(["src"], true)).toBe("untracked");
    expect(gitMarks([row(["src", "one.rs"], " "), row(["src", "two.rs"], "A")])(["src"], true))
      .toBe("added");
  });

  it("reaches every folder on the way up, not just the one holding it", () => {
    const marks = gitMarks([row(["a", "b", "c", "deep.md"], "?")]);

    expect(marks(["a"], true)).toBe("untracked");
    expect(marks(["a", "b"], true)).toBe("untracked");
    expect(marks(["a", "b", "c"], true)).toBe("untracked");
  });

  it("gives way to what git said about the folder itself", () => {
    // git names an untracked folder and stops, so the folder has an answer of its own; a rollup that
    // spoke over it would be the tree telling the reader something git did not say.
    const marks = gitMarks([row(["fresh"], "?", true), row(["fresh", "one.md"], "A")]);

    expect(marks(["fresh"], true)).toBe("untracked");
    expect(marks(["fresh"], false), "the folder git named keeps its own mark, open or folded")
      .toBe("untracked");
  });
});
