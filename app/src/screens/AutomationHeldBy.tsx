// **The runs holding a definition**, drawn over a build screen while there is one (`AMB-D-961`).
//
// Core refuses every rewrite of an automation, or of an action placed on one, while a run of it is
// running or paused. A screen that went on offering its fields would let a reader press and then be
// refused, so both build screens hold their fields shut for as long as this list is not empty
// (`./AutomationBuildScreen`, `./AutomationActionBuildScreen`).
//
// **Held shut is a mark, and the way out is a button.** Each run is one band: the lock, which run —
// of which automation, over an action — and on which step, and the moves that end the hold where it
// is read: go to the pane the run is drawn in, or act on the run here with the two moves its state
// has (`AMB-D-1002`) — a run going is paused or force-cancelled, a paused one picked up again or
// cancelled. A sentence explaining that cancelling the run frees the definition is what the cancel
// button already says.
import { useState } from "react";
import { LockMark } from "./automationParts";
import { cancelRun, forceCancelRun, pauseRun, resumeRun } from "../core/automations";
import { builtinWord } from "../core/builtinWords";
import { errText, t, tf } from "../core/i18n";
import { ErrorNote } from "../components/ErrorNote";
import type { AutomationRunCardDto } from "../bindings/bindings";

export function AutomationHeldBy({
  runs,
  withAutomation = false,
  onGoToRun,
}: {
  runs: readonly AutomationRunCardDto[];
  /** Whether each band names the automation the run is of — on an action's build screen, where the
   *  runs holding it can be of any automation it is placed on. */
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
            <LockMark />
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
            {run.status === "paused" ? (
              <>
                <button type="button" className="btn" disabled={pressing} onClick={() => void press(() => resumeRun(run.run))}>
                  {t("auto.run.resume")}
                </button>
                <button
                  type="button"
                  className="btn btn--danger"
                  disabled={pressing}
                  onClick={() => void press(() => cancelRun(run.run))}
                >
                  {t("auto.run.cancel")}
                </button>
              </>
            ) : (
              <>
                <button
                  type="button"
                  className="btn"
                  disabled={pressing || run.pauseRequested}
                  onClick={() => void press(() => pauseRun(run.run))}
                >
                  {t("auto.run.pause")}
                </button>
                <button
                  type="button"
                  className="btn btn--danger"
                  disabled={pressing}
                  onClick={() => void press(() => forceCancelRun(run.run))}
                >
                  {t("auto.run.forceCancel")}
                </button>
              </>
            )}
          </li>
        ))}
      </ul>
    </div>
  );
}
