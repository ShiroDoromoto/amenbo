// **What a person hands a run as they start it** (`AMB-D-970`), asked in a dialog that opens on every
// press that starts an automation, before the run exists.
//
// **It asks for what the entry reads, and nothing else.** Core answers what that is
// (`automation_launch_asks`): an agent's step reads a text and files; the built-in that files a task
// reads the task's title, its notes and a value on each axis offered; any other built-in reads
// nothing. A launch handing an entry what it does not read is refused, so a field the entry would
// not read is one the person could only be refused over.
//
// **Before the launch, not in the run's pane.** What is handed over is written onto the run in the
// launch's own transaction (`amenbo_core::ops::automation_run::HandedAtLaunch`), so the first step
// never opens on a run it has not reached yet — and there is no run to draw a pane for until then.
//
// **Handing nothing is a start like any other** where the entry reads words: most automations take
// their task from the list and are handed nothing, so both fields start empty and the press that
// starts is live from the first. **A task cannot be filed without a title**, nor without a value on
// an axis the project requires, so for that entry the press waits for both — the same two things the
// launch would refuse over.
//
// **A file is named by its path**, picked in the machine's own panel, the way an attachment is: the
// host reads it at the press, checks it against the per-file cap and keeps it (`crate::automation`).
import { useState } from "react";
import { createPortal } from "react-dom";
import type { AutomationLaunchAsksDto } from "../bindings/bindings";
import { NOTHING_HANDED, useLaunchAsks, type Handed } from "../core/automations";
import { pickFiles } from "../core/dialog";
import { t, tf } from "../core/i18n";
import { asTyped } from "../core/keys";
import { Icon } from "./Icon";

/** The last part of a path, which is what a reader knows a file by. */
function baseName(path: string): string {
  return path.split(/[\\/]/).filter((part) => part !== "").pop() ?? path;
}

export function LaunchHanding({
  id,
  name,
  onStart,
  onClose,
}: {
  /** The automation about to be started. */
  id: number;
  /** Its name, as its row names it. */
  name: string;
  onStart: (handed: Handed) => void;
  onClose: () => void;
}) {
  const asks = useLaunchAsks(id);
  const [text, setText] = useState("");
  const [files, setFiles] = useState<string[]>([]);
  const [title, setTitle] = useState("");
  const [notes, setNotes] = useState("");
  // The value chosen on each axis, by the axis's name. An axis left out is not chosen.
  const [chosen, setChosen] = useState<Record<string, string>>({});

  const add = async () => {
    const picked = await pickFiles();
    if (picked.length > 0) setFiles((had) => [...had, ...picked.filter((one) => !had.includes(one))]);
  };

  // What the press hands over: only what the entry reads.
  const handed = (reads: AutomationLaunchAsksDto["reads"]): Handed => {
    switch (reads) {
      case "words":
        return { ...NOTHING_HANDED, text: text.trim(), files };
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
          {tf("auto.hand.title", { name })}
        </h2>

        {asks?.reads === "words" && (
          <>
            <label className="autostep__field">
              <span className="autostep__label">{t("auto.hand.text")}</span>
              <textarea
                {...asTyped}
                autoFocus
                className="autostep__prompt"
                rows={5}
                value={text}
                onChange={(e) => setText(e.target.value)}
              />
            </label>

            <div className="autostep__field">
              <span className="autostep__label">{t("auto.hand.files")}</span>
              {files.length > 0 && (
                <ul className="launchhand__files">
                  {files.map((one) => (
                    <li key={one} className="launchhand__file" title={one}>
                      <Icon name="paperclip" />
                      <span className="launchhand__name">{baseName(one)}</span>
                      <button
                        type="button"
                        className="launchhand__drop"
                        aria-label={tf("auto.hand.fileRemove", { name: baseName(one) })}
                        onClick={() => setFiles((had) => had.filter((path) => path !== one))}
                      >
                        <Icon name="close" />
                      </button>
                    </li>
                  ))}
                </ul>
              )}
              <button type="button" className="btn launchhand__add" onClick={() => void add()}>
                {t("auto.hand.fileAdd")}
              </button>
            </div>
          </>
        )}

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
            {t("auto.start")}
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
