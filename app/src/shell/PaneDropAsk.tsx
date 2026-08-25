import { useState } from "react";
import { createPortal } from "react-dom";
import { errText, t } from "../core/i18n";
import { taskRef } from "../core/idref";

/**
 * What the way out of a pane asks when the session in it is still holding something.
 *
 * **It names what is about to be lost, at the moment it is about to be lost.** Pressing `✕` ends the
 * session and takes the place away (`./TerminalPane`), and a reservation that session made stays
 * `in_progress` with nothing left that could say whose it was — the volatile area goes with the pane
 * (`AMB-D-758`). Nothing afterwards can notice that happened, so the only place it can be said is
 * here.
 *
 * **The three answers are three different things to want**, which is why this is a question and not a
 * confirmation: hand the work back and go, go and leave it standing, or stay. The middle one is not a
 * mistake to be talked out of — a person stepping away from a machine for the night has every reason
 * to leave a reservation where it is.
 *
 * **Nothing is moved until one of them is pressed.** The screen does not tidy the ledger up on its
 * own: a reservation is a fact somebody made, and the only thing that may unmake it is somebody.
 *
 * A hand-back that is refused leaves the question standing with the refusal under it. The place is
 * not taken away in that case — it was asked for *with* the work handed back, and doing half of it
 * would lose the very thing that was just named.
 */
export function PaneDropAsk({ holding, onHandBack, onLeave, onCancel }: {
  /** The tasks this pane's session is holding, as the volatile area has them. Never empty — a pane
   *  holding nothing is not asked this question at all. */
  holding: readonly number[];
  /** Hand every one of them back to `todo`, then take the place away. */
  onHandBack: () => Promise<void>;
  /** Take the place away and leave the reservations standing. */
  onLeave: () => Promise<void>;
  onCancel: () => void;
}) {
  // Pressed once. Both roads that act end with this box gone, and a second press before that lands
  // would ask the same thing of the store twice.
  const [busy, setBusy] = useState(false);
  // A refusal from the road just taken, kept under the question rather than in place of it.
  const [failed, setFailed] = useState<string | null>(null);

  const go = (act: () => Promise<void>) => {
    setBusy(true);
    setFailed(null);
    void act().catch((e: unknown) => {
      setFailed(errText(e));
      setBusy(false);
    });
  };

  return createPortal(
    <div
      className="modal__overlay modal__overlay--raised"
      onMouseDown={(e) => e.stopPropagation()}
      onClick={(e) => { e.stopPropagation(); if (e.target === e.currentTarget) onCancel(); }}
      onKeyDown={(e) => { if (e.key === "Escape") onCancel(); }}
    >
      <div className="panedrop__modal" role="dialog" aria-modal="true" aria-labelledby="panedrop-title">
        <div className="panedrop__title" id="panedrop-title">{t("face.dropConfirm")}</div>
        <div className="panedrop__holding">{t("face.dropHolding")}</div>
        <ul className="panedrop__refs">
          {holding.map((id) => <li key={id}>{taskRef(id)}</li>)}
        </ul>
        {failed !== null && <p className="panedrop__failed">{failed}</p>}
        <div className="panedrop__actions">
          <button
            className="panedrop__action panedrop__action--go"
            autoFocus
            disabled={busy}
            onClick={() => go(onHandBack)}
          >
            {t("face.dropHandBack")}
          </button>
          <button className="panedrop__action" disabled={busy} onClick={() => go(onLeave)}>
            {t("face.dropAnyway")}
          </button>
          <button className="panedrop__action" disabled={busy} onClick={onCancel}>
            {t("face.dropCancel")}
          </button>
        </div>
      </div>
    </div>,
    document.body,
  );
}
