// The automations screen's own seam: a project's definitions, one definition whole, whether that one
// could be started, and the library its steps are pointed at.
//
// It sits beside `core/reads.ts` rather than in it because what it reads is a different shape of
// thing: a task list is paged and an automation is not. An automation is tens of rows, and the build
// screen's picture, its step panel and its launch check all walk the same definition — so it is
// fetched whole, once, and every part of the screen reads that one answer.
//
// **The one write here sits beside its reads** rather than in `core/mutations`, because what it is
// about is this screen and nothing else. What it does not do for itself is the invalidation — the
// ack goes through `mutations.invokeAck`, the same road every other write takes.
//
// **The launch check is read, not worked out here.** The rules live in core, where the launch itself
// reads them (`amenbo_core::ops::automation_run::check`), so what a screen says is in the way and
// what a press refuses cannot come to disagree. What this side supplies is the one fact the store
// cannot answer: which agents this machine can actually start.
import { useQuery } from "./query";
import { inTauri } from "./snapshot";
import { invoke } from "./ipc";
import { invokeAck } from "./mutations";
import type {
  AutomationActionCardDto,
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

/**
 * The library this project reaches — the device's own actions and the project's own, in one list.
 *
 * Both reaches come in one answer because both are one list on screen: what a reader is choosing
 * between is every prompt a step here could be pointed at, and which library holds one is a column.
 */
export async function fetchAutomationActions(projectId: number): Promise<AutomationActionCardDto[]> {
  if (!inTauri()) return [];
  return invoke<AutomationActionCardDto[]>("automation_action_page", { projectId });
}

/** Subscribing read of the library this project reaches. */
export function useAutomationActions(projectId: number | null): AutomationActionCardDto[] {
  const { data } = useQuery<AutomationActionCardDto[]>(
    ["automationActions", projectId ?? null],
    () => (projectId === null ? Promise.resolve([]) : fetchAutomationActions(projectId)),
  );
  return data ?? [];
}

/**
 * Rename a library action, or rewrite its prompt. Only what is passed is written.
 *
 * **The rewrite reaches every step pointing at this action**, which is what a library is for — and
 * why the screen says how many automations that is before the box is opened. A run already under way
 * is not reached: a step's prompt is resolved as the step opens, and what an open step carries is
 * settled.
 */
export async function editAutomationAction(
  id: number,
  patch: { name?: string; prompt?: string },
): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("automation_action_edit", {
    id,
    name: patch.name ?? null,
    prompt: patch.prompt ?? null,
  });
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
 * (`crate::wake::wake_choices`). **An answer that never came goes on as `null`, not as an empty
 * list** — no step is then judged on its agent at all, because telling a reader to install what they
 * already have is worse than saying nothing about it (`AMB-D-792`).
 */
export async function fetchLaunchCheck(
  id: number,
  projectId: number,
  folders: readonly string[],
): Promise<AutomationLaunchCheckDto | null> {
  if (!inTauri()) return null;
  const agents = await invoke<WakeDto>("wake_choices", { project: projectId, folders: [...folders] })
    .then((wake) => wake.candidates.filter((one) => one.installed).map((one) => one.id))
    .catch(() => null);
  return invoke<AutomationLaunchCheckDto>("automation_launch_check", { id, agents });
}

/** Subscribing read of the launch check for one automation. */
export function useLaunchCheck(
  id: number | null,
  projectId: number | null,
  folders: readonly string[],
): AutomationLaunchCheckDto | null {
  const { data } = useQuery<AutomationLaunchCheckDto | null>(
    ["automationLaunchCheck", id ?? null, projectId ?? null, [...folders].join("\n")],
    () =>
      id === null || projectId === null
        ? Promise.resolve(null)
        : fetchLaunchCheck(id, projectId, folders),
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
