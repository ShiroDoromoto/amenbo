// **An automation's trigger, written on a card in the build screen's middle** — what files a task
// without a person and starts a run of this automation with it. The build screen's right column asks
// for it (`./AutomationSettings`), and the screen draws the card where the picture stands
// (`./AutomationBuildScreen`); the card itself stands alone, given the automation and the trigger to
// open, or none for a new one, and told when it is done. Beside it stand the automation's triggers in
// a line each and the task this one files, so more than one can be read side by side with it.
//
// **Its head names the trigger, says whether it is on and when it last filed a task.** The name is
// written in the head itself, in place. Pressing "On" switches the trigger off at once and back
// (`setAutomationTriggerEnabled`), the way a setting kept apart from the saved versions is written; a
// new trigger has neither the chip nor the line until it is saved.
//
// **It asks three things, each in a numbered part**: 1 whether to file a task now (every so many
// minutes, when no run of it is going, while fewer than N tasks match a filter, or when a script says
// so), 2 what the task says (fixed words, a file read from its full path, or what a script writes), and
// 3 the role the task is worked under, or none, which leaves it to a session — picked from a list, the
// picked role's icon drawn beside it as the other screens draw it (`RoleIcon`). Neither 1 nor 2 asks an
// AI: both are decided mechanically, and the AI works only inside the run the task starts.
//
// **It writes the trigger it was opened on**, adding one where it was opened on none, or takes it
// away. The automation's other triggers are left as they are. It is a setting, not part of the
// definition: the save writes it at once, and the build screen's "Save" and "Discard changes" neither
// keep nor undo it. When it is next asked is counted by core from the write, so nothing here sends it.
//
// **A condition and a script can go with any answer to 1**, on top of it. "While fewer than N tasks
// match" is the condition with nothing else, and "when a script says so" the script with nothing else,
// so there each cannot be switched off.
//
// **The condition is picked, not written**: the status of the tasks it counts, the role they are
// worked under — one of this automation's own — and how few it waits for, read as one sentence. The
// status is written into the filter as `status:`, and the role travels beside it as the condition's
// own. A filter written elsewhere may say more than the two boxes can show — several statuses, or
// other keys such as `dim:`; several statuses are offered as one more choice, and the rest is kept as
// it was written and said so under the sentence, so opening and saving a trigger never rewrites what
// it counts. Under it stands how many tasks match now, read from core as it is typed and again when the
// tasks move, and what that means for the next look.
//
// **What core refuses** — a wait under a minute, a task with no title, a filter that does not read —
// comes back as core's own sentence, drawn at the top, and the card stays open with what was typed.
// The form does not check its own boxes (`noValidate`), so core's sentence is the only one drawn.
//
// **"Try it now" tries the trigger as it is typed, before it is saved** (`tryAutomationTrigger`), so
// what it would file can be seen without saving and waiting. Core checks it as the save would, counts
// its condition, runs the script it asks first and the one that writes the task, or reads the file,
// and writes nothing: no trigger, no task, no run. What it says stands in the preview on the right — the
// title and notes of the task it would file, or why it would file none: the condition does not hold,
// the script said no, or nothing could be read (core's sentence). When it is due and whether a run
// leaves it room are not part of the try; "Now: N match → …" says those. A script may run long, so the
// button cannot be pressed again until it answers, and closing the card stops it. What is tried is
// what was typed then: typing anything after takes the answer away, as it no longer says what the
// card holds. What core refuses is drawn at the top, as the save's refusal is.
//
// **Only its buttons close it** from within: a stray press or Escape would throw away what was typed.
// The build screen puts its picture back in the card's place when a role is picked on the left or the
// trigger goes away.
import { useEffect, useId, useRef, useState, type FormEvent, type ReactNode } from "react";
import {
  addAutomationTrigger,
  removeAutomationTrigger,
  setAutomationTriggerEnabled,
  stopTriggerTry,
  tryAutomationTrigger,
  updateAutomationTrigger,
  useTriggerCount,
} from "../core/automations";
import { confirmDialog } from "../core/dialog";
import { dueLabel, errText, t, tf } from "../core/i18n";
import { asTyped } from "../core/keys";
import { STATUS_ALL } from "../core/status";
import { ErrorNote } from "../components/ErrorNote";
import { filterValueLabel } from "./automationParts";
import { RoleIcon } from "./AutomationRoles";
import type {
  AutomationDetailDto,
  AutomationRoleDto,
  AutomationScriptInputDto,
  AutomationStepScriptDto,
  AutomationTriggerConditionDto,
  AutomationTriggerDto,
  AutomationTriggerInputDto,
  AutomationTriggerTriedDto,
} from "../bindings/bindings";

