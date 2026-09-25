// **Put a step in on a line inside an action** (`AMB-T-5257`), asked from the `+` on that line — and,
// from the press over the picture, a step on its own: the first one of all, or one more
// (`AMB-T-5315`).
//
// **It is the action picture's alone.** On an automation's picture what goes in is a placement of a
// library action: one off the shelf is picked in the panel beside the picture
// (`./AutomationLibraryPanel`), and one made on the spot is asked only a name and a library
// (`./AutomationActionMake`, `AMB-D-956`). A step is what carries a prompt, so it is here that the
// prompt is written.
//
// **The two roads are named apart** (`AMB-T-5526`). From a line it is "put a step in" and the dialog
// draws where — the box before, the way out, a dashed "here", the box after — because the new step
// goes between two things already there. From the press over the picture it is "add a step", with
// nothing to draw: the step stands on its own until a way out is pointed at it.
//
// **On a line, nothing is left running past a box.** The way out that was pressed comes to point at
// the new step, and the new step goes on to whatever that way out used to reach
// (`amenbo_core::ops::automation::step_insert`) — one act.
//
// **It asks a name and a prompt, and nothing else.** A step is made with one way out, "done"
// (`AMB-T-5516`), and what else it declares — more ways out, what it takes in, how it is run — is
// written on the panel a reader lands on the moment this closes, one row at a time.
//
// **Only its two buttons close it** (`AMB-T-5363`). A press on the backdrop or Escape would throw away
// a prompt half written, and nothing here keeps it — so neither is a way out, and the reader leaves
// by putting the step in or by giving it up.
import { useState } from "react";
import { createPortal } from "react-dom";
import { WhereMark, type WhereTo } from "./automationParts";
import { addAutomationStep, insertAutomationActionStep } from "../core/automations";
import { t } from "../core/i18n";

/** Where the new step goes: onto a line inside the action, or onto the action on its own. */
export type AddTarget = { picture: "action"; edgeId: number } | { picture: "action"; actionId: number };

export function AutomationStepAdd({
  into,
  where = null,
  onClose,
}: {
  into: AddTarget;
  /** The line it goes on, drawn over the fields — for a step put in on a line. */
  where?: WhereTo;
  onClose: () => void;
}) {
  const [name, setName] = useState("");
  const [prompt, setPrompt] = useState("");
  const onLine = "edgeId" in into;

  const ready = name.trim() !== "" && prompt.trim() !== "";
  const put = () => {
    if (!ready) return;
    const step = { name: name.trim(), prompt: prompt.trim() };
    void ("edgeId" in into
      ? insertAutomationActionStep(into.edgeId, step)
      : addAutomationStep(into.actionId, step));
    onClose();
  };

  return createPortal(
    <div className="modal__overlay">
      <div
        className="modal__card modal__card--wide"
        role="dialog"
        aria-modal="true"
        aria-labelledby="auto-add-title"
      >
        <h2 className="autodlg__title" id="auto-add-title">
          {onLine ? t("auto.act.insertTitle") : t("auto.act.addTitle")}
        </h2>

        {onLine && <WhereMark where={where} />}

        <label className="autostep__field">
          <span className="autostep__label">{t("auto.step.name")}</span>
          <input autoFocus value={name} onChange={(e) => setName(e.target.value)} />
        </label>

        <label className="autostep__field">
          <span className="autostep__label">{t("auto.step.prompt")}</span>
          <textarea
            className="autostep__prompt"
            rows={5}
            value={prompt}
            onChange={(e) => setPrompt(e.target.value)}
          />
        </label>

        <div className="buttonrow">
          <button type="button" className="btn btn--primary" disabled={!ready} onClick={put}>
            {onLine ? t("auto.act.insertPut") : t("auto.act.addPut")}
          </button>
          <button type="button" className="btn" onClick={onClose}>
            {t("auto.add.cancel")}
          </button>
        </div>
      </div>
    </div>,
    document.body,
  );
}
