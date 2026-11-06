// The role field of the task editor: which of the project's roles the task is worked under, or none —
// a task with no role is worked in a session rather than in an automation's round.
import { t } from "../core/i18n";
import type { TaskRoleDto } from "../bindings/bindings";

/** The value "none" is the choice of no role, and "" is nothing chosen yet. */
export const ROLE_NONE = "none";

/**
 * `value` is a role's id as a string, `ROLE_NONE`, or "" while nothing is chosen. "" is offered as a
 * choice of its own only while it is the value, so a person cannot go back to it. With roles from
 * more than one automation each is named with its automation, since two may share a name.
 */
export function RoleField({
  roles, value, onChange,
}: {
  roles: TaskRoleDto[];
  value: string;
  onChange: (value: string) => void;
}) {
  const several = new Set(roles.map((r) => r.automationId)).size > 1;
  return (
    <select className="btn" aria-label={t("detail.role")} value={value} onChange={(e) => onChange(e.target.value)}>
      {value === "" && <option value="" disabled>{t("detail.rolePick")}</option>}
      {roles.map((r) => (
        <option value={String(r.id)} key={r.id}>{several ? `${r.automationName} · ${r.name}` : r.name}</option>
      ))}
      <option value={ROLE_NONE}>{t("detail.roleNone")}</option>
    </select>
  );
}