/** The four answers to 1 — whether to file a task now. */
export type WhenKind = "every" | "idle" | "tasks" | "script";
/** The answers to 2 — what the task says. `asked` is what the script of 1 writes beside its say. */
export type TaskKind = "fixed" | "file" | "script" | "asked";

const WHENS: { id: WhenKind; label: () => string }[] = [
  { id: "every", label: () => t("auto.trigger.whenEvery") },
  { id: "idle", label: () => t("auto.trigger.whenIdle") },
  { id: "tasks", label: () => t("auto.trigger.whenTasks") },
  { id: "script", label: () => t("auto.trigger.whenScript") },
];

const TASKS: { id: TaskKind; label: () => string }[] = [
  { id: "fixed", label: () => t("auto.trigger.taskFixed") },
  { id: "file", label: () => t("auto.trigger.taskFile") },
  { id: "script", label: () => t("auto.trigger.taskScript") },
];

/** A script as its three boxes hold it: the program, its arguments one to a line, and its timeout. */
type ScriptBoxes = { program: string; args: string; minutes: string };

/** Everything the card holds, as typed. */
export type TriggerForm = {
  name: string;
  when: WhenKind;
  minutes: string;
  /** Whether a condition goes with an answer to 1 other than "tasks", which always has one. */
  counted: boolean;
  /** The status the condition counts, its values as the filter writes them (`todo`, or
   *  `todo,in_progress` read from a filter written elsewhere), or `""` for any. */
  status: string;
  /** The role the counted tasks are worked under, or `null` for any. */
  countedRoleId: number | null;
  /** What the filter says besides its status, which no box shows: kept as it was written. */
  filterRest: string;
  below: string;
  /** Whether a script is asked with an answer to 1 other than "script", which always asks one. */
  asked: boolean;
  judge: ScriptBoxes;
  task: TaskKind;
  title: string;
  notes: string;
  path: string;
  writer: ScriptBoxes;
  /** The role picked, or `null` for none. */
  roleId: number | null;
};

const NO_SCRIPT: ScriptBoxes = { program: "", args: "", minutes: "" };

function boxesOf(script: AutomationStepScriptDto | undefined): ScriptBoxes {
  if (script === undefined) return NO_SCRIPT;
  return { program: script.program, args: script.args.join("\n"), minutes: String(script.timeoutMinutes) };
}

/**
 * A condition's filter as the card holds it: the values of its first `status:` — `""` where it has
 * none — and every other part, as it was written.
 */
export function splitFilter(filter: string): { status: string; rest: string } {
  let status: string | null = null;
  const rest: string[] = [];
  for (const part of filter.split(/\s+/)) {
    if (part === "") continue;
    if (status === null && part.startsWith("status:") && part.length > "status:".length) {
      status = part.slice("status:".length);
    } else {
      rest.push(part);
    }
  }
  return { status: status ?? "", rest: rest.join(" ") };
}

/** The filter the card's condition sends: its status, then what it kept. */
export function joinFilter(status: string, rest: string): string {
  return [status === "" ? "" : `status:${status}`, rest.trim()].filter((one) => one !== "").join(" ");
}

/** The form a trigger opens with: the one written, or a new one that files every hour. A condition
 *  not yet written starts counting the tasks still to do, worked under any role. */
export function formOf(trigger: AutomationTriggerDto | undefined): TriggerForm {
  const condition = trigger?.condition;
  const filter = condition === undefined ? { status: "todo", rest: "" } : splitFilter(condition.filter);
  return {
    name: trigger?.name ?? "",
    when: trigger?.when.kind ?? "every",
    minutes: trigger?.when.kind === "every" ? String(trigger.when.minutes) : "60",
    counted: condition !== undefined,
    status: filter.status,
    countedRoleId: condition?.roleId ?? null,
    filterRest: filter.rest,
    below: condition === undefined ? "1" : String(condition.below),
    asked: trigger?.script !== undefined,
    judge: boxesOf(trigger?.script),
    task: trigger?.task.kind ?? "fixed",
    title: trigger?.task.kind === "fixed" ? trigger.task.title : "",
    notes: trigger?.task.kind === "fixed" ? trigger.task.notes : "",
    path: trigger?.task.kind === "file" ? trigger.task.path : "",
    writer: boxesOf(trigger?.task.kind === "script" ? trigger.task.script : undefined),
    roleId: trigger?.roleId ?? null,
  };
}

