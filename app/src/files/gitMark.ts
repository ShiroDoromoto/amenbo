// What colour a tree row wears, worked out from what git said about the folder (`AMB-D-785`).
//
// **The host hands over git's own two letters and decides nothing** (`crate::folder_git`). Which of
// them a reader is shown as what is this side's question, because it is a question about a screen:
// the same `MM` is one row in a tree and would be two words in a commit dialog.
//
// **Three marks, not one per letter.** git has a letter for every way a path can differ, and a
// reader looking at a folder for what an agent has been doing in it is asking a coarser question
// than that: is this new, is this changed, is this not recorded at all. So a row that is staged as
// new is `added`, one git has never seen is `untracked`, and everything else — changed, moved,
// copied, deleted from the index — is `modified`.
import type { GitEntryDto } from "../bindings/bindings";

/** The three things a row's colour says. */
export type GitMark = "modified" | "added" | "untracked";

/** The mark one of git's rows wears, read off the index letter it came with. */
function markOf(row: GitEntryDto): GitMark {
  if (row.index === "?") return "untracked";
  if (row.index === "A") return "added";
  return "modified";
}

/** Segments as one key. NUL because it is the one character a path segment cannot hold. */
const keyOf = (path: string[]) => path.join("\0");

/**
 * Read git's answer once, and hand back what each tree row should wear.
 *
 * **A folder git named as a whole answers for everything inside it.** That is what git does with an
 * untracked folder — it names the folder and stops — so a tree that only matched paths exactly
 * would leave every file in a brand-new folder colourless. The bound folder itself is that case
 * spelled with no segments at all, which is a repository where nothing is tracked yet: dropping it
 * would leave a new repository with no colour anywhere.
 *
 * **Nothing rolls up.** A folded folder holding a changed file wears no colour of its own — git
 * named the file, and what a folder should wear when the things under it disagree is a question
 * nobody has answered yet (`AMB-T-3839`).
 */
export function gitMarks(rows: GitEntryDto[]): (path: string[]) => GitMark | null {
  /** The rows git named exactly — files, and folders it named as a whole. */
  const own = new Map<string, GitMark>();
  /** The folders git named as a whole, whose mark reaches everything below them. */
  const under = new Map<string, GitMark>();
  for (const row of rows) {
    const mark = markOf(row);
    own.set(keyOf(row.path), mark);
    if (row.isDir) under.set(keyOf(row.path), mark);
  }
  return (path) => {
    const exact = own.get(keyOf(path));
    if (exact !== undefined) return exact;
    // Up from the folder holding it to the bound folder itself, which is the empty key.
    for (let depth = path.length - 1; depth >= 0; depth -= 1) {
      const above = under.get(keyOf(path.slice(0, depth)));
      if (above !== undefined) return above;
    }
    return null;
  };
}
