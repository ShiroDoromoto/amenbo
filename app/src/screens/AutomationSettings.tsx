// **What the automation holds as a whole, down the build screen's right** (`./AutomationBuildScreen`):
// how many of its runs may go at once, who a failed round is filed to, and its trigger. The roles stand
// down the left and what is built for the picked one in the middle; these three are about none of the
// roles in particular, so they stand beside both, always in view.
//
// **Every press here writes at once**, the way the "Edit" panel's fields do (`./AutomationAboutPanel`).
// The trigger is a setting kept apart from the saved versions (`AMB-D-1119`), so the head's "Save" and
// "Discard changes" neither keep nor undo it. A refusal is drawn once, at the top of the column, in the
// words core wrote.
//
// **The most runs at once is a stepper from 1 to `MAX_CONCURRENT_RUNS`.** It is 1 until somebody
// changes it, and there is no "no limit" (`AMB-D-947` as `AMB-D-1119` rewrote it). A press past either
// end cannot be made, so nothing outside the range is ever written.
//
// **Who failures are filed to is marked required.** A launch is refused while nobody is chosen
// (`not_ready_automation_no_failures_to`), so a definition with none opens on "Choose one", which cannot
// be picked back once something else is. The roles offered are this automation's own, in their order.
//
// **The column draws the automation's first trigger** (`AMB-D-1119`); an automation may have more,
// and listing them all is still to come. Without one the column says so and offers "+ Add a trigger";
// with one it draws what it files and when, and pressing it opens the same dialog to change it or take
// it away (`./AutomationTrigger`).
import { useState } from "react";
import { editAutomation, MAX_CONCURRENT_RUNS } from "../core/automations";
import { errText, t, tf } from "../core/i18n";
import { ErrorNote } from "../components/ErrorNote";
import { AutomationTrigger } from "./AutomationTrigger";
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

/** When the trigger files a task, in the words of the dialog's ①. */
function whenLine(trigger: AutomationTriggerDto): string {
  const when =
    trigger.when.kind === "every"
      ? tf("auto.side.every", { minutes: trigger.when.minutes })
      : trigger.when.kind === "idle"
        ? t("auto.trigger.whenIdle")
        : trigger.when.kind === "tasks"
          ? t("auto.trigger.whenTasks")
          : t("auto.trigger.whenScript");
  const below = trigger.condition === undefined ? null : tf("auto.side.below", { below: trigger.condition.below });
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
  const [triggering, setTriggering] = useState(false);
  const most = automation.maxConcurrentRuns;
  const failures = failuresValue(automation.failuresTo);
  // The first of its triggers, as the dialog writes it.
  const trigger = automation.triggers[0];
  const triggerRoleId = trigger?.roleId;
  const triggerRole =
    triggerRoleId === undefined ? undefined : automation.roles.find((one) => one.id === triggerRoleId);

  const write = (patch: Parameters<typeof editAutomation>[1]) => {
    setRefused(null);
    editAutomation(automation.id, patch).catch((e: unknown) => setRefused(errText(e)));
  };

  return (
    <div className="autoside">
      <span className="actbuild__sec">{t("auto.side.title")}</span>
      {refused !== null && <ErrorNote tone="quiet">{refused}</ErrorNote>}

      <div className="autoside__group">
        <span className="autostep__label" id="autoside-most">{t("auto.side.mostRuns")}</span>
        <div className="autoside__stepline">
          <span className="autoside__stepper" role="group" aria-labelledby="autoside-most">
            <button
              type="button"
              aria-label={t("auto.side.fewer")}
              disabled={most <= 1}
              onClick={() => write({ maxConcurrentRuns: most - 1 })}
            >
              −
            </button>
            <output aria-live="polite">{most}</output>
            <button
              type="button"
              aria-label={t("auto.side.more")}
              disabled={most >= MAX_CONCURRENT_RUNS}
              onClick={() => write({ maxConcurrentRuns: most + 1 })}
            >
              ＋
            </button>
          </span>
          <span className="autostep__hint">{tf("auto.side.cap", { n: MAX_CONCURRENT_RUNS })}</span>
        </div>
        <span className="autostep__hint">{t("auto.side.mostRunsHint")}</span>
      </div>

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
        {trigger === undefined ? (
          <>
            <span className="autostep__hint">{t("auto.side.triggerNone")}</span>
            <button type="button" className="btn autoside__add" onClick={() => setTriggering(true)}>
              {t("auto.side.triggerAdd")}
            </button>
          </>
        ) : (
          <button
            type="button"
            className="autoside__trigger"
            title={t("auto.trigger.title")}
            onClick={() => setTriggering(true)}
          >
            <span className="autoside__when">{trigger.name.trim() || whenLine(trigger)}</span>
            {trigger.name.trim() !== "" && <span className="autostep__hint">{whenLine(trigger)}</span>}
            <span className="autoside__what">{taskLine(trigger)}</span>
            <span className="autostep__hint">
              {triggerRole === undefined
                ? t("auto.side.triggerRoleNone")
                : tf("auto.side.triggerRole", { role: triggerRole.name })}
            </span>
          </button>
        )}
      </div>

      {triggering && <AutomationTrigger automation={automation} onClose={() => setTriggering(false)} />}
    </div>
  );
}
