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
// **Every press here writes at once**, the way the "Edit" panel's fields do (`./AutomationAboutPanel`),
// **but a number** (`AMB-D-1189`). A headcount or the run limit is stepped press after press, and a
// write and a read again each press made the number slow to move and, pressed fast, land on another than
// the one pressed to. So a number is held (`useHeldNumbers`): the box, the sum and what the saved version
// holds move on the press, and the number is written once, 0.8 seconds after it was last changed — each
// change counting the 0.8 seconds again. "Save", and leaving the screen, write what is held first;
// "Discard changes" drops it. A number typed out of range is not handed on, and keeps "Save" held back
// while it stands in its box, so what is saved is never other than what is shown. Taking the limit back
// to no limit, or setting one, is a single press and writes at once.
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
import { useCallback, useEffect, useRef, useState } from "react";
import { editAutomation, editAutomationRole, setAutomationRunLimit } from "../core/automations";
import { builtinWord } from "../core/builtinWords";
import { errText, t, tf } from "../core/i18n";
import { ErrorNote } from "../components/ErrorNote";
import { Stepper } from "../components/Stepper";
import { RoleIcon } from "./AutomationRoles";
import { ClassRows } from "./automationParts";
import { taskLine, whenLine } from "./AutomationTrigger";
import type { FromField } from "./AutomationSaved";
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
function savedLimitLine(automation: AutomationDetailDto, limit: number | undefined): string | null {
  if (automation.saved === undefined || automation.savedRunLimit === limit) return null;
  return automation.savedRunLimit === undefined
    ? t("auto.side.runLimitSavedNone")
    : tf("auto.side.runLimitSaved", { n: automation.savedRunLimit });
}

/**
 * **The most runs that go at once across the roles**, as a row under them: "No limit" with a press to
 * set one, or a stepper with a press to take it back to no limit. A limit is first set at the headcounts
 * added up, the most that go at once without one, within the cap.
 */
function RunLimitRow({ automation, limit, total, write, step, refuse }: {
  automation: AutomationDetailDto;
  /** The limit the box shows: one held, or the one the definition holds. */
  limit: number | undefined;
  total: number;
  /** Set a limit, or take it back to none, at once. */
  write: (limit: number | null) => void;
  /** Step the limit, held to be written once the reader stops. */
  step: (limit: number) => void;
  refuse: (refused: boolean) => void;
}) {
  const saved = savedLimitLine(automation, limit);
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
            valueTemplate={t("auto.side.runLimitValue")}
            lessLabel={t("auto.side.headcountLess")}
            moreLabel={t("auto.side.headcountMore")}
            onStep={step}
            onRefused={refuse}
          />
          <button type="button" className="autoside__limitclear" onClick={() => write(null)}>
            {t("auto.side.runLimitClear")}
          </button>
        </span>
      )}
    </div>
  );
}

/** How long a number stands unchanged before it is written (`AMB-D-1189`). */
export const HOLD_MS = 800;

/** Sends a write with no field to mark beside. */
const direct: FromField = (write) => write();

/** Which number is held: a role's headcount, or the run limit. */
export type HeldKey = `role:${number}` | "limit";

/** The number the definition holds now for a key; undefined for a role that is gone or no limit. */
function written(automation: AutomationDetailDto, key: HeldKey): number | undefined {
  if (key === "limit") return automation.runLimit;
  const id = Number(key.slice("role:".length));
  return automation.roles.find((role) => role.id === id)?.headcount;
}

/**
 * **The numbers stepped here and not yet written** (`AMB-D-1189`), kept by the build screen so its own
 * "Save" can write them first. A number is held from the press that changes it until the definition
 * holds it, so the box never goes back to the number before while the write is on its way.
 */
export type HeldNumbers = {
  /** The headcount the role's box shows: the one held, or the one the definition holds. */
  headcount: (roleId: number, now: number) => number;
  /** The run limit the box shows. */
  runLimit: (now: number | undefined) => number | undefined;
  /** Hold a number, written 0.8 seconds after the last one held. */
  hold: (key: HeldKey, value: number) => void;
  /** Write every number held now, and stop the wait. Refused with core's reason if a write is. */
  flush: () => Promise<void>;
  /** Forget a number held, or every one, without writing it. */
  drop: (key?: HeldKey) => void;
  /** Whether a number is held that the definition does not hold yet. */
  pending: boolean;
  /** Whether a box holds a number typed out of range. */
  refused: boolean;
  /** Tell whether a box holds a number typed out of range. */
  refuse: (key: HeldKey, refused: boolean) => void;
  /** What core refused the last write held here with, until the next number is held. */
  failed: string | null;
};

