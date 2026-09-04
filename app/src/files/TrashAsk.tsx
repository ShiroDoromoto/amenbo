import { useState } from "react";
import { createPortal } from "react-dom";
import { t, tf, tn } from "../core/i18n";
import { setAsksBeforeTrash } from "./askBeforeTrash";

/**
 * The question the panel puts before it bins a row.
 *
 * **It is a confirmation, not a consultation.** There are two answers and one of them is "no", so
 * the room it takes is the name of the row and the two buttons — anything else would be reading
 * matter in front of a press somebody has already decided on.
 *
 * **What it promises is what the bin does.** The row goes where the machine's own deleted files go
 * and no further, and undo brings it back for as long as this window is open (`./folder`). Saying so
 * here is what makes "yes" cheap to press, and it is also why the checkbox can be trusted: turning
 * the question off is not turning a safeguard off.
 *
 * **The checkbox takes effect on the answer, not on the tick.** A reader who ticks it and then
 * cancels has not agreed to anything, so nothing is remembered — the same reading a dialog's
 * checkbox has everywhere else.
 *
 * **Several rows are counted, not listed.** One row is named, because the name is what a reader
 * checks the press against; five of them named would be five lines to read before a press already
 * decided on, and the thing to check then is how many (`AMB-T-4230`).
 */
export function TrashAsk({ names, onGo, onCancel }: {
  /** The rows about to go, as the reader sees them named. */
  names: string[];
  /** Bin it. The dialog is gone by the time this runs. */
  onGo: () => void;
  onCancel: () => void;
}) {
  const [quiet, setQuiet] = useState(false);

  const go = () => {
    if (quiet) setAsksBeforeTrash(false);
    onGo();
  };

  return createPortal(
    <div
      className="modal__overlay modal__overlay--raised"
      onMouseDown={(e) => e.stopPropagation()}
      onClick={(e) => { e.stopPropagation(); if (e.target === e.currentTarget) onCancel(); }}
      onKeyDown={(e) => { if (e.key === "Escape") onCancel(); }}
    >
      <div className="trashask" role="dialog" aria-modal="true" aria-labelledby="trashask-title">
        <div className="trashask__title" id="trashask-title">
          {names.length === 1
            ? tf("files.trashAsk", { name: names[0] ?? "" })
            : tn("files.trashAskMany", names.length)}
        </div>
        <div className="trashask__undoable">{t("files.trashUndoable")}</div>
        <label className="trashask__quiet">
          <input
            type="checkbox"
            checked={quiet}
            onChange={(e) => setQuiet(e.target.checked)}
          />
          {t("files.trashQuiet")}
        </label>
        <div className="trashask__actions">
          <button className="trashask__action trashask__action--go" autoFocus onClick={go}>
            {t("files.trashGo")}
          </button>
          <button className="trashask__action" onClick={onCancel}>{t("files.trashKeep")}</button>
        </div>
      </div>
    </div>,
    document.body,
  );
}
