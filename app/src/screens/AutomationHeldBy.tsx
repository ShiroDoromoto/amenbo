// **The runs holding a definition**, drawn over a build screen while there is one (`AMB-D-961`).
//
// Core refuses every rewrite of an automation, or of an action placed on one, while a run of it is
// running or paused. A screen that went on offering its fields would let a reader press and then be
// refused, so both build screens hold their fields shut for as long as this list is not empty
// (`./AutomationBuildScreen`, `./AutomationActionBuildScreen`).
//
// **Held shut says why, and names the way out.** What lets the definition be edited again is ending
// those runs, so each is one line of the "running" tab (`./RunningTab`'s `RunLine`), pressed to go to
// the pane it is drawn in, where it is stopped.
import { RunLine } from "./RunningTab";
import { t } from "../core/i18n";
import type { AutomationRunCardDto } from "../bindings/bindings";

export function AutomationHeldBy({
  runs,
  onGoToRun,
}: {
  runs: readonly AutomationRunCardDto[];
  /** Go to the pane a run is drawn in. Absent where there is no workspace face to send anybody to. */
  onGoToRun?: (project: number, run: number) => void;
}) {
  if (runs.length === 0) return null;
  return (
    <div className="autoheld">
      <span className="actbuild__sec">{t("auto.held.place")}</span>
      <p className="autoheld__what">{t("auto.held.what")}</p>
      <ul className="autoruns">
        {runs.map((run) => (
          <RunLine key={run.run} run={run} onGo={onGoToRun && (() => onGoToRun(run.project, run.run))} />
        ))}
      </ul>
    </div>
  );
}