export function useHeldNumbers(
  id: number,
  automation: AutomationDetailDto | null,
  /** Notes the field the event is on, so the "Written" mark stands beside it (`./AutomationSaved`). */
  fromHere?: () => FromField,
): HeldNumbers {
  // The numbers held, each with the field it was held from, and the number last sent for each.
  const held = useRef(new Map<HeldKey, { value: number; from: FromField | undefined }>());
  const sent = useRef(new Map<HeldKey, number>());
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [heldNow, setHeldNow] = useState<ReadonlyMap<HeldKey, number>>(new Map());
  const [refusedKeys, setRefusedKeys] = useState<ReadonlySet<HeldKey>>(new Set());
  const [failed, setFailed] = useState<string | null>(null);

  const redraw = useCallback(
    () => setHeldNow(new Map([...held.current].map(([key, one]): [HeldKey, number] => [key, one.value]))),
    [],
  );
  const stopWait = useCallback(() => {
    if (timer.current !== null) clearTimeout(timer.current);
    timer.current = null;
  }, []);

  const flush = useCallback((): Promise<void> => {
    stopWait();
    const writes = [...held.current]
      .filter(([key, one]) => sent.current.get(key) !== one.value)
      .map(([key, { value, from }]) => {
        sent.current.set(key, value);
        const send = () =>
          key === "limit"
            ? setAutomationRunLimit(id, value)
            : editAutomationRole(Number(key.slice("role:".length)), { headcount: value });
        return (from ?? direct)(send).catch((e: unknown) => {
          if (sent.current.get(key) === value) sent.current.delete(key);
          setFailed(errText(e));
          throw e;
        });
      });
    return Promise.all(writes).then(() => undefined);
  }, [id, stopWait]);

  const hold = (key: HeldKey, value: number) => {
    setFailed(null);
    held.current.set(key, { value, from: fromHere?.() });
    redraw();
    stopWait();
    timer.current = setTimeout(() => {
      timer.current = null;
      flush().catch(() => undefined);
    }, HOLD_MS);
  };

  const drop = (key?: HeldKey) => {
    if (key === undefined) {
      held.current.clear();
      sent.current.clear();
    } else {
      held.current.delete(key);
      sent.current.delete(key);
    }
    if (held.current.size === 0) stopWait();
    redraw();
  };

  // A number the definition has come to hold is no longer held: written, or stepped back to it.
  useEffect(() => {
    if (automation === null) return;
    let changed = false;
    for (const [key, one] of held.current) {
      if (written(automation, key) !== one.value) continue;
      held.current.delete(key);
      sent.current.delete(key);
      changed = true;
    }
    if (!changed) return;
    if (held.current.size === 0) stopWait();
    redraw();
  }, [automation, redraw, stopWait]);

  // Leaving the screen, or going to another automation, writes what is held first.
  useEffect(() => {
    const numbers = held.current;
    const last = sent.current;
    return () => {
      flush().catch(() => undefined);
      numbers.clear();
      last.clear();
    };
  }, [flush]);

  const refuse = useCallback((key: HeldKey, refused: boolean) => {
    setRefusedKeys((now) => {
      if (now.has(key) === refused) return now;
      const next = new Set(now);
      if (refused) next.add(key);
      else next.delete(key);
      return next;
    });
  }, []);

  return {
    headcount: (roleId, now) => heldNow.get(`role:${roleId}`) ?? now,
    runLimit: (now) => (now === undefined ? undefined : heldNow.get("limit") ?? now),
    hold,
    flush,
    drop,
    pending: heldNow.size > 0,
    refused: refusedKeys.size > 0,
    refuse,
    failed,
  };
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
  held,
  draft,
  onOpenTrigger,
}: {
  automation: AutomationDetailDto;
  /** The numbers stepped and not yet written, kept by the build screen (`useHeldNumbers`). */
  held: HeldNumbers;
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
  const writeRunLimit = (limit: number | null) => {
    setRefused(null);
    held.drop("limit");
    setAutomationRunLimit(automation.id, limit).catch((e: unknown) => setRefused(errText(e)));
  };
  const step = (key: HeldKey, value: number) => {
    setRefused(null);
    held.hold(key, value);
  };
  const headcounts = automation.roles.reduce((sum, role) => sum + held.headcount(role.id, role.headcount), 0);
  const limit = held.runLimit(automation.runLimit);
  const shownRefusal = refused ?? held.failed;
  // The most under way at once: the headcounts added up, or the limit where it is fewer.
  const total = limit === undefined ? headcounts : Math.min(limit, headcounts);

  return (
    <div className="autoside">
      <span className="actbuild__sec">{t("auto.side.title")}</span>
      {shownRefusal !== null && <ErrorNote tone="quiet">{shownRefusal}</ErrorNote>}

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
            const headcount = held.headcount(role.id, role.headcount);
            return (
              <li key={role.id} className="autoside__role">
                <RoleIcon role={role} size={22} />
                <div className="autoside__rolebody">
                  <span className="autoside__rolename">{role.name}</span>
                  {(actions.length > 0 || (saved !== undefined && saved !== headcount)) && (
                    <span className="autoside__acts">
                      {actions.map((one) => (
                        <span key={one.id} className="autoside__act">
                          {builtinWord(one.builtin, one.name)}
                        </span>
                      ))}
                      {saved !== undefined && saved !== headcount && (
                        <span className="autostep__hint">{tf("auto.side.headcountSaved", { n: saved })}</span>
                      )}
                    </span>
                  )}
                </div>
                <Stepper
                  value={headcount}
                  min={MIN_HEADCOUNT}
                  max={automation.maxHeadcount}
                  label={tf("auto.side.headcountOf", { role: role.name })}
                  valueTemplate={t("auto.side.headcountValue")}
                  lessLabel={t("auto.side.headcountLess")}
                  moreLabel={t("auto.side.headcountMore")}
                  onStep={(next) => step(`role:${role.id}`, next)}
                  onRefused={(no) => held.refuse(`role:${role.id}`, no)}
                />
              </li>
            );
          })}
        </ul>
        <RunLimitRow
          automation={automation}
          limit={limit}
          total={headcounts}
          write={writeRunLimit}
          step={(next) => step("limit", next)}
          refuse={(no) => held.refuse("limit", no)}
        />
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
          {automation.unsaved || held.pending ? (
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
