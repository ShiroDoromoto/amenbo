// **What a person hands a run as they start it** (`AMB-D-970`): a text and files, asked in a dialog
// that opens on every press that starts an automation, before the run exists.
//
// **Before the launch, not in the run's pane.** What is handed over is written onto the run in the
// launch's own transaction (`amenbo_core::ops::automation_run::HandedAtLaunch`), so the first step
// never opens on a run it has not reached yet — and there is no run to draw a pane for until then.
//
// **Handing nothing is a start like any other.** Most automations take their task from the list and
// are handed nothing, so both fields start empty and the press that starts is live from the first.
//
// **A file is named by its path**, picked in the machine's own panel, the way an attachment is: the
// host reads it at the press, checks it against the per-file cap and keeps it (`crate::automation`).
import { useState } from "react";
import { createPortal } from "react-dom";
import { pickFiles } from "../core/dialog";
import { t, tf } from "../core/i18n";
import { asTyped } from "../core/keys";
import { Icon } from "./Icon";

/** What was handed over, as the launch takes it. */
export type Handed = { text: string; files: string[] };

/** The last part of a path, which is what a reader knows a file by. */
function baseName(path: string): string {
  return path.split(/[\\/]/).filter((part) => part !== "").pop() ?? path;
}

export function LaunchHanding({
  name,
  onStart,
  onClose,
}: {
  /** The automation about to be started, as its row names it. */
  name: string;
  onStart: (handed: Handed) => void;
  onClose: () => void;
}) {
  const [text, setText] = useState("");
  const [files, setFiles] = useState<string[]>([]);

  const add = async () => {
    const picked = await pickFiles();
    if (picked.length > 0) setFiles((had) => [...had, ...picked.filter((one) => !had.includes(one))]);
  };

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

        <div className="buttonrow">
          <button
            type="button"
            className="btn btn--primary"
            onClick={() => onStart({ text: text.trim(), files })}
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
