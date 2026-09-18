import { createPortal } from "react-dom";
import { t, tf } from "../core/i18n";

/**
 * The question before a replacement that reaches more than one file (`AMB-D-911`).
 *
 * **It is asked where a reader cannot see what they are about to do, and not otherwise.** Inside one
 * file the list on the screen *is* the answer — every place that would be written is drawn in it —
 * and a question in front of that is a press to get past. Over four files it is not: the rest of
 * them are somewhere below, or not on the screen at all. That is the line JetBrains draws and the
 * one product of four that draws it anywhere (`AMB-T-4952`); the other two ask every time, which is
 * the same as not asking.
 *
 * **What it says is the two numbers a reader checks the press against**: how many places, and how
 * many files. Naming them would be a list to read in front of a press already decided on, and the
 * list is on the screen behind this.
 *
 * **There is no checkbox to stop asking.** The question before a bin has one because the bin gives
 * the row back; this writes the files, and what takes that back is git.
 */
export function ReplaceAsk({ hits, files, onGo, onCancel }: {
  /** How many places would be written. */
  hits: number;
  /** How many files they are in — the number that made this question worth asking. */
  files: number;
  onGo: () => void;
  onCancel: () => void;
}) {
  return createPortal(
    <div
      className="modal__overlay modal__overlay--raised"
      onMouseDown={(e) => e.stopPropagation()}
      onClick={(e) => { e.stopPropagation(); if (e.target === e.currentTarget) onCancel(); }}
      onKeyDown={(e) => { if (e.key === "Escape") onCancel(); }}
    >
      <div className="trashask" role="dialog" aria-modal="true" aria-labelledby="replaceask-title">
        <div className="trashask__title" id="replaceask-title">
          {tf("files.searchAsk", { hits, files })}
        </div>
        <div className="trashask__undoable">{t("files.searchNoUndo")}</div>
        <div className="trashask__actions">
          <button className="trashask__action trashask__action--go" autoFocus onClick={onGo}>
            {t("files.searchReplaceGo")}
          </button>
          <button className="trashask__action" onClick={onCancel}>{t("files.searchAskNo")}</button>
        </div>
      </div>
    </div>,
    document.body,
  );
}