/** A script as core is sent it — arguments one to a line with the empty lines left out, and no
 *  timeout where the box is empty, which leaves it at the default. */
function scriptOf(boxes: ScriptBoxes): AutomationScriptInputDto {
  return {
    program: boxes.program.trim(),
    args: boxes.args.split(/\r?\n/).filter((one) => one !== ""),
    timeoutMinutes: boxes.minutes.trim() === "" ? undefined : Number(boxes.minutes),
  };
}

/** What the save sends. Whether the numbers are in range is core's to say. */
export function inputOf(form: TriggerForm): AutomationTriggerInputDto {
  const when: AutomationTriggerInputDto["when"] =
    form.when === "every" ? { kind: "every", minutes: Number(form.minutes) } : { kind: form.when };
  const task: AutomationTriggerInputDto["task"] =
    form.task === "fixed"
      ? { kind: "fixed", title: form.title.trim(), notes: form.notes }
      : form.task === "file"
        ? { kind: "file", path: form.path.trim() }
        : form.task === "asked"
          ? { kind: "asked" }
          : { kind: "script", script: scriptOf(form.writer) };
  return {
    name: form.name.trim(),
    roleId: form.roleId ?? undefined,
    when,
    condition:
      form.when === "tasks" || form.counted
        ? {
            filter: joinFilter(form.status, form.filterRest),
            roleId: form.countedRoleId ?? undefined,
            below: Number(form.below),
          }
        : undefined,
    script: form.when === "script" || form.asked ? scriptOf(form.judge) : undefined,
    task,
  };
}

/** The three boxes of a script: its program by full path, its arguments, its timeout. */
function ScriptBoxesField({
  boxes,
  onChange,
}: {
  boxes: ScriptBoxes;
  onChange: (boxes: ScriptBoxes) => void;
}) {
  return (
    <>
      <div className="autostep__pair autostep__pair--top">
        <span className="autostep__label">{t("auto.step.program")}</span>
        <div className="autostep__field">
          <input
            {...asTyped}
            className="autostep__mono"
            aria-label={t("auto.step.program")}
            value={boxes.program}
            onChange={(e) => onChange({ ...boxes, program: e.target.value })}
          />
          <span className="autostep__hint">{t("auto.step.programHint")}</span>
        </div>
      </div>
      <div className="autostep__pair autostep__pair--top">
        <span className="autostep__label">{t("auto.step.args")}</span>
        <div className="autostep__field">
          <textarea
            {...asTyped}
            className="autostep__mono"
            aria-label={t("auto.step.args")}
            rows={3}
            value={boxes.args}
            onChange={(e) => onChange({ ...boxes, args: e.target.value })}
          />
          <span className="autostep__hint">{t("auto.step.argsHint")}</span>
        </div>
      </div>
      <div className="autostep__pair autostep__pair--top">
        <span className="autostep__label">{t("auto.step.timeout")}</span>
        <div className="autostep__field">
          <span className="autostep__limit">
            <input
              type="number"
              min={1}
              aria-label={t("auto.step.timeout")}
              value={boxes.minutes}
              onChange={(e) => onChange({ ...boxes, minutes: e.target.value })}
            />
            {t("auto.step.minutes")}
          </span>
          <span className="autostep__hint">{t("auto.step.timeoutHint")}</span>
        </div>
      </div>
    </>
  );
}

/**
 * The role the task is worked under, picked from a list: the automation's roles in their order, then
 * none. The picked role's icon stands before the list; with none picked, an empty space of the same
 * size stands there, so the list does not move. A role since taken away stays picked under its number,
 * so opening and saving the trigger does not change it.
 */
function RolePick({
  automation,
  roleId,
  onPick,
}: {
  automation: AutomationDetailDto;
  roleId: number | null;
  onPick: (roleId: number | null) => void;
}) {
  const picked = automation.roles.find((one) => one.id === roleId);
  const options: { value: string; label: string }[] = automation.roles.map((one) => ({
    value: String(one.id),
    label: one.name,
  }));
  if (roleId !== null && picked === undefined) options.push({ value: String(roleId), label: String(roleId) });
  options.push({ value: "", label: t("auto.trigger.roleNone") });

  return (
    <div className="trigcard__cond trigcard__role">
      {picked === undefined ? (
        <span className="trigcard__noicon" aria-hidden="true" />
      ) : (
        <RoleIcon role={picked} size={16} />
      )}
      <select
        aria-label={t("auto.trigger.role")}
        value={roleId === null ? "" : String(roleId)}
        onChange={(e) => onPick(e.target.value === "" ? null : Number(e.target.value))}
      >
        {options.map((one) => (
          <option key={one.value} value={one.value}>
            {one.label}
          </option>
        ))}
      </select>
    </div>
  );
}

