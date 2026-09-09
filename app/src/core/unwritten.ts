// What has been typed and not yet written, and the one place a window on its way out asks for it.
//
// **A page that keeps what is typed keeps it a moment late.** Writing settles after a moment's quiet
// rather than on every keystroke, so at any instant there can be a sentence on the screen that is
// not in the store yet (`../files/MemoPage`). Every ordinary way out of a page already writes it —
// the page coming down runs its own cleanup — and the two that end a *window* do not: the process
// goes with `exit`, and a window destroyed never unloads the page in it.
//
// So the host holds those two open and asks first (`app/src-tauri/src/quit.rs`,
// `app/src-tauri/src/windows.rs`), and this is what it asks. A page leaves a way to write what it
// has here for as long as it is on screen; the way out writes them all, waits, and answers the host.
//
// **It is per window, because a webview is.** The board and the talk window each draw their own
// draft page and hold their own unwritten sentence, so each is asked and each answers for itself.

/** Writing out what one page holds. It answers when the writing has landed, and never throws. */
type Write = () => Promise<void>;

const held = new Set<Write>();

/**
 * Leave a way to write what this page has unwritten, for as long as it is on screen.
 *
 * The answer stops it being asked, and is the cleanup of the effect that left it — a page that has
 * come down has already written what it had, and a way out asking a page that is gone would be
 * writing over the one that replaced it.
 */
export function holdsUnwritten(write: Write): () => void {
  held.add(write);
  return () => {
    held.delete(write);
  };
}

/**
 * Write out everything this window holds, and answer once all of it has landed.
 *
 * It answers at once where nothing is held, which is the ordinary case: a way out that made every
 * quit wait would be paying for a draft page nobody had open.
 */
export async function writeUnwritten(): Promise<void> {
  await Promise.all([...held].map((write) => write().catch(() => {})));
}

/**
 * Write what is unwritten when the host says `event`, and answer it with `answered`.
 *
 * The host holds a window's way out open only for as long as it takes to be answered, and takes it
 * anyway if it never is (`app/src-tauri/src/quit.rs`), so what matters here is that the answer comes
 * after the writing has landed and not after it has been asked for.
 *
 * Which way out this is, and what answering it means, belong to the window that has one — this is
 * the wiring and nothing else. It answers with the way to stop listening.
 */
export function writesOn(event: string, answered: () => void): () => void {
  let unlisten: (() => void) | undefined;
  let gone = false;
  void import("@tauri-apps/api/event")
    .then(({ listen }) => listen(event, () => void writeUnwritten().then(answered)))
    .then((un) => {
      if (gone) un();
      else unlisten = un;
    })
    // Outside Tauri (`npm run dev` in a browser) there is no host to hear from, and no way out of a
    // window to be asked about either.
    .catch(() => {});
  return () => {
    gone = true;
    unlisten?.();
  };
}
