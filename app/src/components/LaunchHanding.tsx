// **What a test run is handed to walk from** (`AMB-T-5804`), asked in a dialog that opens on the
// build screen's "test run" press, before anything is walked.
//
// **A launch asks nothing.** Work reaches a run as a task filed under one of its roles, and the
// launch's press only opens the run (`AMB-D-1123`, `./StartAutomation`). This dialog is the test
// run's alone.
//
// **It asks for what core says the entry reads, and nothing else** (`automation_launch_asks`). Where
// that is a task, it asks the task's title, its notes and a value on each axis offered; where it is
// nothing, it says so and the press walks at once. There is no field for words on their own: words for
// the task go in its notes, so there is one place to write them, not two. No files are asked: nothing
// a test run writes is kept, so there is no task for them to go onto.
//
// **A task cannot be filed without a title**, nor without a value on an axis the project requires, so
// where the entry reads a task the press waits for both — the same two things the walk would refuse
// over.
import { useState } from "react";
import { createPortal } from "react-dom";
import type { AutomationLaunchAsksDto } from "../bindings/bindings";
import { NOTHING_HANDED, useLaunchAsks, type Handed } from "../core/automations";
import { t, tf } from "../core/i18n";
import { asTyped } from "../core/keys";

export function LaunchHanding({
  id,
  name,
  onStart,
  onClose,
}: {
  /** The automation about to be walked. */
  id: number;
  /** Its name, as its row names it. */
  name: string;
  onStart: (handed: Handed) => void;
  onClose: () => void;
}) {
  const asks = useLaunchAsks(id);
  const [title, setTitle] = useState("");
  const [notes, setNotes] = useState("");
  // The value chosen on each axis, by the axis's name. An axis left out is not chosen.
  const [chosen, setChosen] = useState<Record<string, string>>({});

  // What the press hands over: only what the entry reads.
  const handed = (reads: AutomationLaunchAsksDto["reads"]): Handed => {
    switch (reads) {
      case "task":
        return {
          ...NOTHING_HANDED,
          title: title.trim(),
          notes: notes.trim(),
          classification: Object.entries(chosen).filter(([, value]) => value !== ""),
        };
      case "nothing":
        return NOTHING_HANDED;
    }
  };
  const ready =
    asks !== null &&
    (asks.reads !== "task" ||
      (title.trim() !== "" && asks.axes.every((axis) => !axis.required || (chosen[axis.name] ?? "") !== "")));

  return createPortal(
    <div className="modal__overlay">
      <div
        className="modal__card modal__card--wide launchhand"
        role="dialog"
        aria-modal="true"
        aria-labelledby="launch-hand-title"
        onKeyDown={(e) => {
          if (e.key === "Escape") onClose();
        }}
      >
        <h2 className="autodlg__title" id="launch-hand-title">
          {tf("auto.test.title", { name })}
        </h2>
        <p className="autostep__said">{t("auto.test.about")}</p>

        {asks?.reads === "task" && (
          <>
            <label className="autostep__field">
              <span className="autostep__label">
                {t("auto.hand.taskTitle")}
                <span className="autostep__req">{t("auto.hand.required")}</span>
              </span>
              <input
                {...asTyped}
                autoFocus
                className="launchhand__title"
                value={title}
                onChange={(e) => setTitle(e.target.value)}
              />
            </label>

            <label className="autostep__field">
              <span className="autostep__label">{t("auto.hand.taskNotes")}</span>
              <textarea
                {...asTyped}
                className="autostep__prompt"
                rows={5}
                value={notes}
                onChange={(e) => setNotes(e.target.value)}
              />
            </label>

            {asks.axes.map((axis) => (
              <label key={axis.name} className="autostep__field">
                <span className="autostep__label">
                  {axis.name}
                  {axis.required && <span className="autostep__req">{t("auto.hand.required")}</span>}
                </span>
                <select
                  className="launchhand__axis"
                  value={chosen[axis.name] ?? ""}
                  onChange={(e) => {
                    const value = e.target.value;
                    setChosen((had) => ({ ...had, [axis.name]: value }));
                  }}
                >
                  <option value="">{axis.required ? "—" : t("auto.hand.unchosen")}</option>
                  {axis.values.map((value) => (
                    <option key={value} value={value}>
                      {value}
                    </option>
                  ))}
                </select>
              </label>
            ))}
          </>
        )}

        {asks?.reads === "nothing" && <p className="autostep__said">{t("auto.hand.nothing")}</p>}

        <div className="buttonrow">
          <button
            type="button"
            className="btn btn--primary"
            disabled={!ready}
            onClick={() => {
              if (asks !== null) onStart(handed(asks.reads));
            }}
          >
            {t("auto.test.run")}
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
