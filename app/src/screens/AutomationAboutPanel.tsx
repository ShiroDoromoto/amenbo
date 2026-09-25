// What the automation itself holds — its name, its notes, whether it is kept out of the way, and the
// one press that takes the whole definition away.
//
// **It opens in the build screen's panel, from "Edit" on the head** (`./AutomationBuildScreen`). What a
// reader opens a definition for is the picture and the spot they are about to fix; everything here is
// reached once — when something is renamed, put away, or is not wanted any more. Stacked under the
// picture it would move further off with every placement; in the panel it is in the same spot however
// long the picture runs.
//
// **The panel's head is the name, and the name is written there** (`AutomationNameField`). A head
// naming the automation over a field holding the same name would say it twice, on top of the build
// screen's own head.
//
// **A field writes when the caret leaves it**, the way the step panel beside it does
// (`./AutomationStepPanel`): there is no Save on this screen, so a definition never carries a change
// that looks made and is not. A refusal is drawn once, at the top, in the words core wrote.
//
// **The ID is shown as the line it goes into.** It is what the terminal names the definition by and it
// never changes, so the panel draws `amenbo automation start <id>` with a press that copies it — what
// the number is for is read off the command, and no sentence has to say so.
//
// **Archiving is a switch and not an action.** It takes nothing away and stops nothing already
// running (`amenbo_core::ops::automation::update`) — the row goes to the fold at the end of the list —
// so it saves the way the name does, and a switch says by its shape that it turns back.
//
// **Deleting is the opposite, so it goes through the machine's own confirm** and names what goes
// with it. Core refuses it while a run stands behind it, saying how many
// (`amenbo_core::ops::automation::delete`); that sentence is drawn rather than re-asked here, so the
// screen and the store cannot come to disagree about when a definition can go.
import { useEffect, useState } from "react";
import { Switch } from "./automationParts";
import { deleteAutomation, editAutomation } from "../core/automations";
import { confirmDialog } from "../core/dialog";
import { asTyped } from "../core/keys";
import { errText, t } from "../core/i18n";
import { ErrorNote } from "../components/ErrorNote";
import type { AutomationDetailDto } from "../bindings/bindings";

/**
 * A box of text that writes when the caret leaves it rather than a letter at a time. The step panel
 * keeps the same one for the same reason (`./AutomationStepPanel`): what is being typed is held
 * here, and a definition re-read underneath — because a step was edited, or another window wrote —
 * puts the stored value back in the box.
 */
function useDraft(value: string): [string, (next: string) => void] {
  const [draft, setDraft] = useState(value);
  useEffect(() => setDraft(value), [value]);
  return [draft, setDraft];
}

/**
 * **The name, as the panel's head.** It writes when the caret leaves it, like every field under it.
 *
 * A blank name is not written: core refuses one, and the box goes back to the stored name rather than
 * drawing that refusal. Any other refusal is drawn under the box, since the head stands outside the
 * body where the panel's refusal is.
 *
 * `readOnly` shuts it while a run holds the definition — the head is outside the panel's fieldset, so
 * the fieldset does not shut it.
 */
export function AutomationNameField({
  automation,
  readOnly = false,
}: {
  automation: AutomationDetailDto;
  readOnly?: boolean;
}) {
  const [name, setName] = useDraft(automation.name);
  const [refused, setRefused] = useState<string | null>(null);

  const commit = () => {
    if (name.trim() === "" || name === automation.name) {
      setName(automation.name);
      return;
    }
    setRefused(null);
    editAutomation(automation.id, { name }).catch((e: unknown) => {
      setName(automation.name);
      setRefused(errText(e));
    });
  };

  return (
    <span className="actpanel__titlefield">
      <input
        {...asTyped}
        className="actpanel__titleinput"
        aria-label={t("auto.about.name")}
        value={name}
        disabled={readOnly}
        onChange={(e) => setName(e.target.value)}
        onBlur={commit}
      />
      {refused !== null && <span className="actpanel__refused" role="alert">{refused}</span>}
    </span>
  );
}

export function AutomationAboutPanel({
  automation,
  onDeleted,
}: {
  automation: AutomationDetailDto;
  /** Where to go once the definition is gone — there is no screen left to stand this one on. */
  onDeleted: () => void;
}) {
  const [notes, setNotes] = useDraft(automation.notes);
  const [refused, setRefused] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const command = `amenbo automation start ${automation.id}`;

  const run = (write: Promise<void>): Promise<void> => {
    setRefused(null);
    return write.catch((e: unknown) => setRefused(errText(e)));
  };

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(command);
      setCopied(true);
      setTimeout(() => setCopied(false), 1200);
    } catch { /* where the clipboard is unavailable, quietly skip */ }
  };

  // Physical, and it takes every step with it, so the confirm comes before the write and core's
  // refusal is drawn rather than swallowed as though the definition were already gone.
  const remove = async () => {
    if (!(await confirmDialog(t("auto.about.removeConfirm")))) return;
    setRefused(null);
    try {
      await deleteAutomation(automation.id);
      onDeleted();
    } catch (e) {
      setRefused(errText(e));
    }
  };

  return (
    <div className="autostep">
      {refused !== null && <ErrorNote tone="quiet">{refused}</ErrorNote>}

      <label className="autostep__field">
        <span className="autostep__label">{t("auto.about.notes")}</span>
        <textarea
          {...asTyped}
          rows={3}
          placeholder={t("auto.about.notesHint")}
          value={notes}
          onChange={(e) => setNotes(e.target.value)}
          onBlur={() =>
            notes !== automation.notes && void run(editAutomation(automation.id, { notes }))
          }
        />
      </label>

      <div className="autoabout__cli">
        <span className="autostep__label">{t("auto.about.cli")}</span>
        <code className="autoabout__command">{command}</code>
        <button type="button" className="btn" onClick={() => void copy()}>
          {t("auto.about.copy")}
        </button>
        <span className="autoabout__copied" role="status">{copied ? t("auto.about.copied") : ""}</span>
      </div>

      <label className="switchrow">
        <span>{t("auto.about.archive")}</span>
        <Switch
          checked={automation.archived}
          onChange={(next) => void run(editAutomation(automation.id, { archived: next }))}
        />
      </label>

      <div className="actpanel__foot">
        <button type="button" className="btn btn--danger" onClick={() => void remove()}>
          {t("auto.about.remove")}
        </button>
      </div>
    </div>
  );
}
