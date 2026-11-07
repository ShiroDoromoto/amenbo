// **What the automation holds as a whole, down the build screen's right** (`./AutomationBuildScreen`):
// who a failed round is filed to, and its triggers. The roles stand down the left and what is built for
// the picked one in the middle; these two are about none of the roles in particular, so they stand
// beside both, always in view, each in a box of its own. How many go at once is no longer the
// automation's to say: it is to be said role by role, by how many each has (`AMB-D-1123`).
//
// **Every press here writes at once**, the way the "Edit" panel's fields do (`./AutomationAboutPanel`).
// A trigger is a setting kept apart from the saved versions (`AMB-D-1119`), so the head's "Save" and
// "Discard changes" neither keep nor undo it. A refusal is drawn once, at the top of the column, in the
// words core wrote.
//
// **Who failures are filed to is marked required.** A launch is refused while nobody is chosen
// (`not_ready_automation_no_failures_to`), so a definition with none opens on "Choose one", which cannot
// be picked back once something else is. The roles offered are this automation's own, in their order.
//
// **The column lists every trigger the automation has** (`AMB-D-1119`), in its order, one box each:
// when it files a task, what the task says, and the role it is worked under. Pressing a box opens the
// dialog on that trigger, to change it or take it away (`./AutomationTrigger`). "+ Add a trigger"
// stands under the list however many there are, and opens the same dialog on a new one; with none,
// the column says so above it.
import { useState } from "react";
import { editAutomation } from "../core/automations";
import { errText, t, tf } from "../core/i18n";
import { ErrorNote } from "../components/ErrorNote";
import { AutomationTrigger } from "./AutomationTrigger";
import { filterValueLabel } from "./automationParts";
import type {
  AutomationDetailDto,
  AutomationFailuresToDto,
  AutomationTriggerConditionDto,
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
function countedWords(condition: AutomationTriggerConditionDto, roles: AutomationDetailDto["roles"]): string {
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
function belowLine(condition: AutomationTriggerConditionDto, roles: AutomationDetailDto["roles"]): string {
  const counted = countedWords(condition, roles);
  return counted === ""
    ? tf("auto.side.below", { below: condition.below })
    : tf("auto.side.belowOf", { what: counted, below: condition.below });
}

/** When the trigger files a task, in the words of the dialog's ①, with what its condition counts. */
function whenLine(trigger: AutomationTriggerDto, roles: AutomationDetailDto["roles"]): string {
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
function taskLine(trigger: AutomationTriggerDto): string {
  switch (trigger.task.kind) {
    case "fixed":
      return trigger.task.title;
    case "file":
      return trigger.task.path;
    case "script":
      return t("auto.trigger.taskScript");
  }
}

export function AutomationSettings({ automation }: { automation: AutomationDetailDto }) {
  const [refused, setRefused] = useState<string | null>(null);
  // The trigger whose dialog is open, by its id, or "new" for one being added.
  const [opened, setOpened] = useState<number | "new" | null>(null);
  const failures = failuresValue(automation.failuresTo);
  const roleLine = (trigger: AutomationTriggerDto) => {
    const role = automation.roles.find((one) => one.id === trigger.roleId);
    return role === undefined ? t("auto.side.triggerRoleNone") : tf("auto.side.triggerRole", { role: role.name });
  };

  const write = (patch: Parameters<typeof editAutomation>[1]) => {
    setRefused(null);
    editAutomation(automation.id, patch).catch((e: unknown) => setRefused(errText(e)));
  };

  return (
    <div className="autoside">
      <span className="actbuild__sec">{t("auto.side.title")}</span>
      {refused !== null && <ErrorNote tone="quiet">{refused}</ErrorNote>}

      <label className="autoside__group">
        <span className="autostep__label">
          {t("auto.side.failuresTo")}
          <span className="autostep__req">{t("auto.side.required")}</span>
        </span>
        <select
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
          {automation.roles.map((role) => (
            <option key={role.id} value={`role:${role.id}`}>
              {tf("auto.side.failuresRole", { role: role.name })}
            </option>
          ))}
          <option value="human">{t("auto.side.failuresHuman")}</option>
        </select>
        <span className="autostep__hint">{t("auto.side.failuresHint")}</span>
      </label>

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
                  onClick={() => setOpened(trigger.id)}
                >
                  <span className="autoside__when">{whenLine(trigger, automation.roles)}</span>
                  <span className="autoside__what">{taskLine(trigger)}</span>
                  <span className="autostep__hint">{roleLine(trigger)}</span>
                </button>
              </li>
            ))}
          </ul>
        )}
        <button type="button" className="btn autoside__add" onClick={() => setOpened("new")}>
          {t("auto.side.triggerAdd")}
        </button>
      </div>

      {opened !== null && (
        <AutomationTrigger
          automation={automation}
          trigger={opened === "new" ? undefined : automation.triggers.find((one) => one.id === opened)}
          onClose={() => setOpened(null)}
        />
      )}
    </div>
  );
}
