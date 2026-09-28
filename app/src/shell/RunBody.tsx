// **What a run's pane says where no terminal is up and no built-in is being carried out**
// (`AMB-T-5753`).
//
// A run's pane stands with nothing running in it once the run is over, and when it has come back with
// the app for a run held (`AMB-T-5635`). The row above says which step and the state in one word; the
// body says what that means and what the reader can do next, where it was an empty box before.
//
// **An over run says how it ended and where**, by the step the row names, and holds the press to where
// the run is read from now — the same one the row holds (`AMB-T-5539`), since the body is where a reader
// looking at an empty pane looks first. What was done on each step is the ledger's to say, not here.
//
// **It stands in place of a built-in's card as well.** A run stopped while a built-in waited keeps the
// card it was waiting on, and a turning mark under a row that says "canceled" says it is still
// waiting.
import type { Say } from "../talk/nameplate";
import { t, tf } from "../core/i18n";

/** The body of a run that is over — completed, failed or canceled (`AMB-D-955`). */
export function RunOverCard({ run, onSee, see }: { run: Say; onSee?: () => void; see: string }) {
  const status = run.state?.status;
  const [mark, line] =
    status === "completed"
      ? ["✓", tf("auto.run.body.completed", { step: run.step })]
      : status === "failed"
        ? ["!", tf("auto.run.body.failed", { step: run.step })]
        : ["■", tf("auto.run.body.canceled", { step: run.step })];
  return (
    <div className="slot__builtin slot__runbody" role="status" data-status={status}>
      <span className="slot__runbody-mark" role="img" aria-label={run.state?.word}>{mark}</span>
      <p className="slot__runbody-line">{line}</p>
      {onSee !== undefined && (
        <button type="button" className="btn slot__runbody-see" onClick={onSee}>
          {see}
        </button>
      )}
    </div>
  );
}

/** The body of a run held with no step up — one come back with the app paused. The press that picks
 *  it up again is the row's. */
export function RunPausedCard() {
  return (
    <div className="slot__builtin slot__runbody" role="status" data-status="paused">
      <p className="slot__runbody-line">{t("auto.run.body.paused")}</p>
    </div>
  );
}
