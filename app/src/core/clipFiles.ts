// A paste carrying files, read in a place that takes text.
//
// **The window cannot read a path off the clipboard.** A copy made in the file panel — or in the
// reader's file manager — puts the files themselves on, and the paths beside them as plain words
// (`AMB-D-832`). A page is not told where a file it is handed lives, so where the words are wanted
// the host is asked (`crate::clipboard`).
//
// ⚠ **And the words are not there to fall back on either.** WebKit hides the text flavours of a
// clipboard that is carrying file paths, so `getData("text/plain")` on such a paste answers with
// nothing at all — which is why a paste into an ordinary text box puts nothing in it rather than
// putting the path in (`AMB-T-4400`). The reading is still taken, because a clipboard put together
// somewhere else may carry words and no files, and then the words are all there is.
//
// **What arrives is written by the side that took it, not here.** A pane quotes what it is handed
// because a name with a space in it is two words to a shell; an editor does not, because a path
// pasted into text is a path and quoting it there would be damage (`AMB-D-832`). So this hands over
// the paths and the words, and each side writes its own.

import { invoke } from "./ipc";

/**
 * Whether a paste is carrying files, rather than only the words somebody copied.
 *
 * Both signs are read because they are not the same question and neither is promised on all three
 * machines: `files` is what the paste is carrying, and `types` is what it says it is carrying. The
 * paths themselves are in neither.
 */
export function holdsFiles(data: DataTransfer): boolean {
  return data.files.length > 0 || Array.from(data.types).includes("Files");
}

/**
 * Answer the pastes `host` is given that are carrying files, and leave every other paste alone.
 *
 * `put` is handed the paths the host read back, and the words the paste itself carried — which is
 * what is left when the clipboard holds files this machine will not name, and nothing at all when
 * it holds neither. Writing them is the caller's, because how a path is written depends on where it
 * is going (`AMB-D-832`).
 *
 * ⚠ **It is caught on the way down, on `host` rather than on the box the typing lands in.** The
 * listener that would otherwise answer this paste — the emulator's, or the editor's own — sits on
 * that box, and it was put there when the box was drawn, so a listener added beside it runs second
 * and the paste has already been answered. Held one level up and in the capture phase, this one is
 * reached first. Both halves are needed after that: `stopPropagation` keeps the box's own listener
 * out of it, and `preventDefault` keeps the page from answering the paste itself.
 *
 * **The narrowness is the point.** A paste of ordinary text is left where it was: the box it is
 * going into knows things this does not — whether the program in a pane asked for bracketed paste,
 * what an editor's own undo history should read the insert as — and answering those here would get
 * them wrong.
 *
 * The words are read before the host is asked rather than after: a clipboard event carries what it
 * carries only while it is being handled, and by the time an answer has come back there is nothing
 * left to fall back to.
 *
 * Answers with the way to stop listening.
 */
export function takesPastedFiles(
  host: HTMLElement,
  put: (paths: string[], words: string) => void,
): () => void {
  const pasted = (e: ClipboardEvent) => {
    const carried = e.clipboardData;
    if (carried === null || !holdsFiles(carried)) return;
    e.preventDefault();
    e.stopPropagation();
    const words = carried.getData("text/plain");
    void invoke<string[]>("clip_files", {})
      .then((paths) => put(paths, words))
      .catch(() => {});
  };
  host.addEventListener("paste", pasted, true);
  return () => host.removeEventListener("paste", pasted, true);
}
