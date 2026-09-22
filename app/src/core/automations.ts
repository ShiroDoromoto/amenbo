// The automations screen's own seam: a project's definitions, one definition whole, whether that one
// could be started, the library its steps are pointed at — and the one write that is not the screen's
// at all, a run being stopped from the pane it is drawn in.
//
// It sits beside `core/reads.ts` rather than in it because what it reads is a different shape of
// thing: a task list is paged and an automation is not. An automation is tens of rows, and the build
// screen's picture, its step panel and its launch check all walk the same definition — so it is
// fetched whole, once, and every part of the screen reads that one answer.
//
// **The writes here sit beside their reads** rather than in `core/mutations`, because what they are
// about is this screen and nothing else. What the screen's own write does not do for itself is the
// invalidation — the ack goes through `mutations.invokeAck`, the same road every other write takes.
// The second one is pressed from a pane rather than from this screen, and is no ack at all
// (`stopRun`).
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
  AutomationRunCardDto,
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

/**
 * **What is under way right now**, across every project — the rows of the "running" tab.
 *
 * It crosses projects because the lanes do, and this is where a reader sees what the count on the
 * band over the panes is made of (`fetchLanesHeld`). Runs that are `done` are not in it: what a
 * finished run did is reached from the task it worked, never listed here.
 */
export async function fetchLiveRuns(): Promise<AutomationRunCardDto[]> {
  if (!inTauri()) return [];
  return invoke<AutomationRunCardDto[]>("automation_running_page", {});
}

/** Subscribing read of the runs under way. Empty until the first answer lands. */
export function useLiveRuns(): AutomationRunCardDto[] {
  const { data } = useQuery<AutomationRunCardDto[]>(["automationRuns"], fetchLiveRuns);
  return data ?? [];
}

/**
 * **Stop a run now** — what closing the pane a run is drawn in means (`../shell/TerminalPane`).
 *
 * The cleanup is core's and is the same one every other stop goes through: the lane is handed back,
 * the task the run reserved goes to `todo`, and a line on that task says the run is not coming back
 * (`amenbo_core::ops::automation_stop`).
 *
 * **It is not a `WriteAck` write.** What it moves is a run, a task and a comment, and every screen
 * that draws one of those is already following the change feed — which is how a lane taken by a run
 * in another project reaches this window in the first place (`fetchLanesHeld`).
 *
 * Answers whether this press was the one that stopped it: a run that had already finished is `false`
 * and not a refusal, the press having been about the pane.
 */
export async function stopRun(run: number): Promise<boolean> {
  if (!inTauri()) return false;
  return invoke<boolean>("automation_run_stop", { runId: run });
}

/**
 * **Ask a run to pause** — pressed on a row of the "running" tab (`../screens/RunningTab`).
 *
 * A step under way cannot be cut in half, so the run goes on until that step reports and settles
 * there, handing its lane back; a run with nothing under way pauses on the spot
 * (`amenbo_core::ops::automation_stop::pause`). Not a `WriteAck` write, for `stopRun`'s reason.
 */
export async function pauseRun(run: number): Promise<void> {
  if (!inTauri()) return;
  return invoke<void>("automation_run_pause", { runId: run });
}

/**
 * **Pick a paused run up again**, from the way out its last step left through. It opens a terminal
 * where a lane is free and joins the queue where none is.
 *
 * Refused for a run that is not paused, which is the answer a reader gets rather than nothing
 * happening: the row they pressed was drawn from a picture that has since moved.
 */
export async function resumeRun(run: number): Promise<void> {
  if (!inTauri()) return;
  return invoke<void>("automation_run_resume", { runId: run });
}
