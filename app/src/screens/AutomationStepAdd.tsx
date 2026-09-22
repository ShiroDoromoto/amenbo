// **Put a step in on a line** (`AMB-T-5257`), asked from the `+` on that line.
//
// **There is no "add at the end".** A step nothing points at is a step no run reaches, so the only
// road in is a line that already goes somewhere: the way out that was pressed comes to point at the
// new step, and the new step goes on to whatever that way out used to reach
// (`amenbo_core::ops::automation::step_insert`).
//
// **A step either runs a library action or carries a prompt written here**, which is the same one
// control the step panel puts it on (`./AutomationStepPanel`). One that runs an action declares
// nothing of its own — its ways out and its inputs are the action's — so those two sections are not
// drawn for it rather than drawn and refused.
//
// **What it declares is what a dialog can take without becoming a screen**: the named ways out, and
// the inputs with what each carries. Everything else a step holds is on the panel, which is where a
// reader lands the moment this closes.
import { useState } from "react";
import { createPortal } from "react-dom";
import { insertAutomationStep, useAutomationActions } from "../core/automations";
import { t } from "../core/i18n";
import { Icon } from "../components/Icon";
import { PORT_KINDS } from "./automationPortKinds";

/** One input being written, before it is anything core knows about. */
type Draft = { name: string; kind: string; required: boolean };

export function AutomationStepAdd({
  edgeId,
  projectId,
  agent,
  onClose,
}: {
  /** The line the `+` was on — what the new step is put in front of. */
  edgeId: number;
  projectId: number | null;
  /** What the step this line leaves is carried out by, which is the likeliest answer for the new one. */
  agent: string;
  onClose: () => void;
}) {
  const actions = useAutomationActions(projectId);
  const [name, setName] = useState("");
  const [action, setAction] = useState<string>("");
  const [prompt, setPrompt] = useState("");
  const [interactive, setInteractive] = useState(false);
  const [exits, setExits] = useState<string[]>([]);
  const [inputs, setInputs] = useState<Draft[]>([]);
  const own = action === "";

  const ready = name.trim() !== "" && (!own || prompt.trim() !== "");
  const put = () => {
    if (!ready) return;
    void insertAutomationStep(edgeId, {
      name: name.trim(),
      source: own ? { prompt: prompt.trim() } : { action: Number(action) },
      agent,
      interactive,
      exits: own ? exits.map((one) => one.trim()).filter((one) => one !== "") : [],
      inputs: own ? inputs.filter((one) => one.name.trim() !== "").map((one) => ({ ...one, name: one.name.trim() })) : [],
    });
    onClose();
  };

  return createPortal(
    <div className="modal__overlay" onClick={(e) => e.target === e.currentTarget && onClose()}>
      <div
        className="modal__card modal__card--wide"
        role="dialog"
        aria-modal="true"
        aria-labelledby="auto-add-title"
      >
        <h2 className="autodlg__title" id="auto-add-title">{t("auto.add.title")}</h2>

        <label className="autostep__field">
          <span className="autostep__label">{t("auto.step.name")}</span>
          <input autoFocus value={name} onChange={(e) => setName(e.target.value)} />
        </label>

        <label className="autostep__field">
          <span className="autostep__label">{t("auto.step.source")}</span>
          <select value={action} onChange={(e) => setAction(e.target.value)}>
            <option value="">{t("auto.step.sourceOwn")}</option>
            {actions.map((one) => (
              <option key={one.id} value={String(one.id)}>
                {one.name}
              </option>
            ))}
          </select>
        </label>

        {own && (
          <label className="autostep__field">
            <span className="autostep__label">{t("auto.step.prompt")}</span>
            <textarea
              className="autostep__prompt"
              rows={5}
              value={prompt}
              onChange={(e) => setPrompt(e.target.value)}
            />
          </label>
        )}

        {own && (
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
        )}

        {own && (
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
        )}

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
