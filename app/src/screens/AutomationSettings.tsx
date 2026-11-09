// **What the automation holds as a whole, down the build screen's right** (`./AutomationBuildScreen`):
// who a failed round is filed to, its triggers, and how many each role has. The roles stand down the
// left and what is built for the picked one in the middle; these are about none of the roles in
// particular, or about all of them at once, so they stand beside both, always in view, each in a box
// of its own.
//
// **How many go at once is said role by role** (`AMB-D-1123`): the box lists every role in the order
// they are defined in, each with the actions placed for it and a stepper from 1 to the cap core sets
// (`AMB-D-1124`), and under them the sum — the most tasks under way at once. The headcount is part of
// the definition, so a step goes to the draft like any other write there, and a launch goes with it
// once it is saved. A role whose draft differs from the saved version says what the version holds, and
// the foot of the box saves or discards the draft with the head's own presses — the whole draft, not
// the headcounts alone.
//
// **Every press here writes at once**, the way the "Edit" panel's fields do (`./AutomationAboutPanel`).
// The headcounts are part of the saved versions, so the head's "Save" and "Discard changes" keep and
// undo them. A trigger is a setting kept apart from the saved versions (`AMB-D-1119`), and so are who
// failures are filed to and what they are classified under: neither press keeps or undoes them. A
// refusal is drawn once, at the top of the column, in the words core wrote.
//
// **Who failures are filed to is marked required.** A launch is refused while nobody is chosen
// (`not_ready_automation_no_failures_to`), so a definition with none opens on "Choose one", which cannot
// be picked back once something else is. The roles offered are this automation's own, in their order,
// but for those a trigger starts: they take no task, so core refuses them (`AMB-D-1142`).
//
// **Under it, the classification the failed round's task is filed with** (`AMB-D-1129`): a row per axis
// the project classifies tasks on, its values pressed the way the built-in that files a task has them
// pressed (`./AutomationMakeTaskCfg`), one value an axis. Amenbo guesses none. A required axis left
// empty is said under the rows, once per axis, as core counts it — the save and the launch check refuse
// it, and the reason the head lists is the same one. A closed value cannot be newly chosen; one already
// chosen stays on its row so it can be taken off. Under the box's hint, an example of the body such a
// task is filed with opens on a press.
//
// **The column lists every trigger the automation has** (`AMB-D-1119`), in its order, one box each:
// when it files a task, what the task says, and the role it is worked under. "+ Add a trigger" stands
// under the list however many there are; with none, the column says so above it. Pressing a box, or
// "+ Add a trigger", asks the build screen to open that trigger's card in its middle
// (`./AutomationTrigger`), to change it or take it away, or to write a new one.
import { useState } from "react";
import { axesFor } from "../core/appliesTo";
import { editAutomation, editAutomationRole } from "../core/automations";
import { builtinWord } from "../core/builtinWords";
import { errText, t, tf } from "../core/i18n";
import { ErrorNote } from "../components/ErrorNote";
import { Stepper } from "../components/Stepper";
import { getSnapshot } from "../core/snapshot";
import { RoleIcon } from "./AutomationRoles";
import { taskLine, whenLine } from "./AutomationTrigger";
import type {
  AutomationDetailDto,
  AutomationFailuresToDto,
  AutomationTriggerDto,
  DimensionDto,
} from "../bindings/bindings";

/** The pulldown's value for who failures are filed to: `""` for nobody yet. */
function failuresValue(to: AutomationFailuresToDto | undefined): string {
  if (to === undefined) return "";
  return to.kind === "human" ? "human" : `role:${to.roleId}`;
}

function failuresOf(value: string): AutomationFailuresToDto | null {
  if (value === "human") return { kind: "human" };
  if (value.startsWith("role:")) return { kind: "role", roleId: Number(value.slice("role:".length)) };
  return null;
}

/** The reason core gives for a required axis the failed round's task would be filed with no value on. */
const AXIS_EMPTY = "not_ready_automation_failure_axis_empty";

/**
 * The axes a failed round's task can be classified on in this project, each with the values that can
 * be chosen: the open ones, and a closed one only where it is already chosen. An axis with nothing to
 * press is left out.
 */
function failureAxes(projectId: number, chosen: readonly number[]): DimensionDto[] {
  const dims = getSnapshot().projects.find((p) => p.id === projectId)?.dimensions ?? [];
  return axesFor("task", dims)
    .map((dim) => ({ ...dim, values: dim.values.filter((value) => !value.closed || chosen.includes(value.id)) }))
    .filter((dim) => dim.values.length > 0);
}

/** The classification with `value` pressed on `axis`: it alone on the axis, or none where it was on. */
function pressedValue(chosen: readonly number[], axis: DimensionDto, value: number): number[] {
  const onAxis = new Set(axis.values.map((one) => one.id));
  const rest = chosen.filter((one) => !onAxis.has(one));
  return chosen.includes(value) ? rest : [...rest, value];
}

