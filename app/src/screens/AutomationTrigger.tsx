// **An automation's trigger, written on a dialog of its own** — what files a task without a person and
// starts a run of this automation with it. The build screen's right column opens it
// (`./AutomationSettings`); the dialog itself stands alone, given the automation and told when it is
// done.
//
// **It asks three things, numbered as the reader decides them**: ① whether to file a task now (every so
// many minutes, when no run of it is going, while fewer than N tasks match a filter, or when a script
// says so), ② what the task says (fixed words, a file read from its full path, or what a script
// writes), and ③ the role the task is worked under, or none, which leaves it to a session. Neither ①
// nor ② asks an AI: both are decided mechanically, and the AI works only inside the run the task
// starts.
//
// **It writes the one trigger it is opened on**, or adds a new one when opened on none, and can take
// the one it is opened on away. An automation may have more than one; the build screen lists them all
// and says which to open, and the others are left as they are. It is a setting, not part of the
// definition: the save writes it at once, and the build screen's "Save" and "Discard changes" neither
// keep nor undo it. When it is next asked is counted by core from the write, so nothing here sends it.
//
// **A condition and a script can go with any ①**, on top of it. "While fewer than N tasks match" is
// the condition with nothing else, and "when a script says so" the script with nothing else, so there
// each cannot be switched off.
//
// **What core refuses** — a wait under a minute, a task with no title, a filter that does not read —
// comes back as core's own sentence, drawn at the top, and the dialog stays open with what was typed.
//
// **Only its buttons close it**, as with the other dialogs of the build screen: a stray press on the
// backdrop or Escape would throw away what was typed.
import { useState } from "react";
import { createPortal } from "react-dom";
import { addAutomationTrigger, removeAutomationTrigger, updateAutomationTrigger } from "../core/automations";
import { confirmDialog } from "../core/dialog";
import { errText, t, tf } from "../core/i18n";
import { asTyped } from "../core/keys";
import { ErrorNote } from "../components/ErrorNote";
import type {
  AutomationDetailDto,
  AutomationScriptInputDto,
  AutomationStepScriptDto,
  AutomationTriggerDto,
  AutomationTriggerInputDto,
} from "../bindings/bindings";

/** The four answers to ① — whether to file a task now. */
export type WhenKind = "every" | "idle" | "tasks" | "script";
/** The three answers to ② — what the task says. */
export type TaskKind = "fixed" | "file" | "script";

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

