// The roles a task of a project can be worked under, read for the task editor's role field.
//
// The field is drawn only when the project has a role, so an empty answer is what keeps it off — which
// is also what an unanswered read and a failed one come back as: nothing is drawn rather than a field
// with no choice in it.
//
// It is re-read whenever the store moves, because a role is defined on the build screen while a task
// may be open beside it.
import { useEffect, useState } from "react";
import { fetchProjectRoles } from "./mutations";
import type { TaskRoleDto } from "../bindings/bindings";

/** `projectId` null is a task in no project, which has no automation to take a role from. */
export function useProjectRoles(projectId: number | null): TaskRoleDto[] {
  const [roles, setRoles] = useState<TaskRoleDto[]>([]);

  useEffect(() => {
    let alive = true;
    setRoles([]);
    if (projectId === null) return;
    const read = () => {
      fetchProjectRoles(projectId)
        .then((got) => { if (alive) setRoles(got); })
        .catch(() => {});
    };
    read();
    let off: (() => void) | null = null;
    void import("@tauri-apps/api/event")
      .then(({ listen }) => listen("store-changed", read))
      .then((stop) => { if (alive) off = stop; else stop(); })
      // Outside Tauri (browser iteration) nothing writes to a store and nothing announces one.
      .catch(() => {});
    return () => { alive = false; off?.(); };
  }, [projectId]);

  return roles;
}
