// The "running" tab — what is under way right now, and every failure nobody has seen yet, on one line
// each (`AMB-D-955`).
//
// **Opened from the sidebar it crosses projects, and says which project each run is in.** A run holds
// a terminal on this machine, and this machine is not divided up per project. **Opened from a project
// it is that project's runs alone** (`AMB-D-954`), and then the project column says nothing, so it is
// not drawn: a reader inside one project who is shown another's runs cannot read at a glance what is
// going in their own.
//
// **What is over and needs nobody is not here.** A completed run, a canceled one and a failure
// somebody has acknowledged are the "history" tab's (`./HistoryTab`). A failure stays here, in the
// stop colour, until a person presses "acknowledge": the task it handed back is one nobody is
// carrying, and a failure that slid into the history unseen would take that task out of sight with it.
//
// **The row goes to the pane, the buttons move the run.** Pressing the row is "show me this", so it
// stands the run's place in the workspace and goes to it.
//
// **How far in it is says the action as well as the step** (`AMB-D-949`), for the reason the row over
// a run's pane does (`../talk/nameplate`): a launch opens one spot of the picture into a column of
// steps, so a step's name alone no longer says which spot of the automation this is. Where the spot
// has been taken off the picture since, the line says the step alone.
import { useState, type ReactNode } from "react";
import { acknowledgeRun, pauseRun, resumeRun, stopRun, useLiveRuns } from "../core/automations";
import { errText, t, tf } from "../core/i18n";
import { builtinWord } from "../core/builtinWords";
import { runReasonWord, runStatusWord } from "../core/runWords";
import { exactLabel, whenLabel } from "../core/i18n/format";
import { ErrorNote } from "../components/ErrorNote";
import type { AutomationRunCardDto } from "../bindings/bindings";

/**
 * **One run on one line** — shared by the "running" tab and the "history" tab, so a run reads the same
 * on either side of ending.
 *
 * The line is state, the automation and its run number, the project, how far in it is, the task it is
 * on, and how long since — the time it ended once it has, the time it began while it has not. Under
 * it, for a failure, why. `acts` are the buttons that move the run; the history passes none. The
 * project is left off a list that is one project's already.
 */
export function RunLine({
  run,
  onGo,
  acts,
  withProject = true,
}: {
  run: AutomationRunCardDto;
  /** Whether the line names the project — not on a list narrowed to one. */
  withProject?: boolean;
  /** Go to the pane the run is drawn in. Absent where there is no pane to go to, and then the line is read rather than pressed. */
  onGo?: () => void;
  acts?: ReactNode;
}) {
  const reason = run.status === "failed" ? runReasonWord(run) : null;
  const at = run.endedAt ?? run.startedAt;
  return (
    <li className={`autorun autorun--${run.status}`}>
      <button
        type="button"
        className={withProject ? "autorun__go" : "autorun__go autorun__go--oneproject"}
        disabled={!onGo}
        onClick={onGo}
      >
        <span className="autorun__state">{runStatusWord(run)}</span>
        <span className="autorun__of">
          <span className="autorun__name">{run.automationName}</span>
          <span className="autoid">{tf("face.runNo", { n: run.run })}</span>
        </span>
        {withProject && <span className="autorun__project">{run.projectName}</span>}
        <span className="autorun__step">
          {run.stepName !== undefined &&
            tf("auto.run.step", {
              n: run.stepsDone,
              // Which step, in full — the action and the step as one value, so the language orders
              // the two and the count is counted of the pair.
              // A built-in's step and the action it stands for are drawn in the screen's language.
              step: run.actionName === undefined
                ? builtinWord(run.builtin, run.stepName)
                : tf("auto.run.inAction", {
                    action: builtinWord(run.builtin, run.actionName),
                    step: builtinWord(run.builtin, run.stepName),
                  }),
            })}
        </span>
        <span className="autorun__task">
          {/* Waiting comes first: the task a run waiting to take its next one still holds is the
              last one, closed, and naming it would say the run is on it. */}
          {run.waiting ? (
            <span className="autorun__notask">{t("auto.run.waitingForTask")}</span>
          ) : run.task !== undefined ? (
            `${run.task.ref} ${run.task.title}`
          ) : (
            // Only while it can still take one: a run that ended without a task never had one to take.
            (run.status === "running" || run.status === "paused") && (
              <span className="autorun__notask">{t("auto.run.noTask")}</span>
            )
          )}
        </span>
        <span className="autorun__when" title={at === undefined ? undefined : exactLabel(at)}>
          {at === undefined ? "" : whenLabel(at)}
        </span>
        {reason !== null && <span className="autorun__why">{reason}</span>}
      </button>
      <span className="autorun__acts">{acts}</span>
    </li>
  );
}

export function RunningTab({
  projectId,
  onGoToRun,
}: {
  /** The project whose runs these are, or `null` for every project's — the sidebar's. */
  projectId: number | null;
  /** Go to the pane this run is drawn in. Absent in the window that has no workspace face to send
   *  anybody to, and then the rows are read rather than pressed. */
  onGoToRun?: (project: number, run: number) => void;
}) {
  // Narrowed here rather than asked for: every row already says its project, and the live list is
  // what is going on this machine now, which is short.
  const everyRun = useLiveRuns();
  const runs = projectId === null ? everyRun : everyRun.filter((one) => one.project === projectId);
  const [error, setError] = useState<string | null>(null);

  // One press at a time, whichever row it was on: the writes all move the same list, and a second
  // press landing while the first is still opening a terminal would be answered off a picture that
  // has already changed.
  const [pressing, setPressing] = useState(false);
  const press = async (move: () => Promise<unknown>) => {
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

  // The buttons a row carries are the moves its state has: a run going can be held or stopped, a held
  // one picked up again or stopped, and a failure acknowledged.
  const actsOf = (run: AutomationRunCardDto) => {
    if (run.status === "failed") {
      return (
        <button type="button" className="btn" disabled={pressing} onClick={() => void press(() => acknowledgeRun(run.run))}>
          {t("auto.run.acknowledge")}
        </button>
      );
    }
    return (
      <>
        {run.status === "paused" ? (
          <button type="button" className="btn" disabled={pressing} onClick={() => void press(() => resumeRun(run.run))}>
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
        <button type="button" className="btn" disabled={pressing} onClick={() => void press(() => stopRun(run.run))}>
          {t("auto.run.stop")}
        </button>
      </>
    );
  };

  return (
    <>
      {/* Above the rows, and above the empty line too: a press that was refused is the answer to
          what the reader just did, and the list emptying under it is not a reason to take it back. */}
      {error && <ErrorNote tone="quiet">{error}</ErrorNote>}
      {runs.length === 0 && <div className="auto__empty">{t("auto.running.empty")}</div>}
      {runs.length > 0 && (
        <ul className="autoruns">
          {runs.map((run) => (
            <RunLine
              key={run.run}
              run={run}
              onGo={onGoToRun && (() => onGoToRun(run.project, run.run))}
              acts={actsOf(run)}
              withProject={projectId === null}
            />
          ))}
        </ul>
      )}
    </>
  );
}