/** A row of radio buttons, one of them picked. */
function Radios<K extends string>({
  label,
  options,
  picked,
  onPick,
}: {
  label: string;
  options: { id: K; label: string }[];
  picked: K;
  onPick: (id: K) => void;
}) {
  const name = useId();
  return (
    <div className="trigcard__radios" role="radiogroup" aria-label={label}>
      {options.map((one) => (
        <label key={one.id} className="trigcard__radio">
          <input type="radio" name={name} checked={picked === one.id} onChange={() => onPick(one.id)} />
          {one.label}
        </label>
      ))}
    </div>
  );
}

/** One of the card's three numbered parts: its number in a circle, its heading, and what it asks. */
function Part({ no, title, children }: { no: number; title: string; children: ReactNode }) {
  const id = useId();
  return (
    <section className="trigcard__part" aria-labelledby={id}>
      <div className="trigcard__parthead">
        <span className="trigcard__no" aria-hidden="true">
          {no}
        </span>
        <h3 id={id}>{title}</h3>
      </div>
      {children}
    </section>
  );
}

/** How often an "every" trigger is asked, in the largest unit the minutes divide into. */
function everyLine(minutes: number): string {
  if (minutes % 1440 === 0) return tf("auto.side.everyDays", { days: minutes / 1440 });
  if (minutes % 60 === 0) return tf("auto.side.everyHours", { hours: minutes / 60 });
  return tf("auto.side.every", { minutes });
}

/**
 * What a trigger's condition counts, in the words the filter's values are drawn with elsewhere
 * (`filterValueLabel`): the values one key takes are joined by "/", the keys and the role the tasks
 * are worked under stand side by side. Empty where every task is counted.
 */
function countedWords(condition: AutomationTriggerConditionDto, roles: readonly AutomationRoleDto[]): string {
  const groups: string[] = [];
  for (const part of condition.filter.split(/\s+/)) {
    const at = part.indexOf(":");
    if (at <= 0) continue;
    const key = part.slice(0, at);
    const values = part.slice(at + 1).split(",").filter((one) => one !== "");
    if (values.length > 0) groups.push(values.map((one) => filterValueLabel(key, one)).join("/"));
  }
  const role = roles.find((one) => one.id === condition.roleId);
  if (role !== undefined) groups.push(role.name);
  return groups.join(" ");
}

/** How few of what the condition counts it waits for, naming what it counts where it says. */
function belowLine(condition: AutomationTriggerConditionDto, roles: readonly AutomationRoleDto[]): string {
  const counted = countedWords(condition, roles);
  return counted === ""
    ? tf("auto.side.below", { below: condition.below })
    : tf("auto.side.belowOf", { what: counted, below: condition.below });
}

/** When the trigger files a task, in the words of the card's part 1, with what its condition counts. */
export function whenLine(trigger: AutomationTriggerDto, roles: readonly AutomationRoleDto[]): string {
  const when =
    trigger.when.kind === "every"
      ? everyLine(trigger.when.minutes)
      : trigger.when.kind === "idle"
        ? t("auto.trigger.whenIdle")
        : trigger.when.kind === "tasks"
          ? t("auto.trigger.whenTasks")
          : t("auto.trigger.whenScript");
  const below = trigger.condition === undefined ? null : belowLine(trigger.condition, roles);
  // "Tasks" is the condition with nothing else, so it is named by the condition alone.
  if (trigger.when.kind === "tasks" && below !== null) return below;
  return below === null ? when : `${when} ・ ${below}`;
}

/** What the task the trigger files says, as far as it can be read before it is filed. */
export function taskLine(trigger: AutomationTriggerDto): string {
  switch (trigger.task.kind) {
    case "fixed":
      return trigger.task.title;
    case "file":
      return trigger.task.path;
    case "script":
      return t("auto.trigger.taskScript");
    case "asked":
      return t("auto.trigger.taskAsked");
  }
}

