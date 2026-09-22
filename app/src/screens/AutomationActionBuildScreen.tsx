// One library action, opened — the screen its steps are built in (`AMB-T-5315`).
//
// **It is the automation build screen one layer down** (`AMB-D-949`). There the boxes are the actions
// placed on an automation; here they are the steps inside one action, and one step is one terminal.
// The picture, the panel and the add dialog are the same three (`./AutomationPicture`,
// `./AutomationActionStepPanel`, `./AutomationStepAdd`), handed this picture instead of that one.
//
// **There is no launch place.** What is started is an automation, and an action is what one places —
// so what this screen has in that spot is the action's own name, what it is for, its reach, and what
// a rewrite here reaches: every automation that places it. What it is for is written the way the
// automation's notes are (`./AutomationAboutPanel`), and like them it reaches no launch (`AMB-D-952`).
//
// **A step is added here, or put in on a line.** The `+` on a line is the road that leaves nothing
// pointing at nothing — but a picture has no line until two boxes are joined, so the press above it
// adds a step on its own: the first one, which an empty action takes as the step a placement opens
// (`../core/automations`), and every later one a reader then says what leads to with the way out's
// own pulldown (`./AutomationActionStepPanel`).
//
// **Which step is pressed is the screen's, not the picture's**, for the automation screen's reason:
// the picture marks that box and the panel draws that step, so it is held where both can see it. A
// step that is deleted takes the panel's selection with it.
import { useState } from "react";
import { AutomationActionDeclaresPanel } from "./AutomationActionDeclaresPanel";
import { AutomationActionStepPanel } from "./AutomationActionStepPanel";
import { AutomationPicture } from "./AutomationPicture";
import { AutomationStepAdd, type AddTarget } from "./AutomationStepAdd";
import { editAutomationAction, useAutomationAction } from "../core/automations";
import { actionGraph } from "./automationLayout";
import { errText, t, tf, tn } from "../core/i18n";
import { asTyped } from "../core/keys";
import { ErrorNote } from "../components/ErrorNote";
import { Icon } from "../components/Icon";
import { useDraft, type Run } from "./automationPanel";
import type { AutomationActionDetailDto } from "../bindings/bindings";

/**
 * What carries out a step put in on a line — the likeliest answer for the one being put in front of
 * it. An action whose steps name none falls back to the first agent the catalog lists, which is what
 * `automation step-add` asks for and never guesses.
 */
function agentOn(action: AutomationActionDetailDto | null, edgeId: number): string {
  const edge = action?.edges.find((one) => one.id === edgeId);
  return action?.steps.find((one) => one.id === edge?.fromId)?.agent ?? "claude-code";
}

export function AutomationActionBuildScreen({
  id,
  projectId,
  onBack,
}: {
  id: number;
  /** Whose project this is — what the machine is asked about when a step picks an agent. */
  projectId: number | null;
  onBack: () => void;
}) {
  const action = useAutomationAction(id);
  // Which step the panel is showing. Nothing until a box is pressed — an action opens on the
  // picture, and a step picked for the reader would be one they did not choose.
  const [step, setStep] = useState<number | null>(null);
  // Where the dialog that writes a step is about to put one, while it is open.
  const [adding, setAdding] = useState<AddTarget | null>(null);
  const [refused, setRefused] = useState<string | null>(null);
  const [name, setName] = useDraft(action?.name ?? "");
  const [note, setNote] = useDraft(action?.note ?? "");

  const run: Run = (write) => {
    setRefused(null);
    return Promise.resolve(write)
      .then(() => true)
      .catch((e: unknown) => {
        setRefused(errText(e));
        return false;
      });
  };

  return (
    <div className="settings">
      <div className="settings__section">
        <div className="settings__body">
          <div className="auto__head">
            <button type="button" className="btn" onClick={onBack}>
              <Icon name="chevronLeft" /> {t("auto.build.back")}
            </button>
            <span className="auto__name">{action?.name ?? ""}</span>
          </div>
        </div>
      </div>

      {refused !== null && <ErrorNote tone="quiet">{refused}</ErrorNote>}

      {action !== null && (
        <div className="settings__section">
          <div className="settings__body">
            <h3 className="auto__place">{t("auto.act.about")}</h3>
            <label className="autostep__field">
              <span className="autostep__label">{t("auto.actions.name")}</span>
              <input
                value={name}
                onChange={(e) => setName(e.target.value)}
                onBlur={() =>
                  name !== action.name && void run(editAutomationAction(action.id, { name }))
                }
              />
            </label>
            <label className="autostep__field">
              <span className="autostep__label">{t("auto.actions.note")}</span>
              <textarea
                {...asTyped}
                rows={3}
                value={note}
                onChange={(e) => setNote(e.target.value)}
                onBlur={() =>
                  note !== action.note && void run(editAutomationAction(action.id, { note }))
                }
              />
              <span className="autostep__said">{t("auto.actions.noteWhat")}</span>
            </label>
            <div className="autostep__field">
              <span className="autostep__label">{t("auto.actions.reach")}</span>
              <span className="autostep__said">
                {action.global ? t("auto.actions.reachDevice") : t("auto.actions.reachProject")}
              </span>
            </div>
            <div className="autostep__said">
              {action.usedBy === 0
                ? t("auto.actions.unused")
                : tn("auto.actions.usedBy", action.usedBy)}
            </div>
          </div>
        </div>
      )}

      <div className="settings__section">
        <div className="settings__body">
          <h3 className="auto__place">{t("auto.build.picture")}</h3>
          <AutomationPicture
            graph={actionGraph(action)}
            empty={t("auto.act.empty")}
            insertLabel={t("auto.act.insert")}
            selectedBoxId={step ?? undefined}
            onPickBox={setStep}
            onInsert={(edgeId) => setAdding({ picture: "action", edgeId })}
          />
          {action !== null && (
            <button
              type="button"
              className={action.steps.length === 0 ? "btn btn--primary" : "btn"}
              onClick={() => setAdding({ picture: "action", actionId: action.id })}
            >
              {action.steps.length === 0 ? t("auto.act.firstStep") : t("auto.act.stepAdd")}
            </button>
          )}
          {action !== null && action.entryStepId === undefined && action.steps.length > 0 && (
            <div className="auto__notready">{t("auto.act.noEntry")}</div>
          )}
        </div>
      </div>

      <div className="settings__section">
        <div className="settings__body">
          <h3 className="auto__place">{t("auto.act.step")}</h3>
          <AutomationActionStepPanel
            action={action}
            stepId={step}
            projectId={projectId}
            onRemoved={() => setStep(null)}
          />
        </div>
      </div>

      {action !== null && (
        <div className="settings__section">
          <div className="settings__body">
            <h3 className="auto__place">{t("auto.act.declares")}</h3>
            <div className="autostep__said">{tf("auto.act.declaresWhat", { name: action.name })}</div>
            <AutomationActionDeclaresPanel action={action} run={run} />
          </div>
        </div>
      )}

      {adding !== null && (
        <AutomationStepAdd
          into={adding}
          projectId={projectId}
          agent={"edgeId" in adding ? agentOn(action, adding.edgeId) : "claude-code"}
          onClose={() => setAdding(null)}
        />
      )}
    </div>
  );
}
