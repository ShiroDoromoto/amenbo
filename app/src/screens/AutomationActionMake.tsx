// **Make an action on the spot, from an automation's picture** (`AMB-D-956`) — what the library
// panel's "make one" press opens (`./AutomationLibraryPanel`).
//
// **It asks two things: a name, and which library the action is kept in.** The inside of an action
// is its steps, and each step carries its own prompt, ways out and outputs — so it is built on the
// action's own screen. A dialog that asked for part of it here would be a second place to declare
// the same thing (`AMB-D-954`).
//
// **The press puts the empty action where it was asked for and goes to build it.** Where it stands is
// decided now, by the line pressed or the empty picture, so the reader does not have to remember it
// while they build the inside; they come back to find it standing there. Until a step is written in
// it, the launch check names the action as empty.
//
// **Which library is asked, not assumed** (`AMB-T-5317`): an action made here is an ordinary action,
// and where one is kept outlives the picture it was made at. This project's is offered first; with no
// project there is only the device's.
//
// **Only its two buttons close it** (`AMB-T-5363`): a stray press on the backdrop or Escape would
// throw away what was typed.
import { useState } from "react";
import { createPortal } from "react-dom";
import { makeAutomationAction, type ActionShelf } from "../core/automations";
import { errText, t } from "../core/i18n";
import { asTyped, isEnterSubmit } from "../core/keys";
import { ErrorNote } from "../components/ErrorNote";

export function AutomationActionMake({
  into,
  projectId,
  onMade,
  onClose,
}: {
  /** Where the new action is placed: on the line pressed, or on a picture with no line yet. */
  into: { edgeId: number } | { automationId: number };
  projectId: number | null;
  /** The action is placed — go and build it. */
  onMade: (actionId: number) => void;
  onClose: () => void;
}) {
  const [name, setName] = useState("");
  const [shelf, setShelf] = useState<ActionShelf>("project");
  const [making, setMaking] = useState(false);
  const [refused, setRefused] = useState<string | null>(null);
  const shelfPicked = projectId === null ? "device" : shelf;
  const ready = name.trim() !== "" && !making;

  const make = async () => {
    if (!ready) return;
    setMaking(true);
    setRefused(null);
    try {
      const id = await makeAutomationAction(into, name.trim(), shelfPicked);
      onClose();
      if (id !== null) onMade(id);
    } catch (e) {
      setRefused(errText(e));
      setMaking(false);
    }
  };

  return createPortal(
    <div className="modal__overlay">
      <div className="modal__card" role="dialog" aria-modal="true" aria-labelledby="auto-make-title">
        <h2 className="autodlg__title" id="auto-make-title">
          {t("auto.lib.make")}
        </h2>

        {refused !== null && <ErrorNote tone="quiet">{refused}</ErrorNote>}

        <label className="autostep__field">
          <span className="autostep__label">{t("auto.actions.name")}</span>
          <input
            {...asTyped}
            autoFocus
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => { if (isEnterSubmit(e)) void make(); }}
          />
        </label>

        {projectId !== null && (
          <label className="autostep__field">
            <span className="autostep__label">{t("auto.actions.reach")}</span>
            <select value={shelf} onChange={(e) => setShelf(e.target.value as ActionShelf)}>
              <option value="project">{t("auto.actions.reachProject")}</option>
              <option value="device">{t("auto.actions.reachGlobal")}</option>
            </select>
          </label>
        )}

        <p className="autostep__said">{t("auto.make.said")}</p>

        <div className="buttonrow">
          <button type="button" className="btn btn--primary" disabled={!ready} onClick={() => void make()}>
            {t("auto.make.go")}
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