/** **What a failed round's task is classified under**: a row of values per axis, and each required
 *  axis core finds empty. */
function FailureClass({ automation, write }: {
  automation: AutomationDetailDto;
  write: (patch: Parameters<typeof editAutomation>[1]) => void;
}) {
  const chosen = automation.failureValues;
  const axes = failureAxes(automation.projectId, chosen);
  const empty = automation.saveBlocks.filter((block) => block.code === AXIS_EMPTY).map((block) => block.fields.axis ?? "");
  return (
    <div className="autoside__class">
      <span className="autoside__classname">{t("auto.side.failureClass")}</span>
      {axes.length === 0 ? (
        <span className="autostep__hint">{t("auto.step.noAxes")}</span>
      ) : (
        <div className="autostep__rows">
          {axes.map((dim) => (
            <div key={dim.id} className="autostep__row">
              <span className="autostep__rowname">
                {dim.name}
                {dim.required && <span className="autostep__req">{t("auto.side.required")}</span>}
              </span>
              {dim.values.map((one) => {
                const on = chosen.includes(one.id);
                return (
                  <button
                    key={one.id}
                    type="button"
                    className={`autostep__chip ${on ? "autostep__chip--on" : ""}`}
                    aria-pressed={on}
                    onClick={() => write({ failureValues: pressedValue(chosen, dim, one.id) })}
                  >
                    {one.name}
                  </button>
                );
              })}
            </div>
          ))}
        </div>
      )}
      {empty.map((axis) => (
        <div key={axis} className="autoside__why" role="status">
          {tf("auto.side.failureAxisEmpty", { axis })}
        </div>
      ))}
    </div>
  );
}

/** **An example of the body a failure task is filed with**, behind a press. The values are made up. */
function FailureExample() {
  const task = t("auto.side.failureExampleTaskValue");
  const rows: [string, string][] = [
    [t("auto.side.failureExampleWhy"), t("auto.side.failureExampleWhyValue")],
    [t("auto.side.failureExampleStep"), t("auto.side.failureExampleStepValue")],
    [t("auto.side.failureExampleReport"), t("auto.side.failureExampleReportValue")],
    [t("auto.side.failureExampleRun"), "#540"],
    [t("auto.side.failureExampleTask"), task],
    [t("auto.side.failureExampleCount"), tf("auto.side.failureExampleCountValue", { n: 3 })],
  ];
  return (
    <details className="autoside__example">
      <summary>{t("auto.side.failureExample")}</summary>
      <div className="autoside__body">
        <b>{tf("auto.side.failureExampleTitle", { task })}</b>
        <dl>
          {rows.map(([term, value]) => (
            <div key={term} className="autoside__bodyrow">
              <dt>{term}</dt>
              <dd>{value}</dd>
            </div>
          ))}
        </dl>
      </div>
    </details>
  );
}

/** The fewest a role can have. The most is core's (`AutomationDetailDto.maxHeadcount`). */
const MIN_HEADCOUNT = 1;

/**
 * **The head's "Save" and "Discard changes"**, handed down so the foot of the headcount box presses
 * the same two, held back on the same terms.
 */
export type DraftPresses = {
  save: () => void;
  discard: () => void;
  canSave: boolean;
  canDiscard: boolean;
};

/** "Most N at once", with the number set off the way the sentence around it is not. */
function TotalLine({ total }: { total: number }) {
  const [before, after = ""] = t("auto.side.headcountTotal").split("{total}");
  return (
    <span>
      {before}
      <b className="autoside__num">{total}</b>
      {after}
    </span>
  );
}

