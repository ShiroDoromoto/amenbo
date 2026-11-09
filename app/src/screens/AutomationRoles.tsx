// The roles of one automation, on its build screen: the list down the left with the press that adds
// one, the head of the role picked from it — its icon and the press that draws it anew, its name, and
// how its round starts — and the agent and model its steps fall back to where a step chose none
// (`AMB-D-960`). The head and the agent and model are two pieces, because the picture of how the role
// works stands between them.
//
// **How a role starts is two presses side by side under its head** (`AMB-D-1127`): by a task of the
// role turning up, or by a trigger aimed at it. A role just added starts by a task. Pressing the other
// takes away where its round starts (`AMB-D-1142`) and leaves what is placed on the picture; a role
// failed rounds are filed to is not made to start by a trigger, and core's sentence says so under the
// head. A role started by a trigger names the triggers aimed at it on a second line, each pressed to
// open its card in the middle.
//
// **Every write here goes to the draft** (`AMB-D-1005`): a role is in the version the next save makes,
// and "Discard changes" takes it back the way it takes back a box.
//
// **The count on a row is the steps its round reaches**: the steps of every placement whose `roleId`
// is the role's. The build screen writes no step (`AMB-D-949`), so the count is read, not chosen.
//
// **The agent and model are chosen the way a step's are** (`./AutomationStepPanel`): the agents this
// project could start, one not on this machine named but not pickable, and the models the agent's own
// CLI lists (`AMB-D-865`). A new agent puts the model back to the agent's own default.
import { useState } from "react";
import {
  addAutomationRole,
  editAutomationRole,
  moveAutomationRole,
  removeAutomationRole,
} from "../core/automations";
import { confirmDialog } from "../core/dialog";
import { errText, t, tf } from "../core/i18n";
import { asTyped } from "../core/keys";
import { Identicon } from "../components/identicon";
import { useAgents, useDraft, useModels } from "./automationPanel";
import { whenLine } from "./AutomationTrigger";
import type { AutomationDetailDto, AutomationRoleDto, AutomationTriggerDto } from "../bindings/bindings";

/** The two ways a role's round starts, in the order they are offered in. */
const STARTS_BY: { id: AutomationRoleDto["startsBy"]; label: () => string }[] = [
  { id: "task", label: () => t("auto.build.roleStartsTask") },
  { id: "trigger", label: () => t("auto.build.roleStartsTrigger") },
];

/** How many steps the role's round reaches, over every placement it is on. */
export function roleStepCount(automation: AutomationDetailDto, roleId: number): number {
  return automation.placements
    .filter((one) => one.roleId === roleId)
    .reduce((sum, one) => sum + one.steps.length, 0);
}

/** A role's picture, drawn from its icon seed, or from what the role is called until one is written.
 *  It takes any shape that carries the two, so a task's role is drawn by the same rule. */
export function RoleIcon({ role, size }: { role: { icon?: string | null; name: string }; size: number }) {
  return <Identicon seed={role.icon ?? role.name} size={size} />;
}

/** The roles down the left, the picked one marked, and the press that adds one under them. */
export function AutomationRoleList({
  automation,
  pickedId,
  onPick,
  onAdded,
}: {
  automation: AutomationDetailDto;
  pickedId: number | null;
  onPick: (roleId: number) => void;
  /** Called once the new role is written, with how many roles there were at the press: the new one
   *  goes after them. */
  onAdded: (before: number) => void;
}) {
  const [refused, setRefused] = useState<string | null>(null);
  const add = () => {
    setRefused(null);
    const before = automation.roles.length;
    addAutomationRole(automation.id, tf("auto.build.roleNew", { n: before + 1 }))
      .then(() => onAdded(before))
      .catch((e: unknown) => setRefused(errText(e)));
  };
  return (
    <div className="autoroles">
      <span className="actbuild__sec">{t("auto.build.roles")}</span>
      {automation.roles.length > 0 && (
        <ul className="autoroles__list">
          {automation.roles.map((role) => {
            const steps = roleStepCount(automation, role.id);
            return (
              <li key={role.id}>
                <button
                  type="button"
                  className="autoroles__row"
                  aria-current={role.id === pickedId ? "true" : undefined}
                  onClick={() => onPick(role.id)}
                >
                  <RoleIcon role={role} size={16} />
                  <span className="autoroles__name">{role.name}</span>
                  <span className="autoroles__count" title={tf("auto.build.roleSteps", { n: steps })}>
                    {steps}
                  </span>
                </button>
              </li>
            );
          })}
        </ul>
      )}
      <button type="button" className="btn autoroles__add" onClick={add}>
        {t("auto.build.roleAdd")}
      </button>
      {refused !== null && <span className="actpanel__refused" role="alert">{refused}</span>}
    </div>
  );
}

/**
 * The picked role's head: its icon and name, where it stands in the list, the press that removes it,
 * how its round starts, and — for one started by a trigger — the triggers aimed at it.
 */