/** Everything the dialog holds, as typed. */
export type TriggerForm = {
  name: string;
  when: WhenKind;
  minutes: string;
  /** Whether a condition goes with an ① other than "tasks", which always has one. */
  counted: boolean;
  filter: string;
  below: string;
  /** Whether a script is asked with an ① other than "script", which always asks one. */
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

/** The form a trigger opens with: the one written, or a new one that files every hour. */
export function formOf(trigger: AutomationTriggerDto | undefined): TriggerForm {
  return {
    name: trigger?.name ?? "",
    when: trigger?.when.kind ?? "every",
    minutes: trigger?.when.kind === "every" ? String(trigger.when.minutes) : "60",
    counted: trigger?.condition !== undefined,
    filter: trigger?.condition?.filter ?? "",
    below: trigger?.condition === undefined ? "1" : String(trigger.condition.below),
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
        : { kind: "script", script: scriptOf(form.writer) };
  return {
    name: form.name.trim(),
    roleId: form.roleId ?? undefined,
    when,
    condition:
      form.when === "tasks" || form.counted
        ? { filter: form.filter.trim(), below: Number(form.below) }
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

/** A row of chips, one of them pressed. */
function Chips<K extends string | number>({
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
  return (
    <div className="buttonrow" role="group" aria-label={label}>
      {options.map((one) => (
        <button
          key={one.id}
          type="button"
          className={picked === one.id ? "actchip actchip--on" : "actchip"}
          aria-pressed={picked === one.id}
          onClick={() => onPick(one.id)}
        >
          {one.label}
        </button>
      ))}
    </div>
  );
}

/** The task the trigger files, as it will stand: its title, the role, who works it and the trigger. */
function Preview({ automation, form }: { automation: AutomationDetailDto; form: TriggerForm }) {
  const role = automation.roles.find((one) => one.id === form.roleId);
  const title =
    form.task === "fixed"
      ? form.title.trim()
      : t(form.task === "file" ? "auto.trigger.previewFromFile" : "auto.trigger.previewFromScript");
  return (
    <section className="autodlg__stack" aria-labelledby="auto-trigger-preview">
      <h3 className="autodlg__title" id="auto-trigger-preview">
        {t("auto.trigger.preview")}
      </h3>
      <div>
        <div data-preview="title">{title === "" ? t("auto.trigger.previewNoTitle") : title}</div>
        <span className="autostep__hint" data-preview="line">
          {tf("auto.trigger.previewLine", {
            role: role?.name ?? t("auto.trigger.roleNone"),
            trigger: form.name.trim(),
          })}
        </span>
      </div>
    </section>
  );
}

export function AutomationTrigger({
  automation,
  trigger: opened,
  onSaved,
  onClose,
}: {
  automation: AutomationDetailDto;
  /** The trigger to change or take away, or none to add a new one. */
  trigger?: AutomationTriggerDto;
  /** The trigger is written, or taken away. */
  onSaved?: () => void;
  onClose: () => void;
}) {
  const [form, setForm] = useState<TriggerForm>(() => formOf(opened));
  const [busy, setBusy] = useState(false);
  const [refused, setRefused] = useState<string | null>(null);
  const put = (patch: Partial<TriggerForm>) => setForm((now) => ({ ...now, ...patch }));

  const write = async (trigger: AutomationTriggerInputDto | null) => {
    setBusy(true);
    setRefused(null);
    try {
      if (trigger === null) {
        if (opened !== undefined) await removeAutomationTrigger(opened.id);
      } else if (opened === undefined) {
        await addAutomationTrigger(automation.id, trigger);
      } else {
        await updateAutomationTrigger(opened.id, trigger);
      }
      onClose();
      onSaved?.();
    } catch (e) {
      setRefused(errText(e));
      setBusy(false);
    }
  };

  const save = () => {
    if (!busy) void write(inputOf(form));
  };

  const remove = async () => {
    const name = opened?.name ?? "";
    if (await confirmDialog(tf("auto.trigger.removeConfirm", { name }))) void write(null);
  };

  return createPortal(
    <div className="modal__overlay">
      <div
        className="modal__card modal__card--wide autodlg__stack"
        role="dialog"
        aria-modal="true"
        aria-labelledby="auto-trigger-title"
      >
        <h2 className="autodlg__title" id="auto-trigger-title">
          {t("auto.trigger.title")}
        </h2>
        <span className="autostep__hint">{t("auto.trigger.lead")}</span>

        {refused !== null && <ErrorNote tone="quiet">{refused}</ErrorNote>}

        <label className="autostep__field">
          <span className="autostep__label">{t("auto.trigger.name")}</span>
          <input
            {...asTyped}
            autoFocus
            value={form.name}
            onChange={(e) => put({ name: e.target.value })}
          />
        </label>

        <section className="autodlg__stack" aria-labelledby="auto-trigger-when">
          <h3 className="autodlg__title" id="auto-trigger-when">
            {t("auto.trigger.when")}
          </h3>
          <Chips
            label={t("auto.trigger.when")}
            options={WHENS.map((one) => ({ id: one.id, label: one.label() }))}
            picked={form.when}
            onPick={(when) => put({ when })}
          />
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
              <div className="autostep__pair autostep__pair--top">
                <span className="autostep__label">{t("auto.trigger.filter")}</span>
                <div className="autostep__field">
                  <input
                    {...asTyped}
                    className="autostep__mono"
                    aria-label={t("auto.trigger.filter")}
                    value={form.filter}
                    onChange={(e) => put({ filter: e.target.value })}
                  />
                  <span className="autostep__hint">{t("auto.trigger.filterHint")}</span>
                </div>
              </div>
              <span className="autostep__limit">
                <input
                  type="number"
                  min={1}
                  aria-label={t("auto.trigger.below")}
                  value={form.below}
                  onChange={(e) => put({ below: e.target.value })}
                />
                {t("auto.trigger.belowUnit")}
              </span>
            </>
          )}
        </section>

        <section className="autodlg__stack" aria-labelledby="auto-trigger-task">
          <h3 className="autodlg__title" id="auto-trigger-task">
            {t("auto.trigger.task")}
          </h3>
          <Chips
            label={t("auto.trigger.task")}
            options={TASKS.map((one) => ({ id: one.id, label: one.label() }))}
            picked={form.task}
            onPick={(task) => put({ task })}
          />
          {form.task === "fixed" && (
            <>
              <label className="autostep__field">
                <span className="autostep__label">{t("auto.trigger.taskTitle")}</span>
                <input {...asTyped} value={form.title} onChange={(e) => put({ title: e.target.value })} />
              </label>
              <label className="autostep__field">
                <span className="autostep__label">{t("auto.trigger.taskNotes")}</span>
                <textarea rows={4} value={form.notes} onChange={(e) => put({ notes: e.target.value })} />
              </label>
            </>
          )}
          {form.task === "file" && (
            <label className="autostep__field">
              <span className="autostep__label">{t("auto.trigger.taskPath")}</span>
              <input
                {...asTyped}
                className="autostep__mono"
                value={form.path}
                onChange={(e) => put({ path: e.target.value })}
              />
              <span className="autostep__hint">{t("auto.trigger.taskPathHint")}</span>
            </label>
          )}
          {form.task === "script" && (
            <>
              <span className="autostep__hint">{t("auto.trigger.writerHint")}</span>
              <ScriptBoxesField boxes={form.writer} onChange={(writer) => put({ writer })} />
            </>
          )}
        </section>

        <section className="autodlg__stack" aria-labelledby="auto-trigger-role">
          <h3 className="autodlg__title" id="auto-trigger-role">
            {t("auto.trigger.role")}
          </h3>
          <Chips<number | "none">
            label={t("auto.trigger.role")}
            options={[
              ...automation.roles.map((one) => ({ id: one.id as number | "none", label: one.name })),
              { id: "none", label: t("auto.trigger.roleNone") },
            ]}
            picked={form.roleId ?? "none"}
            onPick={(id) => put({ roleId: id === "none" ? null : id })}
          />
        </section>

        <span className="autostep__hint" data-note="recorded">
          {tf("auto.trigger.recorded", { name: form.name.trim() })}
        </span>

        <Preview automation={automation} form={form} />

        <div className="buttonrow">
          <button type="button" className="btn btn--primary" disabled={busy} onClick={save}>
            {t("auto.trigger.save")}
          </button>
          <button type="button" className="btn" disabled={busy} onClick={onClose}>
            {t("auto.add.cancel")}
          </button>
          {opened !== undefined && (
            <button type="button" className="btn" disabled={busy} onClick={() => void remove()}>
              {t("auto.trigger.remove")}
            </button>
          )}
        </div>
      </div>
    </div>,
    document.body,
  );
}
