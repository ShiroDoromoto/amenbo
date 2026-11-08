// The role field of the task editor: which of the project's roles the task is worked under, or none —
// a task with no role is worked in a session rather than in an automation's round. Every choice is in
// sight at once, each role led by its picture, and "no role" spans the grid below them on a dashed
// line, being the choice to take the task out of every round rather than one more role.
import { useId } from "react";
import { t } from "../core/i18n";
import { RoleIcon } from "../screens/AutomationRoles";
import type { TaskRoleDto } from "../bindings/bindings";

/** The value "none" is the choice of no role, and "" is nothing chosen yet. */
export const ROLE_NONE = "none";

/**
 * `value` is a role's id as a string, `ROLE_NONE`, or "" while nothing is chosen — then no choice is
 * checked, and once one is there is no going back to nothing chosen. With roles from more than one
 * automation each is named with its automation, since two may share a name.
 */
export function RoleField({
  roles, value, onChange,
}: {
  roles: TaskRoleDto[];
  value: string;
  onChange: (value: string) => void;
}) {
  const name = useId();
  const several = new Set(roles.map((r) => r.automationId)).size > 1;
  const choice = (key: string, label: string, role: TaskRoleDto | null) => (
    <label className={role ? "rolepick__one" : "rolepick__one rolepick__one--none"} key={key}>
      <input type="radio" name={name} value={key} checked={value === key} onChange={() => onChange(key)} />
      {role && <RoleIcon role={role} size={16} />}
      <span>{label}</span>
    </label>
  );
  return (
    <div className="rolepick" role="radiogroup" aria-label={t("detail.role")}>
      {roles.map((r) => choice(String(r.id), several ? `${r.automationName} · ${r.name}` : r.name, r))}
      {choice(ROLE_NONE, t("detail.roleNone"), null)}
    </div>
  );
}
