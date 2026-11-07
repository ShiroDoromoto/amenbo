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
// when it files a task, what the task says, and the role it is worked under. "+ Add a trigger" stands
// under the list however many there are; with none, the column says so above it. Pressing a box, or
// "+ Add a trigger", asks the build screen to open that trigger's card in its middle
// (`./AutomationTrigger`), to change it or take it away, or to write a new one.
import { useState } from "react";
import { editAutomation } from "../core/automations";
import { errText, t, tf } from "../core/i18n";
import { ErrorNote } from "../components/ErrorNote";
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

export function AutomationSettings({
  automation,
  onOpenTrigger,
}: {
  automation: AutomationDetailDto;
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
    </div>
  );
}
