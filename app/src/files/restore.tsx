// Throwing away what a file has that git has not recorded, as every row that offers it reaches the
// same act (`AMB-D-906`).
//
// **Held once and pressed from several places**, the way the bin is (`./trash`): the rows of the
// tree, the changed paths in the rail's git half, and the file being read on the other side of the
// panes all name the same act on the same folder. Written out at each of them it would be one
// question answered three ways, drifting apart at the first change.
//
// **The question is drawn where the press was made.** Each caller mounts its own, so a reader who
// pressed on a row in the rail is asked in the rail. What is shared is the behaviour, not one
// question standing somewhere neither press came from.
//
// **There is no way back from this one.** A commit, a stash and a branch switch are all still in
// the reflog; a change never written down is nowhere once git has been told. That is why the
// question stands by default and why its own switch is not the bin's (`./askBeforeRestore`).
import { useState, type ReactNode } from "react";
import { errText } from "../core/i18n";
import { folderGitRestore } from "./folder";
import { asksBeforeRestore } from "./askBeforeRestore";
import { RestoreAsk } from "./RestoreAsk";

export type Restore = {
  /** Throw the working tree's changes to these paths away, asking first unless this reader has
   *  said not to. The paths are spelled from the bound folder, as git's own rows are. */
  askRestore: (root: string, paths: string[][]) => void;
  /** Whether a question is standing. A column reads it to leave Escape to the question rather than
   *  taking a layer of its own off. */
  asking: boolean;
  /** The question and the line git's refusal left, to be drawn where the press was made. */
  aside: ReactNode;
};

/** The act, for one caller. */
export function useRestore(projectId: number | null): Restore {
  // The paths a question is standing over, or nothing while none is up.
  const [asking, setAsking] = useState<{ root: string; paths: string[][] } | null>(null);
  // What git said in refusing the last one — kept until the next press, because the row it is about
  // may be gone from the list by the time it is read.
  const [refused, setRefused] = useState<string | null>(null);

  const go = (root: string, paths: string[][]) => {
    if (projectId === null) return;
    setRefused(null);
    void folderGitRestore(projectId, root, paths)
      // git's own sentence, word for word. Amenbo does not say it again in its own words: what the
      // reader gets here is what they would get in a terminal (`AMB-D-906`).
      .catch((why: unknown) => setRefused(errText(why)));
  };

  return {
    askRestore: (root, paths) => {
      if (paths.length === 0) return;
      if (asksBeforeRestore()) setAsking({ root, paths });
      else go(root, paths);
    },
    asking: asking !== null,
    aside: (
      <>
        {refused !== null && <p className="files__none">{refused}</p>}
        {asking !== null && (
          <RestoreAsk
            names={asking.paths.map((one) => one[one.length - 1] ?? "")}
            onGo={() => { const now = asking; setAsking(null); go(now.root, now.paths); }}
            onCancel={() => setAsking(null)}
          />
        )}
      </>
    ),
  };
}
