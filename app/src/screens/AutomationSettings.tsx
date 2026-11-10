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
// **Under the roles, the most runs that go at once across them** (`AMB-D-1145`): "No limit" by default,
// where as many go as the headcounts come to. Set, it is stepped from 1 to the cap core sets, and taken
// back to no limit with a press. Like a headcount it is part of the draft, and says what the saved
// version holds where the two differ. The sum under it is the smaller of the limit and the headcounts
// added up; a limit at or over that sum is said to have no effect.
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
import { editAutomation, editAutomationRole, setAutomationRunLimit } from "../core/automations";
import { builtinWord } from "../core/builtinWords";
import { errText, t, tf } from "../core/i18n";
import { ErrorNote } from "../components/ErrorNote";
import { Stepper } from "../components/Stepper";
import { RoleIcon } from "./AutomationRoles";
import { ClassRows } from "./automationParts";
import { taskLine, whenLine } from "./AutomationTrigger";
import type {
  AutomationDetailDto,
  AutomationFailuresToDto,
  AutomationTriggerDto,
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

/** **What a failed round's task is classified under**: a row of values per axis, and each required
 *  axis core finds empty. */
function FailureClass({ automation, write }: {
  automation: AutomationDetailDto;
  write: (patch: Parameters<typeof editAutomation>[1]) => void;
}) {
  const empty = automation.saveBlocks.filter((block) => block.code === AXIS_EMPTY).map((block) => block.fields.axis ?? "");
  return (
    <div className="autoside__class">
      <span className="autoside__classname">{t("auto.side.failureClass")}</span>
      <ClassRows
        projectId={automation.projectId}
        chosen={automation.failureValues}
        onChange={(failureValues) => write({ failureValues })}
      />
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

/** The fewest runs a limit lets go at once. The most is core's (`AutomationDetailDto.maxRunLimit`). */
const MIN_RUN_LIMIT = 1;

/** What the saved version holds for the run limit, said where it differs from the draft's; null where
 *  it does not, or nobody has saved. */
function savedLimitLine(automation: AutomationDetailDto): string | null {
  if (automation.saved === undefined || automation.savedRunLimit === automation.runLimit) return null;
  return automation.savedRunLimit === undefined
    ? t("auto.side.runLimitSavedNone")
    : tf("auto.side.runLimitSaved", { n: automation.savedRunLimit });
}

/**
 * **The most runs that go at once across the roles**, as a row under them: "No limit" with a press to
 * set one, or a stepper with a press to take it back to no limit. A limit is first set at the headcounts
 * added up, the most that go at once without one, within the cap.
 */
function RunLimitRow({ automation, total, write }: {
  automation: AutomationDetailDto;
  total: number;
  write: (limit: number | null) => void;
}) {
  const limit = automation.runLimit;
  const saved = savedLimitLine(automation);
  return (
    <div className="autoside__limit">
      <div className="autoside__rolebody">
        <span className="autoside__rolename">{t("auto.side.runLimit")}</span>
        <span className="autoside__acts">
          {limit === undefined && <span>{t("auto.side.runLimitNone")}</span>}
          {limit === undefined && saved !== null && " · "}
          {saved !== null && <span className="autostep__hint">{saved}</span>}
        </span>
      </div>
      {limit === undefined ? (
        <button
          type="button"
          className="btn"
          onClick={() => write(Math.min(Math.max(total, MIN_RUN_LIMIT), automation.maxRunLimit))}
        >
          {t("auto.side.runLimitSet")}
        </button>
      ) : (
        <span className="autoside__limitset">
          <Stepper
            value={limit}
            min={MIN_RUN_LIMIT}
            max={automation.maxRunLimit}
            label={t("auto.side.runLimit")}
            valueText={tf("auto.side.runLimitValue", { n: limit })}
            lessLabel={t("auto.side.headcountLess")}
            moreLabel={t("auto.side.headcountMore")}
            onStep={write}
          />
          <button type="button" className="autoside__limitclear" onClick={() => write(null)}>
            {t("auto.side.runLimitClear")}
          </button>
        </span>
      )}
    </div>
  );
}

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
  const writeRunLimit = (limit: number | null) => {
    setRefused(null);
    setAutomationRunLimit(automation.id, limit).catch((e: unknown) => setRefused(errText(e)));
  };
  const headcounts = automation.roles.reduce((sum, role) => sum + role.headcount, 0);
  const limit = automation.runLimit;
  // The most under way at once: the headcounts added up, or the limit where it is fewer.
  const total = limit === undefined ? headcounts : Math.min(limit, headcounts);

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
        <RunLimitRow automation={automation} total={headcounts} write={writeRunLimit} />
        <div className="autoside__total">
          <TotalLine total={total} />
          {limit !== undefined && limit >= headcounts && (
            <span className="autostep__hint">{tf("auto.side.runLimitIdle", { n: limit, total: headcounts })}</span>
          )}
          <span className="autostep__hint">
            {tf("auto.side.headcountRange", { min: MIN_HEADCOUNT, max: automation.maxHeadcount })}
          </span>
          <span className="autostep__hint">
            {tf("auto.side.runLimitRange", { min: MIN_RUN_LIMIT, max: automation.maxRunLimit })}
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
