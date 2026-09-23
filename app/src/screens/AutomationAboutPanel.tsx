// What the automation itself holds — its name, its notes, whether it is kept out of the way, and the
// one press that takes the whole definition away.
//
// **It opens in the build screen's panel, from "Edit" on the head** (`./AutomationBuildScreen`). What a
// reader opens a definition for is the picture and the spot they are about to fix; the name is
// already drawn on the head of the screen, and everything here is reached once — when something is
// renamed, put away, or is not wanted any more. Stacked under the picture it would move further off
// with every placement; in the panel it is in the same spot however long the picture runs.
//
// **A field writes when the caret leaves it**, the way the step panel beside it does
// (`./AutomationStepPanel`): there is no Save on this screen, so a definition never carries a change
// that looks made and is not. A refusal is drawn once, at the top, in the words core wrote.
//
// **Archiving is a field and not an action.** It takes nothing away and stops nothing already
// running (`amenbo_core::ops::automation::update`) — the row stays in the list carrying the mark —
// so it saves the way the name does.
//
// **Deleting is the opposite, so it goes through the machine's own confirm** and names what goes
// with it. Core refuses it while a run stands behind it, saying how many
// (`amenbo_core::ops::automation::delete`); that sentence is drawn rather than re-asked here, so the
// screen and the store cannot come to disagree about when a definition can go.
import { useEffect, useState } from "react";
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

export function AutomationAboutPanel({
  automation,
  onDeleted,
}: {
  automation: AutomationDetailDto;
  /** Where to go once the definition is gone — there is no screen left to stand this one on. */
  onDeleted: () => void;
}) {
  const [name, setName] = useDraft(automation.name);
  const [notes, setNotes] = useDraft(automation.notes);
  const [refused, setRefused] = useState<string | null>(null);

  const run = (write: Promise<void>): Promise<void> => {
    setRefused(null);
    return write.catch((e: unknown) => setRefused(errText(e)));
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
        <span className="autostep__label">{t("auto.about.name")}</span>
        <input
          {...asTyped}
          value={name}
          onChange={(e) => setName(e.target.value)}
          onBlur={() => name !== automation.name && void run(editAutomation(automation.id, { name }))}
        />
      </label>

      <label className="autostep__field">
        <span className="autostep__label">{t("auto.about.notes")}</span>
        <textarea
          {...asTyped}
          rows={4}
          value={notes}
          onChange={(e) => setNotes(e.target.value)}
          onBlur={() =>
            notes !== automation.notes && void run(editAutomation(automation.id, { notes }))
          }
        />
        <span className="autostep__said">{t("auto.about.notesWhat")}</span>
      </label>

      <label className="autostep__check">
        <input
          type="checkbox"
          checked={automation.archived}
          onChange={(e) => void run(editAutomation(automation.id, { archived: e.target.checked }))}
        />
        {t("auto.about.archive")}
      </label>
      <div className="autostep__said">{t("auto.about.archiveWhat")}</div>

      <div className="settings__row">
        <button type="button" className="btn btn--danger" onClick={() => void remove()}>
          {t("auto.about.remove")}
        </button>
      </div>
    </div>
  );
}
