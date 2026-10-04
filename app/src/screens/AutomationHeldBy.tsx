// **The runs going on a definition**, drawn over a build screen while there is one.
//
// A definition is written while a run of it is running or paused, and the run goes on from the
// snapshot it took at its start (`AMB-D-1015`). What is written here reaches it only at the next
// start, or at a resume from a pause at a task's end — so the runs are named where the writing is
// done (`./AutomationBuildScreen`, `./AutomationActionBuildScreen`).
//
// Each run is one band: which run — of which automation, over an action — and on which step, the way
// to the pane it is drawn in, and the moves its state has behind one mark (`../shell/RunActs`).
import { useState } from "react";
import { builtinWord } from "../core/builtinWords";
import { errText, t, tf } from "../core/i18n";
import { ErrorNote } from "../components/ErrorNote";
import { RunActs } from "../shell/RunActs";
import type { AutomationRunCardDto } from "../bindings/bindings";

export function AutomationHeldBy({
  runs,
  withAutomation = false,
  onGoToRun,
}: {
  runs: readonly AutomationRunCardDto[];
  /** Whether each band names the automation the run is of — on an action's build screen, where the
   *  runs going on it can be of any automation it is placed on. */
  withAutomation?: boolean;
  /** Go to the pane a run is drawn in. Absent where there is no workspace face to send anybody to. */
  onGoToRun?: (project: number, run: number) => void;
}) {
  const [error, setError] = useState<string | null>(null);
  // One press at a time: a cancel moves the list this band is drawn from.
  const [pressing, setPressing] = useState(false);

  if (runs.length === 0) return null;

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

  return (
    <div className="autoheld">
      {error !== null && <ErrorNote tone="quiet">{error}</ErrorNote>}
      <ul className="autoheld__runs">
        {runs.map((run) => (
          <li key={run.run} className="autoheld__run">
            <span className="autoheld__what">
              {tf("auto.held.by", { run: run.run })}
              {withAutomation && <span className="autoheld__of">{run.automationName}</span>}
              {run.stepName !== undefined && (
                <span className="autoheld__step">{builtinWord(run.builtin, run.stepName)}</span>
              )}
            </span>
            {onGoToRun && (
              <button type="button" className="btn" onClick={() => onGoToRun(run.project, run.run)}>
                {t("auto.held.openPane")}
              </button>
            )}
            <RunActs run={run} busy={pressing} onPress={(move) => void press(move)} />
          </li>
        ))}
      </ul>
    </div>
  );
}