export function AutomationSettings({
  automation,
  draft,
  onOpenTrigger,
}: {
  automation: AutomationDetailDto;
  draft: DraftPresses;
  /** Open a trigger's card: the one given, or a new one. */
  onOpenTrigger: (trigger: AutomationTriggerDto | undefined) => void;
}) {
  const [refused, setRefused] = useState<string | null>(null);
  const failures = failuresValue(automation.failuresTo);
  const roleLine = (trigger: AutomationTriggerDto) => {
    const role = automation.roles.find((one) => one.id === trigger.roleId);
    return role === undefined ? t("auto.side.triggerRoleNone") : tf("auto.side.triggerRole", { role: role.name });
  };

  const write = (patch: Parameters<typeof editAutomation>[1]) => {
    setRefused(null);
    editAutomation(automation.id, patch).catch((e: unknown) => setRefused(errText(e)));
  };
  const writeHeadcount = (roleId: number, headcount: number) => {
    setRefused(null);
    editAutomationRole(roleId, { headcount }).catch((e: unknown) => setRefused(errText(e)));
  };
  const total = automation.roles.reduce((sum, role) => sum + role.headcount, 0);

  return (
    <div className="autoside">
      <span className="actbuild__sec">{t("auto.side.title")}</span>
      {refused !== null && <ErrorNote tone="quiet">{refused}</ErrorNote>}

      <div className="autoside__group">
        <span className="autostep__label">
          {t("auto.side.failuresTo")}
          <span className="autostep__req">{t("auto.side.required")}</span>
        </span>
        <select
          aria-label={t("auto.side.failuresTo")}
          value={failures}
          onChange={(e) => {
            const to = failuresOf(e.target.value);
            if (to !== null) write({ failuresTo: to });
          }}
        >
          {failures === "" && (
            <option value="" disabled>
              {t("auto.side.failuresNone")}
            </option>
          )}
          {automation.roles
            .filter((role) => role.startsBy === "task" || failures === `role:${role.id}`)
            .map((role) => (
              <option key={role.id} value={`role:${role.id}`}>
                {tf("auto.side.failuresRole", { role: role.name })}
              </option>
            ))}
          <option value="human">{t("auto.side.failuresHuman")}</option>
        </select>
        <FailureClass automation={automation} write={write} />
        <span className="autostep__hint">{t("auto.side.failuresHint")}</span>
        <FailureExample />
      </div>

      <div className="autoside__group">
        <span className="autostep__label">{t("auto.side.trigger")}</span>
        {automation.triggers.length === 0 ? (
          <span className="autostep__hint">{t("auto.side.triggerNone")}</span>
        ) : (
          <ul className="autoside__triggers">
            {automation.triggers.map((trigger) => (
              <li key={trigger.id}>
                <button
                  type="button"
                  className="autoside__trigger"
                  title={trigger.name.trim() || t("auto.trigger.title")}
                  onClick={() => onOpenTrigger(trigger)}
                >
                  <span className="autoside__when">{whenLine(trigger, automation.roles)}</span>
                  <span className="autoside__what">{taskLine(trigger)}</span>
                  <span className="autostep__hint">{roleLine(trigger)}</span>
                </button>
              </li>
            ))}
          </ul>
        )}
        <button type="button" className="btn autoside__add" onClick={() => onOpenTrigger(undefined)}>
          {t("auto.side.triggerAdd")}
        </button>
      </div>

      <div className="autoside__group autoside__group--flush">
        <span className="autostep__label autoside__pad">{t("auto.side.headcount")}</span>
        <ul className="autoside__roles">
          {automation.roles.map((role) => {
            const actions = automation.placements.filter((one) => one.roleId === role.id);
            const saved = role.savedHeadcount;
            return (
              <li key={role.id} className="autoside__role">
                <RoleIcon role={role} size={22} />
                <div className="autoside__rolebody">
                  <span className="autoside__rolename">{role.name}</span>
                  {(actions.length > 0 || (saved !== undefined && saved !== role.headcount)) && (
                    <span className="autoside__acts">
                      {actions.map((one) => (
                        <span key={one.id} className="autoside__act">
                          {builtinWord(one.builtin, one.name)}
                        </span>
                      ))}
                      {saved !== undefined && saved !== role.headcount && (
                        <span className="autostep__hint">{tf("auto.side.headcountSaved", { n: saved })}</span>
                      )}
                    </span>
                  )}
                </div>
                <Stepper
                  value={role.headcount}
                  min={MIN_HEADCOUNT}
                  max={automation.maxHeadcount}
                  label={tf("auto.side.headcountOf", { role: role.name })}
                  valueText={tf("auto.side.headcountValue", { n: role.headcount })}
                  lessLabel={t("auto.side.headcountLess")}
                  moreLabel={t("auto.side.headcountMore")}
                  onStep={(next) => writeHeadcount(role.id, next)}
                />
              </li>
            );
          })}
        </ul>
        <div className="autoside__total">
          <TotalLine total={total} />
          <span className="autostep__hint">
            {tf("auto.side.headcountRange", { min: MIN_HEADCOUNT, max: automation.maxHeadcount })}
          </span>
        </div>
        <div className="autoside__draft">
          {automation.unsaved ? (
            <>
              <span>{t("auto.side.draftUnsaved")}</span>
              <span className="autoside__presses">
                <button type="button" className="btn" disabled={!draft.canDiscard} onClick={draft.discard}>
                  {t("auto.side.draftDiscard")}
                </button>
                <button type="button" className="btn btn--primary" disabled={!draft.canSave} onClick={draft.save}>
                  {t("auto.side.draftSave")}
                </button>
              </span>
            </>
          ) : (
            <span className="autostep__hint">{t("auto.side.draftSame")}</span>
          )}
        </div>
      </div>
    </div>
  );
}
