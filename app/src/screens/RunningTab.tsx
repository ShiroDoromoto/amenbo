// The "running" tab — what is under way right now, on one line each (`AMB-T-5259`).
//
// **It crosses projects, and says which project each run is in.** A lane is a terminal on this
// machine and the attention of whoever is watching it, and neither is divided up per project. The
// band over the panes draws the count and nothing more; this is where a reader comes to see what the
// count is made of (`../core/automations`, `../shell/WorkspaceFace`).
//
// **A run that is `done` is not here.** What a finished run did is reached from the task it worked or
// the automation it came from, never listed — a tab that grew every run ever launched would stop
// answering the one question it is opened for. A run that **stopped** does stay, in the stop colour:
// a failure nobody was watching is the thing most worth seeing here, and it is gone from the screen
// the moment it is the reader's turn to be told about it.
//
// **The row goes to the pane, the buttons move the run.** Pressing the row is "show me this", so it
// stands the run's place in the workspace and goes to it — including for a run still waiting for a
// lane, whose place is stood empty and is the one its first step opens in (`../talk/layout`).
import { useState } from "react";
import { useLiveRuns } from "../core/automations";
import { pauseRun, resumeRun, stopRun } from "../core/mutations";
import { errText, t, tf } from "../core/i18n";
import { taskRef } from "../core/idref";
import { ErrorNote } from "../components/ErrorNote";
import type { AutomationRunCardDto } from "../bindings/bindings";

/**
 * What the run is doing, in one word.
 *
 * A pause that has been asked for and not settled is its own line rather than either of the two it
 * sits between: the run is still `running` and a reader told only that would press pause again, and
 * told "paused" would believe the step had already stopped (`amenbo_core::ops::automation_stop`).
 */
function statusText(run: AutomationRunCardDto): string {
  if (run.status === "running" && run.pauseRequested) return t("auto.run.pausing");
  switch (run.status) {
    case "queued": return t("auto.run.queued");
    case "running": return t("auto.run.running");
    case "paused": return t("auto.run.paused");
    case "stopped": return t("auto.run.stopped");
    default: return run.status;
  }
}

/** Why it stopped, where core named one. A stop with no reason on it says nothing rather than guessing. */
function reasonText(run: AutomationRunCardDto): string | null {
  switch (run.stoppedReason) {
    case "crashed": return t("auto.run.crashed");
    case "max_times": return t("auto.run.maxTimes");
    case "no_agent": return t("auto.run.noAgent");
    case "by_human": return t("auto.run.byHuman");
    default: return null;
  }
}

export function RunningTab({
  onGoToRun,
}: {
  /** Go to the pane this run is drawn in. Absent in the window that has no workspace face to send
   *  anybody to, and then the rows are read rather than pressed. */
  onGoToRun?: (project: number, run: number) => void;
}) {
  const runs = useLiveRuns();
  const [error, setError] = useState<string | null>(null);

  // One press at a time, whichever row it was on: the three writes all move the same queue, and a
  // second press landing while the first is still opening a terminal would be answered off a picture
  // that has already changed.
  const [pressing, setPressing] = useState(false);
  const press = async (move: () => Promise<void>) => {
    setError(null);
    setPressing(true);
    try {
      await move();
    } catch (err) {
      setError(errText(err));
    } finally {
      setPressing(false);
    }
  };

  return (
    <>
      {/* Above the rows, and above the empty line too: a press that was refused is the answer to
          what the reader just did, and the list emptying under it is not a reason to take it back. */}
      {error && <ErrorNote tone="quiet">{error}</ErrorNote>}
      {runs.length === 0 && <div className="auto__empty">{t("auto.running.empty")}</div>}
      {runs.length > 0 && (
      <ul className="auto__list">
        {runs.map((run) => {
          const reason = reasonText(run);
          const over = run.status === "stopped";
          return (
            <li key={run.run} className="autorun">
              <button
                type="button"
                className="auto__row autorun__go"
                disabled={!onGoToRun}
                onClick={() => onGoToRun?.(run.project, run.run)}
              >
                <span className="autorun__what">
                  <span className="autorun__of">{run.automationName}</span>
                  {run.stepName !== undefined && (
                    <span className="autorun__step">
                      {tf("auto.run.step", { n: run.stepsDone, step: run.stepName })}
                    </span>
                  )}
                  {run.taskId !== undefined && (
                    <span className="autorun__task">
                      {taskRef(run.taskId)}
                      {run.taskTitle !== undefined && ` ${run.taskTitle}`}
                    </span>
                  )}
                </span>
                <span className="auto__mark">{run.projectName}</span>
                <span className={`autorun__state autorun__state--${run.status}`}>
                  {statusText(run)}
                  {reason !== null && <span className="autorun__why">{reason}</span>}
                </span>
              </button>

              {/* A stopped run has nothing left to move, so it carries no buttons at all rather than
                  three that refuse. */}
              {!over && (
                <span className="autorun__acts">
                  {run.status === "paused" ? (
                    <button
                      type="button"
                      className="btn"
                      disabled={pressing}
                      onClick={() => void press(() => resumeRun(run.run))}
                    >
                      {t("auto.run.resume")}
                    </button>
                  ) : (
                    <button
                      type="button"
                      className="btn"
                      disabled={pressing || run.pauseRequested}
                      onClick={() => void press(() => pauseRun(run.run))}
                    >
                      {t("auto.run.pause")}
                    </button>
                  )}
                  <button
                    type="button"
                    className="btn"
                    disabled={pressing}
                    onClick={() => void press(() => stopRun(run.run))}
                  >
                    {t("auto.run.stop")}
                  </button>
                </span>
              )}
            </li>
          );
        })}
      </ul>
      )}
    </>
  );
}
