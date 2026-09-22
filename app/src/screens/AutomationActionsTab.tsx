// The library, as the "actions" tab draws it — every prompt a step in this project can be pointed
// at, and what rewriting one would reach.
//
// **One list, two reaches.** An action lives either in the device's own library, which every project
// on this machine reaches, or in one project's. They are drawn as one list with a column saying
// which, because what a reader is choosing between is the prompts themselves: splitting them into
// two lists would make them go and look in both to answer "is this already in here".
//
// **The count is the whole warning.** A prompt rewritten here is carried by every step pointing at
// this action, so each row says how many automations that is before the box is opened. It counts
// automations rather than steps: two steps of one automation is one automation whose runs change,
// and what a reader weighs is how far the rewrite carries, not how often the pointer occurs.
//
// **A row opens into the action build screen** (`AMB-T-5315`), where its steps are drawn and its
// prompts written. Making one here asks for a name and a reach and no prompt: an action is born
// empty, and the screen the press lands on is where the words go.
import { useEffect, useState } from "react";
import { addAutomationAction, useAutomationActions } from "../core/automations";
import { asTyped } from "../core/keys";
import { errText, t, tn } from "../core/i18n";
import { ErrorNote } from "../components/ErrorNote";

export function AutomationActionsTab({
  projectId,
  onOpen,
}: {
  projectId: number | null;
  /** Open the build screen on this action — a press on a row, and on the row a press just made. */
  onOpen: (id: number) => void;
}) {
  const actions = useAutomationActions(projectId);
  // The ids the library held when Make was pressed, while the row that was made is still on its way.
  // Nothing while none is.
  const [born, setBorn] = useState<ReadonlySet<number> | null>(null);

  // **The new row is the one the library did not hold before.** A write answers with an ack — the
  // ids it touched and what to re-read, never a body — so which row was made is read off the list
  // that came back, the way a new project is picked out of the refreshed snapshot
  // (`core/mutations.createProject`).
  useEffect(() => {
    if (born === null) return;
    const fresh = actions.find((one) => !born.has(one.id));
    if (fresh === undefined) return;
    setBorn(null);
    onOpen(fresh.id);
  }, [actions, born, onOpen]);

  async function make(name: string, project: number | null) {
    const before = new Set(actions.map((one) => one.id));
    await addAutomationAction(name, project);
    setBorn(before);
  }

  return (
    <>
      <ActionAdd projectId={projectId} onMake={make} />
      {actions.length === 0 ? (
        <div className="auto__empty">{t("auto.actions.empty")}</div>
      ) : (
        <ul className="auto__list">
          {actions.map((one) => (
            <li key={one.id}>
              <button type="button" className="auto__row" onClick={() => onOpen(one.id)}>
                <span className="auto__name">{one.name}</span>
                <span className="auto__mark">
                  {one.global ? t("auto.actions.reachDevice") : t("auto.actions.reachProject")}
                </span>
                <span className="auto__steps">
                  {one.usedBy === 0
                    ? t("auto.actions.unused")
                    : tn("auto.actions.usedBy", one.usedBy)}
                </span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </>
  );
}

/**
 * **Make an action from the list itself**, which until now could only be done from the CLI or by
 * raising a step that already carried the words (`AutomationStepPanel`).
 *
 * **It asks for a name and a reach, and no prompt.** The prompt belongs to a step inside the action,
 * and a field here would be writing one before there is a step to write it on — so what this makes
 * is the row, and the press lands in the build screen on it (`./AutomationActionBuildScreen`).
 *
 * **The reach is asked for rather than defaulted.** The device's library is reached by every project
 * on this machine and the project's by one, and moving an action between them is not something this
 * screen does — so it is a choice made here rather than one a reader finds out about later.
 *
 * It stands over the list whether or not the library has anything in it: an empty library is exactly
 * where the way to fill it has to be.
 */
function ActionAdd({
  projectId,
  onMake,
}: {
  projectId: number | null;
  onMake: (name: string, project: number | null) => Promise<void>;
}) {
  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  const [wide, setWide] = useState(false);
  const [making, setMaking] = useState(false);
  const [error, setError] = useState<string | null>(null);

  if (!open) {
    return (
      <button type="button" className="btn" onClick={() => setOpen(true)}>
        {t("auto.actions.add")}
      </button>
    );
  }

  const make = async () => {
    setError(null);
    setMaking(true);
    try {
      await onMake(name.trim(), wide ? null : projectId);
      setName("");
      setOpen(false);
    } catch (err) {
      setError(errText(err));
    } finally {
      setMaking(false);
    }
  };

  return (
    <div className="settings__form">
      <label className="field">
        <span className="fieldlabel">{t("auto.actions.name")}</span>
        <input {...asTyped} value={name} onChange={(e) => setName(e.target.value)} />
      </label>
      <label className="field">
        <span className="fieldlabel">{t("auto.actions.reach")}</span>
        <select
          value={wide ? "device" : "project"}
          onChange={(e) => setWide(e.target.value === "device")}
        >
          <option value="project">{t("auto.actions.reachProject")}</option>
          <option value="device">{t("auto.actions.reachDevice")}</option>
        </select>
      </label>
      <div className="settings__row">
        <button
          type="button"
          className="btn btn--primary"
          disabled={making || name.trim() === "" || (!wide && projectId === null)}
          onClick={() => void make()}
        >
          {t("auto.actions.add")}
        </button>
        <button type="button" className="btn" onClick={() => setOpen(false)}>
          {t("auto.actions.cancel")}
        </button>
      </div>
      {error !== null && <ErrorNote>{error}</ErrorNote>}
    </div>
  );
}