/** The role a trigger's task is worked under, as one line. */
function roleLine(automation: AutomationDetailDto, roleId: number | undefined): string {
  const role = roleId === undefined ? undefined : automation.roles.find((one) => one.id === roleId);
  return role === undefined ? t("auto.side.triggerRoleNone") : tf("auto.side.triggerRole", { role: role.name });
}

/**
 * When and why a trigger last could not start a run, as one line (`AMB-D-1136`), or `null` where it has
 * not failed since it last started one. A trigger toward a role started by a task, or toward none, files a
 * task, so it says it could not file one; one toward a role started by a trigger says it could not start.
 */
function refusedLine(automation: AutomationDetailDto, trigger: AutomationTriggerDto): string | null {
  const refused = trigger.lastRefused;
  if (refused === undefined) return null;
  const role = trigger.roleId === undefined ? undefined : automation.roles.find((one) => one.id === trigger.roleId);
  return tf(role?.startsBy === "trigger" ? "auto.trigger.refusedStarting" : "auto.trigger.refusedFiling", {
    at: dueLabel({ kind: "at", at: refused.at }),
    why: errText(refused),
  });
}

/**
 * The automation's triggers, a framed line each: its name, then when it files, what, and under which role
 * — and, under a trigger that last could not start a run, when and why, its frame turned red.
 */
function TriggerList({ automation }: { automation: AutomationDetailDto }) {
  if (automation.triggers.length === 0) return null;
  return (
    <>
      <h3>{t("auto.trigger.list")}</h3>
      <ul className="trigstage__list" data-list="triggers">
        {automation.triggers.map((one) => {
          const refused = refusedLine(automation, one);
          const detail = [
            one.name.trim() === "" ? null : whenLine(one, automation.roles),
            taskLine(one),
            roleLine(automation, one.roleId),
          ]
            .filter((part) => part !== null && part !== "")
            .join(" ・ ");
          return (
            <li key={one.id} className={refused === null ? undefined : "is-refused"}>
              <b>{one.name.trim() || whenLine(one, automation.roles)}</b>
              {detail !== "" && <span>{detail}</span>}
              {refused !== null && <span className="trigstage__refused">{refused}</span>}
            </li>
          );
        })}
      </ul>
    </>
  );
}

/** What a try said, as the preview's first line: the title of the task it would file, or why none. */
function triedLine(tried: AutomationTriggerTriedDto): string {
  switch (tried.kind) {
    case "files":
      return tried.title;
    case "condition":
      return t("auto.trigger.triedCondition");
    case "program_said_no":
      return t("auto.trigger.triedSaidNo");
    case "said_nothing":
      return tf("auto.trigger.triedNothing", { why: tried.why });
  }
}

/**
 * The task the trigger files, as it will stand: its title, the role, who works it and the trigger.
 * Where the card was tried as it now stands, what the try said takes the title's place — the title
 * and notes the task would be filed with, or why none would be.
 */
function Preview({
  automation,
  form,
  tried,
}: {
  automation: AutomationDetailDto;
  form: TriggerForm;
  tried: AutomationTriggerTriedDto | null;
}) {
  const role = automation.roles.find((one) => one.id === form.roleId);
  const typed =
    form.task === "fixed"
      ? form.title.trim()
      : t(form.task === "file" ? "auto.trigger.previewFromFile" : "auto.trigger.previewFromScript");
  const title = tried === null ? typed : triedLine(tried);
  const notes = tried?.kind === "files" ? tried.notes.trim() : "";
  return (
    <>
      <h3>{t("auto.trigger.preview")}</h3>
      <div className="trigstage__preview" data-tried={tried?.kind}>
        <span className="trigstage__previewtitle" data-preview="title">
          {title === "" ? t("auto.trigger.previewNoTitle") : title}
        </span>
        {notes !== "" && (
          <span className="trigstage__previewnotes" data-preview="notes">
            {notes}
          </span>
        )}
        <span className="autostep__hint" data-preview="line">
          {tf("auto.trigger.previewLine", {
            role: role?.name ?? t("auto.trigger.roleNone"),
            trigger: form.name.trim(),
          })}
        </span>
      </div>
    </>
  );
}

/**
 * **Part 1's condition, as one sentence**: "status is [ ], role is [ ], fewer than [ ] tasks → file
 * one". A status or role the boxes do not offer — several statuses, or a role no longer listed — is
 * offered as one more choice, so what was written stays picked; what the filter says besides is named
 * under the sentence, kept as it is.
 */
