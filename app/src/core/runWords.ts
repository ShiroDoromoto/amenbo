// **A run's state, in the screen's words** — one set of them for every place a run is read
// (`AMB-D-955`).
//
// The "running" and "history" tabs say it on a run's line (`../screens/RunningTab`), and the row over
// a run's pane says it over the terminal (`../talk/nameplate`). They are the same words because they
// are the same run: a reader who sees "failed" on the pane and goes to the tab to find out more has to
// be able to find the line that says the same.
import type { AutomationRunCardDto } from "../bindings/bindings";
import { t, tf } from "./i18n";
import { builtinWord } from "./builtinWords";
import type { RunState } from "../talk/nameplate";

/** The name core gives the error way out — `../screens/automationLayout`'s `ERROR_EXIT`. */
const ERROR_EXIT = "*";

/**
 * What the run is doing, in one word.
 *
 * A pause that has been asked for and not settled is its own line rather than either of the two it
 * sits between: the run is still `running` and a reader told only that would press pause again, and
 * told "paused" would believe the step had already stopped (`amenbo_core::ops::automation_stop`).
 */
export function runStatusWord(run: Pick<AutomationRunCardDto, "status" | "pauseRequested">): string {
  if (run.status === "running" && run.pauseRequested) return t("auto.run.pausing");
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
 * **A way out a run left through**, as the screen names it — the name the run's copy holds, which is
 * empty for the unnamed one. A built-in's is drawn in the screen's language, so `builtin` is the key
 * of the step it left.
 */
export function runExitWord(builtin: string | null | undefined, name: string): string {
  if (name === "") return t("auto.step.exitUnnamed");
  if (name === ERROR_EXIT) return t("auto.pic.errorExit");
  return builtinWord(builtin, name);
}

/**
 * **Where a run stands, as the row over its pane says it** (`../talk/nameplate`, `AMB-T-5506`) — or
 * null until the run has been read, and then the row says nothing of its state rather than guessing.
 *
 * A failure says where as well as why: the step it failed in, and the way out that step left by where
 * it left by one. A program that exited before it reported left by none, and then the step is all
 * there is to say.
 */
export function runStateOf(run: AutomationRunCardDto | undefined): RunState | null {
  if (run === undefined) return null;
  const failed = run.status === "failed";
  return {
    status: run.status,
    word: runStatusWord(run),
    why: failed ? runReasonWord(run) : null,
    where: failed ? runWhere(run) : null,
  };
}

/** The step a run is on or ended on, and the way out it left by — or null before any step opened. */
function runWhere(run: AutomationRunCardDto): string | null {
  if (run.stepName === undefined) return null;
  const step = builtinWord(run.builtin, run.stepName);
  const which = run.actionName === undefined
    ? step
    : tf("auto.run.inAction", { action: builtinWord(run.builtin, run.actionName), step });
  return run.exitName === undefined
    ? which
    : tf("face.runFailedAt", { step: which, exit: runExitWord(run.builtin, run.exitName) });
}
