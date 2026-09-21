// The read side of the automations screen: a project's definitions, one definition whole, and
// whether that one could be started.
//
// It sits beside `core/reads.ts` rather than in it because what it reads is a different shape of
// thing: a task list is paged and an automation is not. An automation is tens of rows, and the build
// screen's picture, its step panel and its launch check all walk the same definition — so it is
// fetched whole, once, and every part of the screen reads that one answer.
//
// **The launch check is read, not worked out here.** The rules about what makes a definition
// unfinished live in one place (`crate::automation`), so the words this screen puts on the screen and
// the refusal a launch would give cannot come to disagree. What this side supplies is the two facts
// the host cannot know on its own: what this machine can start, and whether there is a workspace to
// open the run's panes in.
import { useQuery } from "./query";
import { inTauri } from "./snapshot";
import { invoke } from "./ipc";
import type {
  AutomationCardDto,
  AutomationDetailDto,
  AutomationLaunchCheckDto,
  WakeDto,
} from "../bindings/bindings";

/** A project's automations, in the order they were placed in. */
export async function fetchAutomations(projectId: number): Promise<AutomationCardDto[]> {
  if (!inTauri()) return [];
  return invoke<AutomationCardDto[]>("automation_page", { projectId });
}

/** Subscribing read of a project's automations. */
export function useAutomations(projectId: number | null): AutomationCardDto[] {
  const { data } = useQuery<AutomationCardDto[]>(
    ["automations", projectId ?? null],
    () => (projectId === null ? Promise.resolve([]) : fetchAutomations(projectId)),
  );
  return data ?? [];
}

/** One automation's whole definition, or nothing where that id names none. */
export async function fetchAutomation(id: number): Promise<AutomationDetailDto | null> {
  if (!inTauri()) return null;
  return invoke<AutomationDetailDto | null>("automation_detail", { id });
}

/** Subscribing read of one automation's definition. */
export function useAutomation(id: number | null): AutomationDetailDto | null {
  const { data } = useQuery<AutomationDetailDto | null>(
    ["automation", id ?? null],
    () => (id === null ? Promise.resolve(null) : fetchAutomation(id)),
  );
  return data ?? null;
}

/**
 * Whether this automation could be started, and what is in the way.
 *
 * `folders` are the project's bound folders: whether an agent is installed is asked of the machine
 * the way the empty frame asks it, over the folders this project actually works in
 * (`crate::wake::wake_choices`). An answer that never came is an empty list of agents, and then no
 * step is judged on its agent at all — telling a reader to install what they already have is worse
 * than saying nothing about it.
 */
export async function fetchLaunchCheck(
  id: number,
  projectId: number,
  folders: readonly string[],
  workspaceOpen: boolean,
): Promise<AutomationLaunchCheckDto | null> {
  if (!inTauri()) return null;
  const agents = await invoke<WakeDto>("wake_choices", { project: projectId, folders: [...folders] })
    .then((wake) => wake.candidates.filter((one) => one.installed).map((one) => one.id))
    .catch(() => [] as string[]);
  return invoke<AutomationLaunchCheckDto>("automation_launch_check", { id, agents, workspaceOpen });
}

/** Subscribing read of the launch check for one automation. */
export function useLaunchCheck(
  id: number | null,
  projectId: number | null,
  folders: readonly string[],
  workspaceOpen: boolean,
): AutomationLaunchCheckDto | null {
  const { data } = useQuery<AutomationLaunchCheckDto | null>(
    ["automationLaunchCheck", id ?? null, projectId ?? null, [...folders].join("\n"), workspaceOpen],
    () =>
      id === null || projectId === null
        ? Promise.resolve(null)
        : fetchLaunchCheck(id, projectId, folders, workspaceOpen),
  );
  return data ?? null;
}

/**
 * **How many lanes the automations are holding right now**, across every project.
 *
 * It is a query rather than a field of the snapshot because of when it moves: the snapshot is re-read
 * after a write made on this screen, and a lane is taken and handed back by whatever step reported,
 * from wherever it was running. So it hangs off the change feed instead (`./changes`), and the band
 * drawing it follows a run that nobody in this window started.
 */
export async function fetchLanesHeld(): Promise<number> {
  if (!inTauri()) return 0;
  return invoke<number>("automation_lanes_held", {});
}

/** Subscribing read of how many lanes are held. `0` until the first answer lands. */
export function useLanesHeld(): number {
  const { data } = useQuery<number>(["automationLanesHeld"], fetchLanesHeld);
  return data ?? 0;
}
