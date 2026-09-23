// **Declare what a way out hands on** (`AMB-T-5257`), asked from the way out it belongs to.
//
// **The name follows the way out until somebody writes their own.** A way out that hands on one
// thing is named for what it hands on nine times out of ten — "the draft" leaving by "drafted" — so
// the box starts on the way out's own name and stops following the moment a reader touches it. It
// only starts there where the way out is named and hands on nothing yet: a second output named after
// the way out would be the first one's name again, and the unnamed way out has no name to lend.
//
// **Required or optional is a pulldown with no sentence under it.** What the two do is refuse the
// step or let it run, which the words already say; a line explaining it would be read once and then
// never again.
//
// **Only its two buttons close it** (`AMB-T-5363`), as with the dialog that puts a box in
// (`./AutomationStepAdd`): a press on the backdrop or Escape is not a way out.
import { useState } from "react";
import { createPortal } from "react-dom";
import { addAutomationOutput } from "../core/automations";
import { t } from "../core/i18n";
import { PORT_KINDS } from "./automationPortKinds";
import type { AutomationExitDto } from "../bindings/bindings";

export function AutomationOutputAdd({
  exit,
  onClose,
}: {
  exit: AutomationExitDto;
  onClose: () => void;
}) {
  // Nothing written yet. The way out's own name stands in until it is, and `null` is what says so.
  const [own, setOwn] = useState<string | null>(null);
  const [kind, setKind] = useState<string>("value");
  const [required, setRequired] = useState(true);
  // The unnamed way out has no name to lend — what the list calls it is a word for "the only one",
  // which is not what anybody would call the thing it hands on.
  const name = own ?? (exit.name !== undefined && exit.outputs.length === 0 ? exit.name : "");

  const add = () => {
    if (name.trim() === "") return;
    void addAutomationOutput(exit.id, { name: name.trim(), kind, required });
    onClose();
  };

  return createPortal(
    <div className="modal__overlay">
      <div className="modal__card" role="dialog" aria-modal="true" aria-labelledby="auto-out-title">
        <h2 className="autodlg__title" id="auto-out-title">{t("auto.out.title")}</h2>

        <label className="autostep__field">
          <span className="autostep__label">{t("auto.step.name")}</span>
          <input autoFocus value={name} onChange={(e) => setOwn(e.target.value)} />
        </label>

        <label className="autostep__field">
          <span className="autostep__label">{t("auto.out.kind")}</span>
          <select value={kind} onChange={(e) => setKind(e.target.value)}>
            {PORT_KINDS.map((one) => (
              <option key={one.id} value={one.id}>
                {one.label()}
              </option>
            ))}
          </select>
        </label>

        <label className="autostep__field">
          <span className="autostep__label">{t("auto.add.required")}</span>
          <select value={required ? "yes" : "no"} onChange={(e) => setRequired(e.target.value === "yes")}>
            <option value="yes">{t("auto.add.required")}</option>
            <option value="no">{t("auto.add.optional")}</option>
          </select>
        </label>

        <div className="buttonrow">
          <button type="button" className="btn btn--primary" disabled={name.trim() === ""} onClick={add}>
            {t("auto.out.add")}
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
