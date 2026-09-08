// The screen a reader gets when the file they are typing in was written to underneath them, and
// they want to see what that means before choosing (`AMB-D-863`).
//
// **It is a comparison, not a merge editor.** The two texts stand side by side and neither can be
// typed into; what it is opened to settle is which of them stands, and the two answers are on it —
// the same two the notice behind it carries, so a reader who looked does not have to close this and
// remember what they saw.
//
// **What it shows is a still of both.** The disk's text was read when this was opened and the
// reader's was taken out of the editor then; nothing here follows either afterwards. Neither can
// move while it is up — the editor is behind this screen and the file being written to again is
// what the save finds out, and says.

import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { t } from "../core/i18n";
import { mountDiff, type Compared } from "./diffLoad";

export function FileDiff({ name, theirs, mine, onKeepMine, onReadAgain, onClose }: {
  /** The file's own name, said at the head so the screen names what it is about. */
  name: string;
  /** What is on the disk now. */
  theirs: string;
  /** What the reader has typed and not saved. */
  mine: string;
  /** Write the reader's text over the file. This screen is gone by the time it runs. */
  onKeepMine: () => void;
  /** Take the disk's text and lose the reader's. It asks before it does, and this screen stands
   *  behind the question until it is answered — a reader who says no is one still weighing the two
   *  texts (`./FilesPanel`). */
  onReadAgain: () => void;
  onClose: () => void;
}) {
  const host = useRef<HTMLDivElement | null>(null);
  const [drawn, setDrawn] = useState(false);

  useEffect(() => {
    let alive = true;
    let mounted: Compared | null = null;
    const parent = host.current;
    if (parent === null) return;
    void mountDiff(parent, theirs, mine).then(
      (one) => {
        if (!alive) {
          one.close();
          return;
        }
        mounted = one;
        setDrawn(true);
      },
      // A comparison that never arrives leaves the two texts as texts. They are what the screen is
      // for, and a reader can still read one against the other — what is lost is the marks saying
      // where they differ, not the answer they were opened to give.
      () => {},
    );
    return () => {
      alive = false;
      mounted?.close();
    };
  }, [theirs, mine]);

  return createPortal(
    <div
      className="modal__overlay modal__overlay--raised"
      onMouseDown={(e) => e.stopPropagation()}
      onClick={(e) => { e.stopPropagation(); if (e.target === e.currentTarget) onClose(); }}
      onKeyDown={(e) => { if (e.key === "Escape") onClose(); }}
    >
      <div className="filediff" role="dialog" aria-modal="true" aria-labelledby="filediff-title">
        <div className="filediff__title" id="filediff-title">
          {t("files.seeDifference")} — {name}
        </div>
        {/* Which side is which, said above the two rather than inside them: the left one is the
            file as it stands and the right one is the reader's, and a screen that did not say so
            would leave the whole choice to be guessed from which text looks familiar. */}
        <div className="filediff__sides">
          <span className="filediff__side">{t("files.diffTheirs")}</span>
          <span className="filediff__side">{t("files.diffMine")}</span>
        </div>
        <div className="filediff__body">
          <div className="filediff__view" ref={host} />
          {!drawn && (
            <div className="filediff__plain">
              <pre className="files__text">{theirs}</pre>
              <pre className="files__text">{mine}</pre>
            </div>
          )}
        </div>
        <div className="filediff__actions">
          <button className="filediff__action filediff__action--go" onClick={onKeepMine}>
            {t("files.keepMine")}
          </button>
          <button className="filediff__action" onClick={onReadAgain}>{t("files.readAgain")}</button>
          <button className="filediff__action" onClick={onClose}>{t("files.diffClose")}</button>
        </div>
      </div>
    </div>,
    document.body,
  );
}
