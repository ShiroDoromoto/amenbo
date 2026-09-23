// **Put a box in on a line** (`AMB-T-5257`), asked from the `+` on that line — and, on a picture with
// nothing on it, the first box of all (`AMB-T-5315`).
//
// **It serves both pictures** (`AMB-D-949`): on an automation what goes in is a placement, a spot
// with a library action standing on it; inside an action it is a step carrying its own prompt. The
// fields are the same ones either way, so what differs is the door the press goes through
// (`../core/automations`).
//
// **On a line, nothing is left running past a box.** The way out that was pressed comes to point at
// the new box, and the new box goes on to whatever that way out used to reach
// (`amenbo_core::ops::automation::placement_insert`, `…::step_insert`) — one act.
//
// **A picture with nothing on it is opened the same way.** The press on an empty picture names the
// picture instead of a line, and the box it puts down stands on its own with nothing pointing at it
// — so the first box of all is written here too, rather than only in the library tab.
//
// **On an automation it writes an action, and nothing else.** Picking one off the shelf is the
// panel's (`./AutomationLibraryPanel`), done beside the picture rather than in front of it; this is
// what the panel's "make one" press opens. A prompt typed here becomes an ordinary action and is
// placed in the same act (`AMB-T-5317`).
//
// **Where the written action is kept is asked here, not assumed** (`AMB-T-5317`). It is an ordinary
// action once written, so it lands either in the device's library, which every project on this
// machine reaches, or in this project's — the same two the library tab asks for
// (`./AutomationActionsTab`), and the choice outlives the picture it was written at.
//
// **What it declares is what a dialog can take without becoming a screen**: the named ways out, and
// the inputs with what each carries. Everything else is on the panel, which is where a reader lands
// the moment this closes.
//
// **Only its two buttons close it** (`AMB-T-5363`). A press on the backdrop or Escape would throw away
// a prompt half written, and nothing here keeps it — so neither is a way out, and the reader leaves
// by putting the box in or by giving it up.
import { useState } from "react";
import { createPortal } from "react-dom";
import {
  addAutomationStep,
  insertAutomationActionStep,
  insertAutomationStep,
  placeAutomationActionFromPrompt,
  type ActionShelf,
} from "../core/automations";
import { t } from "../core/i18n";
import { Icon } from "../components/Icon";
import { PORT_KINDS } from "./automationPortKinds";

/** One input being written, before it is anything core knows about. */
type Draft = { name: string; kind: string; required: boolean };

/**
 * Where the new box goes: onto a line of either picture, or onto a picture that has no line to press
 * yet — an automation or an action, named by itself.
 */
export type AddTarget =
  | { picture: "automation"; edgeId: number }
  | { picture: "automation"; automationId: number }
  | { picture: "action"; edgeId: number }
  | { picture: "action"; actionId: number };

export function AutomationStepAdd({
  into,
  projectId,
  agent,
  onPut,
  onClose,
}: {
  into: AddTarget;
  projectId: number | null;
  /** What the box this line leaves is carried out by, which is the likeliest answer for the new one. */
  agent: string;
  /** The box went in — told before `onClose`, which both buttons call. */
  onPut?: () => void;
  onClose: () => void;
}) {
  const [name, setName] = useState("");
  const [shelf, setShelf] = useState<ActionShelf>("project");
  const [prompt, setPrompt] = useState("");
  const [interactive, setInteractive] = useState(false);
  const [exits, setExits] = useState<string[]>([]);
  const [inputs, setInputs] = useState<Draft[]>([]);
  // Which library a written action lands in. A project's own is the likelier answer and the one
  // offered first, but with no project open the device's is the only one there is.
  const shelfPicked = projectId === null ? "device" : shelf;

  const ready = name.trim() !== "" && prompt.trim() !== "";
  const put = () => {
    if (!ready) return;
    const declared = {
      exits: exits.map((one) => one.trim()).filter((one) => one !== ""),
      inputs: inputs
        .filter((one) => one.name.trim() !== "")
        .map((one) => ({ ...one, name: one.name.trim() })),
    };
    if (into.picture === "automation") {
      void ("edgeId" in into
        ? insertAutomationStep(into.edgeId, {
            name: name.trim(),
            source: { prompt: prompt.trim(), shelf: shelfPicked },
            agent,
            interactive,
            ...declared,
          })
        : placeAutomationActionFromPrompt(into.automationId, {
            name: name.trim(),
            prompt: prompt.trim(),
            shelf: shelfPicked,
            agent,
            interactive,
            ...declared,
          }));
    } else {
      const step = { name: name.trim(), prompt: prompt.trim(), agent, interactive, ...declared };
      void ("edgeId" in into
        ? insertAutomationActionStep(into.edgeId, step)
        : addAutomationStep(into.actionId, step));
    }
    onPut?.();
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
          {into.picture === "action" ? t("auto.act.addTitle") : t("auto.add.title")}
        </h2>

        <label className="autostep__field">
          <span className="autostep__label">{t("auto.step.name")}</span>
          <input autoFocus value={name} onChange={(e) => setName(e.target.value)} />
        </label>

        {into.picture === "automation" && projectId !== null && (
          <label className="autostep__field">
            <span className="autostep__label">{t("auto.actions.reach")}</span>
            <select value={shelf} onChange={(e) => setShelf(e.target.value as ActionShelf)}>
              <option value="project">{t("auto.actions.reachProject")}</option>
              <option value="device">{t("auto.actions.reachDevice")}</option>
            </select>
          </label>
        )}

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
