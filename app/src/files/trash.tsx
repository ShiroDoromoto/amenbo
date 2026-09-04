// The machine's bin, as both columns of the terminal face reach it: the tree in the rail, where rows
// are picked out and binned together, and the file being read in the column on the other side
// (`AMB-D-835`).
//
// **Held once and used twice, because the two columns ask the same question.** What a bin press
// needs is the question before it, the line a refusal left, and the way back — and writing that out
// on each side would be two answers to one question, drifting apart at the first change.
//
// **The question is drawn where the press was made.** Each column mounts its own, so a reader who
// pressed the bin on a row in the rail is asked in the rail, and one who pressed it on the file they
// are reading is asked over the file. What is shared is the behaviour, not one question standing in
// a place neither press came from.
import { useState, type ReactNode } from "react";
import { errText } from "../core/i18n";
import { folderTrash, folderUntrash } from "./folder";
import { whyStopped } from "./stopped";
import { asksBeforeTrash } from "./askBeforeTrash";
import { TrashAsk } from "./TrashAsk";

export type Trash = {
  /** Put rows in the bin, asking first unless this reader has said not to (`./askBeforeTrash`). */
  askTrash: (root: string, paths: string[][]) => void;
  /** The last press of the bin, undone. */
  undo: () => void;
  /** Whether a question is standing. A column reads it to leave Escape to the question rather than
   *  taking a layer of its own off. */
  asking: boolean;
  /** How many presses have landed. A column watches it to take the focus back once one has. */
  acted: number;
  /** The question and the line the last refusal left, to be drawn in the column that pressed. */
  aside: ReactNode;
};

/**
 * The bin, for one column.
 *
 * `onGone` is how the column that binned rows tells whoever else is drawing them: the paths that
 * actually went, in the folder they went from. What the reading column does with that is put the
 * file away where it was one of them — a file that is no longer there is not one to go on reading.
 */
export function useTrash(
  projectId: number | null,
  onGone?: (root: string, went: string[]) => void,
): Trash {
  // The rows a question about the bin is standing over, or nothing while none is up.
  const [asking, setAsking] = useState<{ root: string; paths: string[][] } | null>(null);
  // What the machine said about the last row that would not go — kept until the next press, because
  // the row it is about is no longer on the list to say it for itself.
  const [stopped, setStopped] = useState<string | null>(null);
  // How many times this side has changed a folder. What redraws the rows is not this but the host's
  // word that the folder moved, which each section counts for itself; this is only how the focus
  // knows a press has landed.
  const [acted, setActed] = useState(0);

  // Put rows in the machine's bin. Nothing here deletes: what the host offers is the bin, and a
  // machine that cannot offer one refuses rather than deleting instead (`./folder`).
  //
  // **One press however many rows it is about**, which is what makes undo put back exactly what the
  // press took away: the host holds a press as one entry (`crate::trash`).
  const bin = (root: string, paths: string[][]) => {
    if (projectId === null) return;
    setStopped(null);
    void folderTrash(projectId, root, paths)
      .then((done) => {
        setActed((n) => n + 1);
        setStopped(done.stopped === null ? null : whyStopped(done.stopped));
        // Which of them went is the front of the list: the rows go in the order they were given and
        // the first refusal ends the press, so the count of them is where it got to
        // (`crate::trash`).
        const went = paths.slice(0, done.gone.length).map((one) => one.join("/"));
        if (went.length > 0) onGone?.(root, went);
      })
      .catch((e: unknown) => setStopped(errText(e)));
  };

  // Undo, which here means the last press of the bin and nothing else. It is the OS's own key rather
  // than one Amenbo invented, and it is heard on the column rather than on the window: the terminal
  // beside it has its own idea of what the key means, and the boundary between the two is which of
  // them the reader is in (`AMB-D-780`).
  const undo = () => {
    setStopped(null);
    void folderUntrash()
      .then((done) => {
        if (done === null) return;
        setActed((n) => n + 1);
        setStopped(done.stopped === null ? null : whyStopped(done.stopped));
      })
      .catch((e: unknown) => setStopped(errText(e)));
  };

  return {
    askTrash: (root, paths) => {
      if (asksBeforeTrash()) setAsking({ root, paths });
      else bin(root, paths);
    },
    undo,
    asking: asking !== null,
    acted,
    aside: (
      <>
        {stopped !== null && <p className="files__stopped">{stopped}</p>}
        {asking !== null && (
          <TrashAsk
            names={asking.paths.map((one) => one[one.length - 1] ?? "")}
            onGo={() => { const one = asking; setAsking(null); bin(one.root, one.paths); }}
            onCancel={() => setAsking(null)}
          />
        )}
      </>
    ),
  };
}
