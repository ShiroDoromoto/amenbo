// **Put a step in on a line inside an action** (`AMB-T-5257`), asked from the `+` on that line — and,
// on a picture with nothing on it, the first step of all (`AMB-T-5315`).
//
// **It is the action picture's alone.** On an automation's picture what goes in is a placement of a
// library action: one off the shelf is picked in the panel beside the picture
// (`./AutomationLibraryPanel`), and one made on the spot is asked only a name and a library
// (`./AutomationActionMake`, `AMB-D-956`). A step is what carries a prompt, so it is here that the
// prompt is written.
//
// **On a line, nothing is left running past a box.** The way out that was pressed comes to point at
// the new step, and the new step goes on to whatever that way out used to reach
// (`amenbo_core::ops::automation::step_insert`) — one act.
//
// **A picture with nothing on it is opened the same way.** The press on an empty picture names the
// action instead of a line, and the step it puts down is the one a placement of the action opens.
//
// **What it declares is what a dialog can take without becoming a screen**: the named ways out, and
// the inputs with what each carries. Everything else is on the panel, which is where a reader lands
// the moment this closes.
//
// **Only its two buttons close it** (`AMB-T-5363`). A press on the backdrop or Escape would throw away
// a prompt half written, and nothing here keeps it — so neither is a way out, and the reader leaves
// by putting the step in or by giving it up.
import { useState } from "react";
import { createPortal } from "react-dom";
import { addAutomationStep, insertAutomationActionStep } from "../core/automations";
import { t } from "../core/i18n";
import { Icon } from "../components/Icon";
import { PORT_KINDS } from "./automationPortKinds";

/** One input being written, before it is anything core knows about. */
type Draft = { name: string; kind: string; required: boolean };

/** Where the new step goes: onto a line inside the action, or onto an action with no line yet. */
export type AddTarget = { picture: "action"; edgeId: number } | { picture: "action"; actionId: number };

export function AutomationStepAdd({
  into,
  agent,
  onClose,
}: {
  into: AddTarget;
  /** What the step this line leaves is carried out by, which is the likeliest answer for the new one. */
  agent: string;
  onClose: () => void;
}) {
  const [name, setName] = useState("");
  const [prompt, setPrompt] = useState("");
  const [interactive, setInteractive] = useState(false);
  const [exits, setExits] = useState<string[]>([]);
  const [inputs, setInputs] = useState<Draft[]>([]);

  const ready = name.trim() !== "" && prompt.trim() !== "";
  const put = () => {
    if (!ready) return;
    const declared = {
      exits: exits.map((one) => one.trim()).filter((one) => one !== ""),
      inputs: inputs
        .filter((one) => one.name.trim() !== "")
        .map((one) => ({ ...one, name: one.name.trim() })),
    };
    const step = { name: name.trim(), prompt: prompt.trim(), agent, interactive, ...declared };
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
          {t("auto.act.addTitle")}
        </h2>

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

        <div className="autostep__field">
          <span className="autostep__label">{t("auto.step.exits")}</span>
          {exits.map((one, nth) => (
            <div key={nth} className="autodlg__row">
              <input
                placeholder={t("auto.add.exitPh")}
                value={one}
                onChange={(e) =>
                  setExits(exits.map((was, i) => (i === nth ? e.target.value : was)))
                }
              />
              <button
                type="button"
                className="btn"
                aria-label={t("auto.add.drop")}
                onClick={() => setExits(exits.filter((_, i) => i !== nth))}
              >
                <Icon name="close" />
              </button>
            </div>
          ))}
          <button type="button" className="btn" onClick={() => setExits([...exits, ""])}>
            {t("auto.add.exitAdd")}
          </button>
        </div>

        <div className="autostep__field">
          <span className="autostep__label">{t("auto.step.inputs")}</span>
          {inputs.map((one, nth) => (
            <div key={nth} className="autodlg__row">
              <input
                placeholder={t("auto.add.namePh")}
                value={one.name}
                onChange={(e) =>
                  setInputs(inputs.map((was, i) => (i === nth ? { ...was, name: e.target.value } : was)))
                }
              />
              <select
                value={one.kind}
                onChange={(e) =>
                  setInputs(inputs.map((was, i) => (i === nth ? { ...was, kind: e.target.value } : was)))
                }
              >
                {PORT_KINDS.map((kind) => (
                  <option key={kind.id} value={kind.id}>
                    {kind.label()}
                  </option>
                ))}
              </select>
              <select
                value={one.required ? "yes" : "no"}
                onChange={(e) =>
                  setInputs(
                    inputs.map((was, i) =>
                      i === nth ? { ...was, required: e.target.value === "yes" } : was,
                    ),
                  )
                }
              >
                <option value="yes">{t("auto.add.required")}</option>
                <option value="no">{t("auto.add.optional")}</option>
              </select>
              <button
                type="button"
                className="btn"
                aria-label={t("auto.add.drop")}
                onClick={() => setInputs(inputs.filter((_, i) => i !== nth))}
              >
                <Icon name="close" />
              </button>
            </div>
          ))}
          <button
            type="button"
            className="btn"
            onClick={() => setInputs([...inputs, { name: "", kind: "value", required: true }])}
          >
            {t("auto.add.inputAdd")}
          </button>
        </div>

        <label className="autostep__check">
          <input
            type="checkbox"
            checked={interactive}
            onChange={(e) => setInteractive(e.target.checked)}
          />
          {t("auto.step.interactive")}
        </label>

        <div className="buttonrow">
          <button type="button" className="btn btn--primary" disabled={!ready} onClick={put}>
            {t("auto.add.put")}
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
