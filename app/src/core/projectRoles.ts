// The roles a task of a project can be worked under, read for the task editor's role field.
//
// The field is drawn only when the project has a role, so an empty answer is what keeps it off — which
// is also what an unanswered read and a failed one come back as: nothing is drawn rather than a field
// with no choice in it.
//
// It is re-read when the "automations" scope moves — a role is defined on the build screen while a task
// may be open beside it, and the roles are read off the project's automations, which all fold to that
// scope (`core/changes`). A write anywhere else in the store leaves it alone.
import { useQuery } from "./query";
import { fetchProjectRoles } from "./mutations";
import type { TaskRoleDto } from "../bindings/bindings";

const NONE: TaskRoleDto[] = [];

/** `projectId` null is a task in no project, which has no automation to take a role from. */
export function useProjectRoles(projectId: number | null): TaskRoleDto[] {
  const { data } = useQuery(["projectRoles", projectId], () =>
    projectId === null ? Promise.resolve(NONE) : fetchProjectRoles(projectId),
  );
  return data ?? NONE;
}

/**
 * The roles a task's role field offers: an archived automation's roles are left out, as nobody files a
 * task with one any more — except the one the task already has (`roleId`), which stays so the field
 * still shows it checked. A new task has none, so it gets no archived role at all.
 */
export function taskRoleChoices(roles: readonly TaskRoleDto[], roleId: number | null): TaskRoleDto[] {
  return roles.filter((r) => !r.automationArchived || r.id === roleId);
}
