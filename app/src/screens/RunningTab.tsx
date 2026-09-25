// The "running" tab — what is under way right now, and every failure nobody has seen yet, on one line
// each (`AMB-D-955`).
//
// **Opened from the sidebar it crosses projects, and says which project each run is in.** A run holds
// a terminal on this machine, and this machine is not divided up per project. **Opened from a project
// it is that project's runs alone** (`AMB-D-954`), and then naming the project says nothing, so it is
// left off: a reader inside one project who is shown another's runs cannot read at a glance what is
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
// **A row is read by marks, not sentences**. The state is a dot in its colour and, where
// the dot alone would not say it, a short chip beside the name; the step is the picture's own number
// for its box and the step's name, the way the box itself reads (`./AutomationPicture`); a run waiting
// for a task says so in the task's place, and one that simply has not taken one yet says nothing. A
// failure nobody has acknowledged paints its whole row.
//
// **How far in it is names the action as well as the step, where the two differ** (`AMB-D-949`): a
// launch opens one spot of the picture into a column of steps, so a step's name alone no longer says
// which spot of the automation this is. Where the spot has been taken off the picture since, it has no
// box to be numbered by, and the line says the step alone.
//
// **A step that could not leave its report on the task is marked on the row** (`AMB-D-963`): a step
// built to carry its report onto the task leaves none on a closed one, and without the mark nothing
// on screen would say why the task holds no report.
import { useState, type ReactNode } from "react";
import { acknowledgeRun, pauseRun, resumeRun, stopRun, useLiveRuns } from "../core/automations";
import { errText, t, tf } from "../core/i18n";
import { builtinWord } from "../core/builtinWords";
import { runReasonWord, runStatusWord } from "../core/runWords";
import { exactLabel, listLabel, whenLabel } from "../core/i18n/format";
import { ErrorNote } from "../components/ErrorNote";
import { Icon } from "../components/Icon";
import { useBoxNumber } from "./boxNumber";
import type { AutomationRunCardDto } from "../bindings/bindings";

/**
 * The chip beside the name — the state, where the dot alone would not say it. A run under way is
 * the plain case and carries none; a failure still waiting to be acknowledged has its whole row
 * painted instead. Once it is over, in the history, how it ended is what the row is read for.
 */
function stateChip(run: AutomationRunCardDto, ended: boolean): { icon: string; word: string } | null {
  if (run.status === "running" && run.pauseRequested) return { icon: "⏸", word: runStatusWord(run) };
  if (run.status === "paused") return { icon: "⏸", word: runStatusWord(run) };
  if (ended) return { icon: "", word: runStatusWord(run) };
  return null;
}

/**
 * That a step owed the task its report and left none, the task being closed by then (`AMB-D-963`) —
 * as a mark after the task rather than a sentence, with the steps it was named in its title. It
 * does not shrink, so a long title cut short does not take it with it.
 */
function WithheldMark({ steps }: { steps: readonly string[] }) {
  const said = tf("auto.run.reportWithheld", { steps: listLabel([...steps]) });
  return (
    <span className="autorun__withheld" title={said}>
      <Icon name="comment" label={said} />
    </span>
  );
}

/**
 * **One run on one line** — shared by the "running" tab and the "history" tab, so a run reads the same
 * on either side of ending.
 *
 * The first line is the automation and its run number, with the state's chip beside them where it
 * has one; the line under it is the project, the step, and the task it is on — or why it failed. How
 * long since stands at the end: the time it ended once it has, the time it began while it has not.
 * `acts` are the buttons that move the run; the history passes none, and says `ended` instead. The
 * project is left off a list that is one project's already.
 */
export function RunLine({
  run,
  onGo,
  acts,
  ended = false,
  withProject = true,
}: {
  run: AutomationRunCardDto;
  /** Whether the line names the project — not on a list narrowed to one. */
  withProject?: boolean;
  /** Go to the pane the run is drawn in. Absent where there is no pane to go to, and then the line is read rather than pressed. */
  onGo?: () => void;
  acts?: ReactNode;
  /** A row of the history — every run on it is over, and how it ended goes on the chip. */
  ended?: boolean;
}) {
  const no = useBoxNumber(run.automation, run.placement);
  const reason = run.status === "failed" ? runReasonWord(run) : null;
  const at = run.endedAt ?? run.startedAt;
  const chip = stateChip(run, ended);
  const pausing = run.status === "running" && run.pauseRequested;
  // The step as its box reads, and the action it was opened from only where that says something
  // more — a spot whose action is one step of the same name would say the one name twice.
  const step = run.stepName === undefined
    ? undefined
    : run.actionName === undefined || run.actionName === run.stepName
      ? builtinWord(run.builtin, run.stepName)
      : tf("auto.run.inAction", {
          action: builtinWord(run.builtin, run.actionName),
          step: builtinWord(run.builtin, run.stepName),
        });
  return (
    <li className={`autorun autorun--${run.status}${pausing ? " autorun--pausing" : ""}`}>
      <button type="button" className="autorun__go" disabled={!onGo} onClick={onGo}>
        {/* The state's colour, and its word for whoever cannot see the colour. */}
        <span className="autorun__dot" role="img" aria-label={runStatusWord(run)} title={runStatusWord(run)} />
        <span className="autorun__body">
          <span className="autorun__head">
            <span className="autorun__name">{run.automationName}</span>
            <span className="autoid">{tf("face.runNo", { n: run.run })}</span>
            {chip !== null && (
              <span className="autorun__chip">
                {chip.icon !== "" && <span aria-hidden="true">{chip.icon}</span>}
                {chip.word}
              </span>
            )}
          </span>
          <span className="autorun__sub">
            {withProject && <span className="autorun__project">{run.projectName}</span>}
            {step !== undefined && (
              <span className="autorun__step">
                {no !== undefined && <span className="autopic__no">{no}</span>}
                <span className="autorun__stepname">{step}</span>
              </span>
            )}
            {/* Waiting comes first: the task a run waiting to take its next one still holds is the
                last one, closed, and naming it would say the run is on it. A run that simply has
                not taken one yet says nothing here. */}
            {run.waiting ? (
              <span className="autorun__task autorun__task--wait">
                <span aria-hidden="true">⏳</span>
                {t("auto.run.taskWait")}
              </span>
            ) : (
              run.task !== undefined && <span className="autorun__task">{`${run.task.ref} ${run.task.title}`}</span>
            )}
            {run.reportWithheld.length > 0 && <WithheldMark steps={run.reportWithheld} />}
            {reason !== null && <span className="autorun__why">{reason}</span>}
          </span>
        </span>
        <span className="autorun__when" title={at === undefined ? undefined : exactLabel(at)}>
          {at === undefined ? "" : whenLabel(at)}
        </span>
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
      {/* Nothing under way is the automations' mark and a dash — the sentence stays as its name,
          for whoever cannot see the mark. */}
      {runs.length === 0 && (
        <div className="autoruns__none" role="status" aria-label={t("auto.running.empty")}>
          <Icon name="rocket" size="lg" />
          <span aria-hidden="true">—</span>
        </div>
      )}
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
