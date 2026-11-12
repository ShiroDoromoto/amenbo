// The role a task is worked under, shown on a list row and a board card: the role's picture and its
// name, never the automation's. A role whose automation is archived is shown all the same — taking it
// off the choices (`taskRoleChoices`) is only about filing a task anew. A long name ends in an ellipsis
// and the whole of it is in the tooltip.
//
// The roles are read per project (`useProjectRoles`), as a list may hold tasks of several projects; the
// rows of one project share the one read. A task with no role, and one whose role is not in the
// project's answer (not read yet, or the read failed), draws nothing.
import { useProjectRoles } from "../core/projectRoles";
import { RoleIcon } from "../screens/AutomationRoles";

export function TaskRoleChip({ projectId, roleId }: { projectId: number | null; roleId: number | null }) {
  if (roleId === null) return null;
  return <KnownRole projectId={projectId} roleId={roleId} />;
}

// Split off so a task with no role subscribes to no read at all.
function KnownRole({ projectId, roleId }: { projectId: number | null; roleId: number }) {
  const role = useProjectRoles(projectId).find((r) => r.id === roleId);
  if (!role) return null;
  return (
    <span className="taskrole" title={role.name}>
      <RoleIcon role={role} size={14} />
      <span className="taskrole__name">{role.name}</span>
    </span>
  );
}
