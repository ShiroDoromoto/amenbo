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

  it("is what git said about a folder it answered for as a whole, all the way down", () => {
    // Asked with `-uall`, the only folder git answers for whole is one it cannot walk into: a
    // repository of its own sitting inside this one (`AMB-D-919`). It says nothing about what is in
    // there, so the folder's own mark is the only answer there is, and it reaches every row below.
    const marks = gitMarks([row(["nested"], "?", true)]);

    expect(marks(["nested"], true)).toBe("untracked");
    expect(marks(["nested"], false), "the folder git named keeps its own mark, open or folded")
      .toBe("untracked");
    expect(marks(["nested", "one.md"]), "and nothing under it is ever named on its own")
      .toBe("untracked");
  });
});