export function AutomationRoleHead({
  automation,
  role,
  onRemoved,
  onOpenTrigger,
}: {
  automation: AutomationDetailDto;
  role: AutomationRoleDto;
  onRemoved: () => void;
  /** Open a trigger's card in the middle — the one pressed on the line of triggers aimed at the role. */
  onOpenTrigger: (trigger: AutomationTriggerDto) => void;
}) {
  const [name, setName] = useDraft(role.name);
  const [refused, setRefused] = useState<string | null>(null);
  const run = (write: Promise<void>) => {
    setRefused(null);
    write.catch((e: unknown) => setRefused(errText(e)));
  };
  const rename = () => {
    if (name.trim() === "" || name === role.name) {
      setName(role.name);
      return;
    }
    setRefused(null);
    editAutomationRole(role.id, { name }).catch((e: unknown) => {
      setName(role.name);
      setRefused(errText(e));
    });
  };
  const aimed = automation.triggers.filter((one) => one.roleId === role.id);
  const at = automation.roles.findIndex((one) => one.id === role.id);
  const before = automation.roles[at - 1];
  const after = automation.roles[at + 1];
  const remove = async () => {
    if (!(await confirmDialog(tf("auto.build.roleRemoveConfirm", { name: role.name })))) return;
    try {
      setRefused(null);
      await removeAutomationRole(role.id);
      onRemoved();
    } catch (e) {
      setRefused(errText(e));
    }
  };
  return (
    <div className="autorole">
      <div className="autorole__head">
        <RoleIcon role={role} size={32} />
        <input
          {...asTyped}
          className="autorole__name"
          aria-label={t("auto.build.roleName")}
          value={name}
          onChange={(e) => setName(e.target.value)}
          onBlur={rename}
        />
        <button
          type="button"
          className="btn"
          onClick={() => run(editAutomationRole(role.id, { icon: crypto.randomUUID() }))}
        >
          {t("auto.build.roleIconRedo")}
        </button>
        <button
          type="button"
          className="btn"
          disabled={before === undefined}
          onClick={() => before !== undefined && run(moveAutomationRole(role.id, "before", before.id))}
        >
          {t("auto.build.roleUp")}
        </button>
        <button
          type="button"
          className="btn"
          disabled={after === undefined}
          onClick={() => after !== undefined && run(moveAutomationRole(role.id, "after", after.id))}
        >
          {t("auto.build.roleDown")}
        </button>
        <button type="button" className="btn" onClick={() => void remove()}>
          {t("auto.build.roleRemove")}
        </button>
      </div>
      <div className="autorole__start">
        <div className="autorole__startline">
          <span className="autorole__startlabel">{t("auto.build.roleStartsBy")}</span>
          <span className="autorole__seg" role="group" aria-label={t("auto.build.roleStartsBy")}>
            {STARTS_BY.map((one) => (
              <button
                key={one.id}
                type="button"
                aria-pressed={role.startsBy === one.id}
                onClick={() => role.startsBy !== one.id && run(editAutomationRole(role.id, { startsBy: one.id }))}
              >
                {one.label()}
              </button>
            ))}
          </span>
        </div>
        {role.startsBy === "trigger" && (
          <div className="autorole__startline">
            <span className="autorole__startlabel">{t("auto.build.roleTriggers")}</span>
            <span className="autorole__triggers">
              {aimed.length === 0 && <span className="autorole__startlabel">{t("auto.act.none")}</span>}
              {aimed.map((one) => (
                <button key={one.id} type="button" className="autorole__trigger" onClick={() => onOpenTrigger(one)}>
                  {one.name.trim() === ""
                    ? whenLine(one, automation.roles)
                    : tf("auto.build.roleTrigger", { name: one.name.trim(), when: whenLine(one, automation.roles) })}
                </button>
              ))}
            </span>
          </div>
        )}
      </div>
      {refused !== null && <span className="actpanel__refused" role="alert">{refused}</span>}
    </div>
  );
}

/** The picked role's default agent and model, under the picture of how it works. */
export function AutomationRoleWho({
  automation,
  role,
}: {
  automation: AutomationDetailDto;
  role: AutomationRoleDto;
}) {
  const [refused, setRefused] = useState<string | null>(null);
  const agents = useAgents(automation.projectId);
  const models = useModels(role.agent ?? "");
  const run = (write: Promise<void>) => {
    setRefused(null);
    write.catch((e: unknown) => setRefused(errText(e)));
  };
  return (
    <div className="autorole autorole--who">
      <span className="actbuild__sec">{t("auto.build.roleWho")}</span>
      <div className="autorole__who">
        <label htmlFor={`role-agent-${role.id}`}>{t("auto.build.roleAgent")}</label>
        <select
          id={`role-agent-${role.id}`}
          value={role.agent ?? ""}
          onChange={(e) =>
            run(editAutomationRole(role.id, e.target.value === "" ? { agent: null } : { agent: e.target.value, model: null }))
          }
        >
          <option value="">{t("auto.place.agentNone")}</option>
          {role.agent !== undefined && agents.every((one) => one.id !== role.agent) && (
            <option value={role.agent}>{role.agent}</option>
          )}
          {agents.map((one) => (
            <option key={one.id} value={one.id} disabled={!one.installed}>
              {one.installed ? one.label : tf("auto.step.notHere", { agent: one.label })}
            </option>
          ))}
        </select>
        <label htmlFor={`role-model-${role.id}`}>{t("auto.build.roleModel")}</label>
        <select
          id={`role-model-${role.id}`}
          value={role.model ?? ""}
          disabled={role.agent === undefined}
          onChange={(e) => run(editAutomationRole(role.id, { model: e.target.value === "" ? null : e.target.value }))}
        >
          <option value="">{t("auto.step.modelDefault")}</option>
          {role.model !== undefined && (models?.models ?? []).every((one) => one.id !== role.model) && (
            <option value={role.model}>{role.model}</option>
          )}
          {(models?.models ?? []).map((one) => (
            <option key={one.id} value={one.id}>
              {one.label}
            </option>
          ))}
        </select>
      </div>
      {refused !== null && <span className="actpanel__refused" role="alert">{refused}</span>}
    </div>
  );
}
