// **A run's state, in the screen's words** — one set of them for every place a run is read
// (`AMB-D-955`).
//
// The "running" and "history" tabs say it on a run's line (`../screens/RunningTab`), and the row over
// a run's pane says it over the terminal (`../talk/nameplate`). They are the same words because they
// are the same run: a reader who sees "failed" on the pane and goes to the tab to find out more has to
// be able to find the line that says the same.
import type { AutomationRunCardDto } from "../bindings/bindings";
import { t } from "./i18n";
import { builtinWord } from "./builtinWords";
import type { RunState, Say } from "../talk/nameplate";

/** The name core gives the error way out — `../screens/automationLayout`'s `ERROR_EXIT`. */
const ERROR_EXIT = "*";

/**
 * **Whether a pause has been asked for and not settled** — at the end of the action under way
 * (`AMB-D-1002`), or before the run takes its next task (`AMB-D-1009`). The run is still `running`.
 */
export function isPausing(
  run: Pick<AutomationRunCardDto, "status" | "pauseRequested" | "pauseBeforeNextTask">,
): boolean {
  return run.status === "running" && (run.pauseRequested || run.pauseBeforeNextTask);
}

/**
 * What the run is doing, in one word.
 *
 * A pause that has been asked for and not settled is its own line rather than either of the two it
 * sits between: the run is still `running` and a reader told only that would press pause again, and
 * told "paused" would believe the action under way — or, asked before its next task, the task — had
 * already ended (`AMB-D-1002`, `AMB-D-1009`, `amenbo_core::ops::automation_stop`).
 */
export function runStatusWord(
  run: Pick<AutomationRunCardDto, "status" | "pauseRequested" | "pauseBeforeNextTask">,
): string {
  if (isPausing(run)) return t("auto.run.pausing");
  switch (run.status) {
    case "running": return t("auto.run.running");
    case "paused": return t("auto.run.paused");
    case "completed": return t("auto.run.completed");
    case "failed": return t("auto.run.failed");
    case "canceled": return t("auto.run.canceled");
  }
}

/** Why it failed, where core named one. A failure with no reason on it says nothing rather than guessing. */
export function runReasonWord(run: Pick<AutomationRunCardDto, "stoppedReason">): string | null {
  switch (run.stoppedReason) {
    case "crashed": return t("auto.run.crashed");
    case "max_times": return t("auto.run.maxTimes");
    case "no_agent": return t("auto.run.noAgent");
    case "no_input": return t("auto.run.noInput");
    case "no_way_on": return t("auto.run.noWayOn");
    case "halted": return t("auto.run.halted");
    case "left_task_open": return t("auto.run.leftTaskOpen");
    default: return null;
  }
}

/**
 * **A way out a run left through**, as the screen names it — the name the run's copy holds. A
 * built-in's is drawn in the screen's language, so `builtin` is the key of the step it left.
 */
export function runExitWord(builtin: string | null | undefined, name: string): string {
  if (name === ERROR_EXIT) return t("auto.pic.errorExit");
  return builtinWord(builtin, name);
}

/**
 * **Where a run stands, as its pane says it** (`../talk/nameplate`, `AMB-T-5506`) — or null until the
 * run has been read, and then the pane says nothing of its state rather than guessing.
 *
 * A failure says one thing under the row, and never where: the row above it already names the step
 * (`AMB-T-5529`). A step that stopped at a way out calling for a person is said by that way out, the
 * mark the picture draws it with; every other failure is said by its reason.
 */
export function runStateOf(run: AutomationRunCardDto | undefined): RunState | null {
  if (run === undefined) return null;
  const failed = run.status === "failed";
  const byExit = failed && run.stoppedReason === "halted" && run.exitName !== undefined;
  return {
    status: run.status,
    word: runStatusWord(run),
    pauseRequested: run.pauseRequested,
    why: failed && !byExit ? runReasonWord(run) : null,
    exit: byExit ? runExitWord(run.builtin, run.exitName!) : null,
    errorExit: byExit && run.exitName === ERROR_EXIT,
    acknowledged: run.acknowledged,
    ...(run.acknowledgedBy != null ? { acknowledgedBy: run.acknowledgedBy } : {}),
  };
}

/**
 * **A run whose built-in is waiting for a task**, as its pane says it: still running, in the word the
 * running tab says a waiting run with (`auto.run.taskWait`). A run held, over, or asked to pause says
 * that instead.
 */
export function waitingState(run: AutomationRunCardDto | undefined): RunState | null {
  const state = runStateOf(run);
  if (run === undefined || state === null || state.status !== "running" || isPausing(run)) return state;
  return { ...state, word: t("auto.run.taskWait") };
}

/**
 * **The row over a run's pane, said off the run alone** — for a pane no step has arrived in
 * (`AMB-T-5635`). A pane kept by the store comes back when the app does, and a run that is held or
 * failed opens no terminal to be told by: what the row says is the step the run is on, or the last one
 * it ran, and where it stands.
 *
 * It is the run's line of the "running" tab, over the pane. Null until the run has been read, and then
 * the pane draws no row of the run's rather than one with nothing in it.
 */
export function runSayOf(run: AutomationRunCardDto | undefined): Say | null {
  if (run === undefined) return null;
  return {
    automation: run.automationName,
    run: run.run,
    step: builtinWord(run.builtin, run.stepName ?? ""),
    automationId: run.automation,
    // The spot the step was opened from, which is also the box "see the picture" opens: where a run
    // failed, it is where it failed — a crash included, which is the step that was under way.
    placement: run.placement ?? null,
    box: null,
    builtin: run.builtin !== undefined,
    // A pane no step has arrived in has no step running to wait for a person.
    interactive: false,
    action: run.actionName === undefined ? null : builtinWord(run.builtin, run.actionName),
    // A run waiting for the next task still carries the one it closed before (`../shell/WorkspaceFace`).
    task: run.waiting ? null : run.task ?? null,
    state: run.waiting ? waitingState(run) : runStateOf(run),
  };
}
