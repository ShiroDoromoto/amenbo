// **Make an action on the spot, from an automation's picture** (`AMB-D-956`) — what the library
// panel's "make one" press opens (`./AutomationLibraryPanel`).
//
// **It asks two things, under a small picture of where the action goes: a name, and which library
// it is kept in.** No sentence says what the press does: the picture says where, the button says
// "make and open", and an empty action is named by the launch check and marked on its box. The inside of an action
// is its steps, and each step carries its own prompt, ways out and outputs — so it is built on the
// action's own screen. A dialog that asked for part of it here would be a second place to declare
// the same thing (`AMB-D-954`).
//
// **The press puts the empty action where it was asked for and opens it to be built.** Where it stands is
// decided now, by the line pressed, so the reader does not have to remember it
// while they build the inside; they come back to find it standing there. Until a step is written in
// it, the launch check names the action as empty.
//
// **Which library is asked, not assumed** (`AMB-T-5317`): an action made here is an ordinary action,
// and where one is kept outlives the picture it was made at. The two are the reach chips' words, side
// by side, with this project's first; with no project there is only the device's, and nothing to ask.
//
// **The name starts as what the library was searched with** — the reader looked for it, did not
// find it, and pressed to make it under that name.
//
// **Only its two buttons close it** (`AMB-T-5363`): a stray press on the backdrop or Escape would
// throw away what was typed.
import { useState } from "react";
import { createPortal } from "react-dom";
import { makeAutomationAction, type ActionShelf } from "../core/automations";
import { errText, t } from "../core/i18n";
import { asTyped, isEnterSubmit } from "../core/keys";
import { ErrorNote } from "../components/ErrorNote";
import { WhereMark, type WhereTo } from "./automationParts";

/** The two libraries an action made here can be kept in, this project's first. */
const SHELVES: { id: ActionShelf; label: () => string }[] = [
  { id: "project", label: () => t("auto.actions.reachProject") },
  { id: "device", label: () => t("auto.actions.reachGlobal") },
];

export function AutomationActionMake({
  into,
  name: typed = "",
  where,
  projectId,
  onMade,
  onClose,
}: {
  /**
   * Where the new action is placed: on the line pressed. A picture with nothing on it takes one of the
   * built-ins a run starts at, never an action made here (`AMB-D-977`).
   */
  into: { edgeId: number };
  /** What the name box starts with. */
  name?: string;
  /** Where the new action goes, drawn over the fields. */
  where: WhereTo;
  projectId: number | null;
  /** The action is placed — go and build it. */
  onMade: (actionId: number) => void;
  onClose: () => void;
}) {
  const [name, setName] = useState(typed);
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
      const id = await makeAutomationAction(into.edgeId, name.trim(), shelfPicked);
      onClose();
      if (id !== null) onMade(id);
    } catch (e) {
      setRefused(errText(e));
      setMaking(false);
    }
  };

  return createPortal(
    <div className="modal__overlay">
      <div
        className="modal__card autodlg__stack"
        role="dialog"
        aria-modal="true"
        aria-labelledby="auto-make-title"
      >
        <h2 className="autodlg__title" id="auto-make-title">
          {t("auto.lib.make")}
        </h2>

        {refused !== null && <ErrorNote tone="quiet">{refused}</ErrorNote>}

        <WhereMark where={where} />

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
          <div className="buttonrow" role="group" aria-label={t("auto.actions.reach")}>
            {SHELVES.map((one) => (
              <button
                key={one.id}
                type="button"
                className={shelf === one.id ? "actchip actchip--on" : "actchip"}
                aria-pressed={shelf === one.id}
                onClick={() => setShelf(one.id)}
              >
                {one.label()}
              </button>
            ))}
          </div>
        )}

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
