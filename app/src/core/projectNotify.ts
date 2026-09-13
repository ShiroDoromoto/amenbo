// What one project does with the device's shelf — the read and write seam for
// `screens/ProjectNotifySection` (`AMB-D-885`).
//
// The shelf itself is `./notifyTargets`, and the two are deliberately apart: a connection belongs to the
// machine, and what a project answers is **which of them carry it** — plus whether it notifies at all,
// what it reports, and where its mail is addressed. So nothing here writes a connection, and nothing on
// the shelf knows a project.
//
// Every write is one field, sent as it is pressed. There is no save button on this face: a checkbox, a
// chip and a switch each say what they mean the moment they move, and a form that collected them would
// have to invent a half-written state for something the store already holds one field at a time.
import { invoke } from "./ipc";
import { inTauri } from "./snapshot";
import { invalidateQueries, useQuery } from "./query";
import type { ProjectNotifyDto } from "../bindings/bindings";

/** One project's notification row, with the catalog the checkboxes are drawn from (generated DTO). */
export type ProjectNotify = ProjectNotifyDto;

/** What a project that has never been set up answers with — on, carrying nothing, reporting nothing. */
const UNSET: ProjectNotify = {
  enabled: true,
  mailTo: "",
  targetIds: [],
  events: [],
  reportable: [],
};

/** This project's row. Outside Tauri there is no store to ask, so the unset answer stands. */
export async function fetchProjectNotify(projectId: number): Promise<ProjectNotify> {
  if (!inTauri()) return UNSET;
  return invoke<ProjectNotify>("project_notify", { projectId });
}

/** This project's row, for the settings section that draws it. */
export function useProjectNotify(projectId: number): { notify: ProjectNotify; loading: boolean } {
  const { data, loading } = useQuery<ProjectNotify>(["project-notify", projectId], () =>
    fetchProjectNotify(projectId),
  );
  return { notify: data ?? UNSET, loading };
}

/** Refetch one project's row — after any of the writes below. */
function reload(): void {
  invalidateQueries((key) => key[0] === "project-notify");
}

/**
 * Turn this project's notifications on or off. Off keeps the targets and the events where they are: the
 * switch is there to *stop* notifications, not to undo the setting up.
 */
export async function setProjectNotifyEnabled(projectId: number, enabled: boolean): Promise<void> {
  if (!inTauri()) return;
  await invoke("project_notify_set_enabled", { projectId, enabled });
  reload();
}

/**
 * Write where this project's mail is addressed — several addresses on one line, separated by commas, as
 * the person typed them. Empty falls back to the mail target's own account.
 */
export async function setProjectMailTo(projectId: number, mailTo: string): Promise<void> {
  if (!inTauri()) return;
  await invoke("project_notify_set_mail_to", { projectId, mailTo });
  reload();
}

/**
 * Carry this project's notifications through one target, or stop carrying them through it. The target
 * stays on the shelf either way — it is the device's, and the other projects that chose it keep it.
 */
export async function selectProjectTarget(
  projectId: number,
  targetId: number,
  selected: boolean,
): Promise<void> {
  if (!inTauri()) return;
  await invoke("project_notify_select_target", { projectId, targetId, selected });
  reload();
}

/** Tick or untick one of the events this project reports. Unticking them all is an answer, and it is kept. */
export async function setProjectNotifyEvent(
  projectId: number,
  event: string,
  on: boolean,
): Promise<void> {
  if (!inTauri()) return;
  await invoke("project_notify_set_event", { projectId, event, on });
  reload();
}