function Condition({
  automation,
  form,
  put,
}: {
  automation: AutomationDetailDto;
  form: TriggerForm;
  put: (patch: Partial<TriggerForm>) => void;
}) {
  const statuses: { value: string; label: string }[] = STATUS_ALL.map((one) => ({
    value: one,
    label: filterValueLabel("status", one),
  }));
  if (form.status !== "" && !statuses.some((one) => one.value === form.status)) {
    const values = form.status.split(",").filter((one) => one !== "");
    statuses.push({ value: form.status, label: values.map((one) => filterValueLabel("status", one)).join("/") });
  }
  statuses.push({ value: "", label: t("auto.trigger.anyStatus") });

  const roles: { value: string; label: string }[] = automation.roles.map((one) => ({
    value: String(one.id),
    label: one.name,
  }));
  if (form.countedRoleId !== null && !automation.roles.some((one) => one.id === form.countedRoleId)) {
    roles.push({ value: String(form.countedRoleId), label: String(form.countedRoleId) });
  }
  roles.push({ value: "", label: t("auto.trigger.anyRole") });

  return (
    <>
      <div className="trigcard__cond">
        <span>{t("auto.trigger.condStatus")}</span>
        <select
          aria-label={t("auto.trigger.condStatusPick")}
          value={form.status}
          onChange={(e) => put({ status: e.target.value })}
        >
          {statuses.map((one) => (
            <option key={one.value} value={one.value}>
              {one.label}
            </option>
          ))}
        </select>
        <span>{t("auto.trigger.condRole")}</span>
        <select
          aria-label={t("auto.trigger.condRolePick")}
          value={form.countedRoleId === null ? "" : String(form.countedRoleId)}
          onChange={(e) => put({ countedRoleId: e.target.value === "" ? null : Number(e.target.value) })}
        >
          {roles.map((one) => (
            <option key={one.value} value={one.value}>
              {one.label}
            </option>
          ))}
        </select>
        <span>{t("auto.trigger.condBelow")}</span>
        <input
          type="number"
          min={1}
          aria-label={t("auto.trigger.below")}
          value={form.below}
          onChange={(e) => put({ below: e.target.value })}
        />
        <span>{t("auto.trigger.belowUnit")}</span>
      </div>
      {form.filterRest.trim() !== "" && (
        <span className="autostep__hint" data-cond="kept">
          {tf("auto.trigger.condKept", { rest: form.filterRest.trim() })}
        </span>
      )}
    </>
  );
}

/** What the next look does, given `count` tasks match now — in the order core weighs it: a trigger
 *  switched off files nothing, nor does one whose condition does not hold, and otherwise it files at
 *  the next look its answer to 1 waits for. */
export function nowThen(form: TriggerForm, enabled: boolean, count: number): string {
  const below = Number(form.below);
  if (!Number.isInteger(below) || below < 1) return t("auto.trigger.nowNoBelow");
  if (!enabled) return t("auto.trigger.nowOff");
  if (count >= below) return tf("auto.trigger.nowFull", { below });
  if (form.when === "script" || form.asked) return t("auto.trigger.nowScript");
  if (form.when === "every") return t("auto.trigger.nowEvery");
  if (form.when === "idle") return t("auto.trigger.nowIdle");
  return t("auto.trigger.nowTasks");
}

/** "Now: N match → …" — how many tasks the condition, as typed, counts now, and what follows. */
function NowLine({
  automation,
  trigger,
  form,
}: {
  automation: AutomationDetailDto;
  trigger: AutomationTriggerDto | undefined;
  form: TriggerForm;
}) {
  const { count, error } = useTriggerCount(
    automation.id,
    joinFilter(form.status, form.filterRest),
    form.countedRoleId,
  );
  const line =
    error !== undefined
      ? tf("auto.trigger.nowRefused", { why: errText(error) })
      : count === null
        ? null
        : tf("auto.trigger.now", { count, then: nowThen(form, trigger?.enabled ?? true, count) });
  if (line === null) return null;
  return (
    <span className="autostep__hint" data-cond="now">
      {line}
    </span>
  );
}

/** The name the next try is given, so that closing the card stops that one and no other. */
let nextTryTag = 1;

