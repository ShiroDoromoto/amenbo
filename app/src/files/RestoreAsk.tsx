import { useState } from "react";
import { createPortal } from "react-dom";
import { t, tf } from "../core/i18n";
import { setAsksBeforeRestore } from "./askBeforeRestore";

/**
 * The question the face puts before it throws away what git has not recorded.
 *
 * **It is built like the bin's** (`./TrashAsk`), and for the reason that decision gives: the row
 * sits a pixel away from the ones that open a file, and there is nowhere on the screen to reassure
 * anybody afterwards (`AMB-D-777`).
 *
 * **What it says is the opposite of what the bin's says.** The bin promises the row comes back;
 * this one says it does not. A commit, a stash and a branch switch are all still in the reflog
 * afterwards, and a change never written down is nowhere once this is answered — which is what the
 * reader is weighing, and why the sentence is on the question rather than left to be known.
 *
 * **The checkbox takes effect on the answer, not on the tick**, the way a dialog's does everywhere
 * else: a reader who ticks it and then cancels has agreed to nothing.
 *
 * **Several rows are counted, not listed.** One row is named, because the name is what a reader
 * checks the press against; five named would be five lines to read before a press already decided
 * on (`AMB-T-4230`).
 */
export function RestoreAsk({ names, onGo, onCancel }: {
  /** The rows about to lose what they are holding, as the reader sees them named. */
  names: string[];
  /** Throw it away. The dialog is gone by the time this runs. */
  onGo: () => void;
  onCancel: () => void;
}) {
  const [quiet, setQuiet] = useState(false);

  const go = () => {
    if (quiet) setAsksBeforeRestore(false);
    onGo();
  };

  return createPortal(
    <div
      className="modal__overlay modal__overlay--raised"
      onMouseDown={(e) => e.stopPropagation()}
      onClick={(e) => { e.stopPropagation(); if (e.target === e.currentTarget) onCancel(); }}
      onKeyDown={(e) => { if (e.key === "Escape") onCancel(); }}
    >
      <div className="trashask" role="dialog" aria-modal="true" aria-labelledby="restoreask-title">
        <div className="trashask__title" id="restoreask-title">
          {/* Counted rather than declined: the one-row sentence is the branch above, so the number
              here is always more than one and no language needs a second arm for it. */}
          {names.length === 1
            ? tf("git.restoreAsk", { name: names[0] ?? "" })
            : tf("git.restoreAskMany", { n: names.length })}
        </div>
        <div className="trashask__undoable">{t("git.restoreGone")}</div>
        <label className="trashask__quiet">
          <input
            type="checkbox"
            checked={quiet}
            onChange={(e) => setQuiet(e.target.checked)}
          />
          {t("git.restoreQuiet")}
        </label>
        <div className="trashask__actions">
          {/* The cancel is what the focus lands on, where the bin's lands on going ahead. The two
              questions are the same shape and are not the same press: one of these is undone by
              pressing undo, and the other is not undone at all. */}
          <button className="trashask__action trashask__action--go" onClick={go}>
            {t("git.restoreGo")}
          </button>
          <button className="trashask__action" autoFocus onClick={onCancel}>
            {t("git.restoreKeep")}
          </button>
        </div>
      </div>
    </div>,
    document.body,
  );
}
