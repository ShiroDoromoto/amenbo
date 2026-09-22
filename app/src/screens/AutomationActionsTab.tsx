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
// **This is the only place a prompt is written.** A step that points at an action holds no copy of
// its own (`amenbo_core::ops::automation_run`), so there is nowhere else the same words could be
// edited into two versions.
import { useState } from "react";
import { editAutomationAction, useAutomationActions } from "../core/automations";
import { asTyped } from "../core/keys";
import { t, tn } from "../core/i18n";
import type { AutomationActionCardDto } from "../bindings/bindings";

export function AutomationActionsTab({ projectId }: { projectId: number | null }) {
  const actions = useAutomationActions(projectId);
  // Which row is open for editing, or nothing. One at a time: the box is opened in place of the row,
  // and two open boxes would be two half-written prompts with one Save each to keep straight.
  const [open, setOpen] = useState<number | null>(null);

  if (actions.length === 0) return <div className="auto__empty">{t("auto.actions.empty")}</div>;

  return (
    <ul className="auto__list">
      {actions.map((one) =>
        one.id === open ? (
          <li key={one.id}>
            <ActionEdit action={one} onDone={() => setOpen(null)} />
          </li>
        ) : (
          <li key={one.id}>
            <button type="button" className="auto__row" onClick={() => setOpen(one.id)}>
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
        ),
      )}
    </ul>
  );
}

/**
 * One action, open for editing. The name and the prompt are held here while they are being typed and
 * written on Save — the row underneath goes on reading the store, so a box left open while somebody
 * else rewrites the same action does not lose what is being typed into it.
 *
 * The fields take `asTyped` like every other box in the app. A prompt is prose, which is the one
 * place a spell checker would have something to offer — but it is prose carrying refs, paths and
 * command words, and an autocorrect that rewrites one of those changes what an agent is told to do.
 */
function ActionEdit({
  action,
  onDone,
}: {
  action: AutomationActionCardDto;
  onDone: () => void;
}) {
  const [name, setName] = useState(action.name);
  const [prompt, setPrompt] = useState(action.prompt);

  async function save() {
    await editAutomationAction(action.id, { name, prompt });
    onDone();
  }

  return (
    <div className="settings__form">
      <label className="field">
        <span className="fieldlabel">{t("auto.actions.name")}</span>
        <input {...asTyped} value={name} onChange={(e) => setName(e.target.value)} />
      </label>
      <label className="field">
        <span className="fieldlabel">{t("auto.actions.prompt")}</span>
        <textarea
          {...asTyped}
          rows={6}
          value={prompt}
          onChange={(e) => setPrompt(e.target.value)}
        />
      </label>
      <div className="auto__reaches">{t("auto.actions.reaches")}</div>
      <div className="settings__row">
        <button type="button" className="btn btn--primary" onClick={save}>
          {t("auto.actions.save")}
        </button>
        <button type="button" className="btn" onClick={onDone}>
          {t("auto.actions.cancel")}
        </button>
      </div>
    </div>
  );
}