export function AutomationTrigger({
  automation,
  trigger,
  onSaved,
  onClose,
}: {
  automation: AutomationDetailDto;
  /** The trigger to rewrite, or none to add one. */
  trigger?: AutomationTriggerDto;
  /** The trigger is written, or taken away. */
  onSaved?: () => void;
  onClose: () => void;
}) {
  const [form, setForm] = useState<TriggerForm>(() => formOf(trigger));
  const [busy, setBusy] = useState(false);
  const [refused, setRefused] = useState<string | null>(null);
  /** What the last try said, and the form it tried — shown only while the card still holds that form. */
  const [tried, setTried] = useState<{ form: TriggerForm; said: AutomationTriggerTriedDto } | null>(null);
  const [trying, setTrying] = useState(false);
  /** The tag of the try running now, or `null`, for closing the card to stop it. */
  const running = useRef<{ tag: number | null }>({ tag: null });
  const put = (patch: Partial<TriggerForm>) => setForm((now) => ({ ...now, ...patch }));
  const titleId = useId();
  const notesId = useId();
  const pathId = useId();

  const write = async (input: AutomationTriggerInputDto | null) => {
    setBusy(true);
    setRefused(null);
    try {
      if (input === null) {
        if (trigger !== undefined) await removeAutomationTrigger(trigger.id);
      } else if (trigger === undefined) {
        await addAutomationTrigger(automation.id, input);
      } else {
        await updateAutomationTrigger(trigger.id, input);
      }
      onClose();
      onSaved?.();
    } catch (e) {
      setRefused(errText(e));
      setBusy(false);
    }
  };

  const save = (e: FormEvent) => {
    e.preventDefault();
    if (!busy) void write(inputOf(form));
  };

  const remove = async () => {
    const name = trigger?.name ?? "";
    if (await confirmDialog(tf("auto.trigger.removeConfirm", { name }))) void write(null);
  };

  // Closing the card stops a try still running: what it would say is no longer read.
  useEffect(() => {
    const now = running.current;
    return () => {
      if (now.tag !== null) void stopTriggerTry(now.tag);
    };
  }, []);

  const tryNow = async () => {
    const tag = nextTryTag++;
    const typed = form;
    running.current.tag = tag;
    setTrying(true);
    setRefused(null);
    try {
      const said = await tryAutomationTrigger(automation.id, inputOf(typed), tag);
      setTried(said === null ? null : { form: typed, said });
    } catch (e) {
      setTried(null);
      setRefused(errText(e));
    } finally {
      running.current.tag = null;
      setTrying(false);
    }
  };

  const switchOn = (on: boolean) => {
    if (trigger === undefined) return;
    setRefused(null);
    setAutomationTriggerEnabled(trigger.id, on).catch((e: unknown) => setRefused(errText(e)));
  };

  return (
    <div className="trigstage">
      <div className="actbuild__canvashead">
        <span className="actbuild__sec">{t("auto.trigger.title")}</span>
      </div>
      <span className="autostep__hint">{t("auto.trigger.lead")}</span>
      <div className="trigstage__row">
        <form className="trigcard" noValidate onSubmit={save}>
          <div className="trigcard__head">
            <input
              {...asTyped}
              autoFocus
              className="trigcard__name"
              aria-label={t("auto.trigger.name")}
              placeholder={t("auto.trigger.newName")}
              value={form.name}
              onChange={(e) => put({ name: e.target.value })}
            />
            {trigger !== undefined && (
              <button
                type="button"
                className={`chip chip--press trigcard__switch${trigger.enabled ? " trigcard__switch--on" : ""}`}
                aria-pressed={trigger.enabled}
                onClick={() => switchOn(!trigger.enabled)}
              >
                {t(trigger.enabled ? "auto.trigger.enabled" : "auto.trigger.disabled")}
              </button>
            )}
            {trigger !== undefined && (
              <span className="autostep__hint trigcard__filed" data-head="filed">
                {trigger.lastFiledAt === undefined
                  ? t("auto.trigger.neverFiled")
                  : tf("auto.trigger.lastFiled", { at: dueLabel({ kind: "at", at: trigger.lastFiledAt }) })}
              </span>
            )}
          </div>

          {refused !== null && <ErrorNote tone="quiet">{refused}</ErrorNote>}

          <Part no={1} title={t("auto.trigger.when")}>
            <Radios
              label={t("auto.trigger.whenPick")}
              options={WHENS.map((one) => ({ id: one.id, label: one.label() }))}
              picked={form.when}
              onPick={(when) => put({ when })}
            />
            <div className="trigcard__sub">
              {form.when === "every" && (
                <span className="autostep__limit">
                  <input
                    type="number"
                    min={1}
                    aria-label={t("auto.trigger.minutes")}
                    value={form.minutes}
                    onChange={(e) => put({ minutes: e.target.value })}
                  />
                  {t("auto.trigger.minutesUnit")}
                </span>
              )}
              {form.when === "idle" && <span className="autostep__hint">{t("auto.trigger.idleHint")}</span>}
              {form.when !== "tasks" && (
                <label className="buttonrow">
                  <input
                    type="checkbox"
                    checked={form.counted}
                    onChange={(e) => put({ counted: e.target.checked })}
                  />
                  {t("auto.trigger.counted")}
                </label>
              )}
              {(form.when === "tasks" || form.counted) && (
                <>
                  <Condition automation={automation} form={form} put={put} />
                  <NowLine automation={automation} trigger={trigger} form={form} />
                </>
              )}
              {form.when !== "script" && (
                <label className="buttonrow">
                  <input
                    type="checkbox"
                    checked={form.asked}
                    onChange={(e) => put({ asked: e.target.checked })}
                  />
                  {t("auto.trigger.asked")}
                </label>
              )}
              {(form.when === "script" || form.asked) && (
                <>
                  <span className="autostep__hint">{t("auto.trigger.judgeHint")}</span>
                  <ScriptBoxesField boxes={form.judge} onChange={(judge) => put({ judge })} />
                </>
              )}
            </div>
          </Part>

          <Part no={2} title={t("auto.trigger.task")}>
            <Radios
              label={t("auto.trigger.taskPick")}
              options={TASKS.map((one) => ({ id: one.id, label: one.label() }))}
              picked={form.task}
              onPick={(task) => put({ task })}
            />
            {form.task === "fixed" && (
              <div className="trigcard__form">
                <label className="trigcard__lbl" htmlFor={titleId}>
                  {t("auto.trigger.taskTitle")}
                </label>
                <input {...asTyped} id={titleId} value={form.title} onChange={(e) => put({ title: e.target.value })} />
                <label className="trigcard__lbl" htmlFor={notesId}>
                  {t("auto.trigger.taskNotes")}
                </label>
                <textarea
                  {...asTyped}
                  id={notesId}
                  rows={3}
                  value={form.notes}
                  onChange={(e) => put({ notes: e.target.value })}
                />
              </div>
            )}
            {form.task === "file" && (
              <div className="trigcard__form">
                <label className="trigcard__lbl" htmlFor={pathId}>
                  {t("auto.trigger.taskPath")}
                </label>
                <div className="autostep__field">
                  <input
                    {...asTyped}
                    id={pathId}
                    className="autostep__mono"
                    value={form.path}
                    onChange={(e) => put({ path: e.target.value })}
                  />
                  <span className="autostep__hint">{t("auto.trigger.taskPathHint")}</span>
                </div>
              </div>
            )}
            {form.task === "script" && (
              <>
                <span className="autostep__hint">{t("auto.trigger.writerHint")}</span>
                <ScriptBoxesField boxes={form.writer} onChange={(writer) => put({ writer })} />
              </>
            )}
          </Part>

          <Part no={3} title={t("auto.trigger.role")}>
            <RolePick automation={automation} roleId={form.roleId} onPick={(roleId) => put({ roleId })} />
          </Part>

          <div className="trigcard__mach">
            <span aria-hidden="true">ⓘ</span>
            <span data-note="recorded">{tf("auto.trigger.recorded", { name: form.name.trim() })}</span>
          </div>

          <div className="trigcard__buttons">
            {trigger !== undefined && (
              <button type="button" className="btn" disabled={busy} onClick={() => void remove()}>
                {t("auto.trigger.remove")}
              </button>
            )}
            <span className="trigcard__sp" />
            <button type="button" className="btn" disabled={busy} onClick={onClose}>
              {t("auto.add.cancel")}
            </button>
            <button
              type="button"
              className="btn"
              disabled={busy || trying}
              aria-busy={trying}
              onClick={() => void tryNow()}
            >
              {t(trying ? "auto.trigger.trying" : "auto.trigger.try")}
            </button>
            <button type="submit" className="btn btn--primary" disabled={busy}>
              {t("auto.trigger.save")}
            </button>
          </div>
        </form>
        <div className="trigstage__notes">
          <TriggerList automation={automation} />
          <Preview automation={automation} form={form} tried={tried?.form === form ? tried.said : null} />
        </div>
      </div>
    </div>
  );
}
