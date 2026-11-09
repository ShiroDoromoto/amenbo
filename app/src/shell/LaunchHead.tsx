// **The header of a launch's pane** — what the launch is and the moves made on the whole of it
// (`AMB-D-1141`). One launch is one pane, and no one run stands for the whole of it, so the header says
// nothing of any run: a run's number, its step and how long it has run are on its seat's row
// (`./RunPicture`), and the run's own moves are on the "running" tab's row.
//
// Left to right: the "auto" chip, the automation's name, where the launch stands, and then "See the
// history", which is where what has already happened in it is read, and its two moves.
//
// - **"Everyone pauses after the task they are on"** asks every run of it to pause before it takes its
//   next task (`AMB-D-1019`): each finishes the task it is on and takes no new one, in every role alike.
//   Once asked, the launch is paused. While a seat is still working its task, the header says how many
//   of its seats have stopped ("Pausing: 1 / 3 seats stopped") and the button says "Cancel the pause"; once
//   every seat has stopped, the header says "Paused" and the button "Resume". Both are the same press
//   (`../core/automations`'s `resumeLaunch`): every role takes tasks again at its headcount — a role
//   stopped on its own stays stopped. A reader who cannot see which seats have stopped cannot tell
//   whether "Resume" is safe to press, so the seats say it too (`./RunPicture`).
// - **"Stop"** is the force-cancel of every run of it (`AMB-D-1002`), so it asks first, as a run's own
//   force-cancel does.
//
// Neither move is drawn once the launch has ended.
//
// **Under the header, what needs a person's hand now** (`LaunchAttention`): a task stopped until the
// task its round's failure was filed as closes, and a seat on a step that waits for a person — one line
// each, the stopped tasks first. Nothing is drawn where there is none.
import { useState } from "react";
import {
  launchPause,
  pauseLaunchBeforeNextTask,
  resumeLaunch,
  stopLaunch,
  useLaunchState,
} from "../core/automations";
import { confirmDialog } from "../core/dialog";
import { errText, t, tf } from "../core/i18n";
import { pushNotice } from "../core/notice";
import { useRefNav } from "../core/refNav";

export function LaunchHead({ launch, project }: { launch: number; project: number }) {
  const state = useLaunchState(launch);
  const ledger = useRefNav();
  // One move at a time: a second press landing while the first is still on its way would be answered
  // off a launch that has already moved.
  const [pressing, setPressing] = useState(false);
  if (state === null) return null;

  const press = async (act: () => Promise<unknown>) => {
    setPressing(true);
    try {
      await act();
    } catch (e) {
      pushNotice(errText(e));
    } finally {
      setPressing(false);
    }
  };
  const stop = async () => {
    const labels = { ok: t("auto.launch.stop"), cancel: t("auto.run.keepRunning") };
    if (!(await confirmDialog(t("auto.launch.stopConfirm"), labels))) return;
    await stopLaunch(launch);
  };

  const going = state.status !== "ended";
  const pause = launchPause(state);
  // A seat still working its task: the pause is asked for and not yet taken hold everywhere.
  const pausing = pause !== null && pause.stopped < pause.seats;
  const where = pause === null
    ? t("auto.launch.running")
    : pausing ? tf("auto.launch.pausing", { n: pause.stopped, m: pause.seats }) : t("auto.launch.paused");
  const hold = pause === null ? "auto.launch.pauseAll" : pausing ? "auto.launch.unpause" : "auto.launch.resume";

  return (
    <div className="launchhead">
      <span className="launchhead__chip launchhead__chip--auto">{t("face.auto")}</span>
      <span className="launchhead__name">{state.automationName}</span>
      {going && (
        <span className={`launchhead__chip launchhead__chip--${pause === null ? "run" : "hold"}`}>{where}</span>
      )}
      <span className="launchhead__gap" />
      {ledger.openRuns !== undefined && (
        <button type="button" className="launchhead__history" onClick={() => ledger.openRuns?.(project, "history")}>
          {t("auto.run.seeHistory")}
        </button>
      )}
      {going && (
        <button
          type="button"
          className="btn launchhead__hold"
          aria-pressed={pause !== null}
          disabled={pressing}
          onClick={() => void press(() => (pause !== null ? resumeLaunch(launch) : pauseLaunchBeforeNextTask(launch)))}
        >
          {t(hold)}
        </button>
      )}
      {going && (
        <button type="button" className="btn btn--danger" disabled={pressing} onClick={() => void press(stop)}>
          {t("auto.launch.stop")}
        </button>
      )}
    </div>
  );
}

/**
 * **What needs a person's hand in the launch `launch` now**, under its header (`AMB-D-1141`): one line
 * each, the stopped tasks first.
 *
 * - **A stopped task** says which task, in whose seat, and that it is not taken until the task its
 *   round's failure was filed as closes — that task pressed to open it.
 * - **A seat waiting for a person** says which task, in whose seat, and "Open the seat", which turns
 *   the pane to that seat's terminal (`onOpenSeat`).
 *
 * The seat is named by its role's name; a role since taken out leaves the task alone.
 */
export function LaunchAttention({ launch, onOpenSeat }: {
  launch: number;
  /** Turn the pane to the terminal of the run `run`. */
  onOpenSeat?: (run: number) => void;
}) {
  const state = useLaunchState(launch);
  const nav = useRefNav();
  if (state === null || state.attention.length === 0) return null;
  const roleName = (role: number | undefined) =>
    role === undefined ? undefined : state.roles.find((one) => one.role === role)?.name;
  return (
    <div className="launchattn" role="status" aria-label={t("auto.launch.attn.label")}>
      {state.attention.map((one) => {
        const stopped = one.kind === "stopped";
        const what = one.taskRef === undefined ? one.stepName ?? "" : `${one.taskRef} ${one.title ?? ""}`.trim();
        const role = roleName(one.role);
        const incident = one.incident;
        return (
          <p key={`${one.kind}:${one.run}:${one.runTask ?? one.task ?? ""}`} className="launchattn__row" data-kind={one.kind}>
            <b className="launchattn__kind">{t(stopped ? "auto.launch.attn.stopped" : "auto.launch.attn.asks")}</b>
            <span className="launchattn__what">{role === undefined ? what : tf("auto.launch.attn.seat", { what, role })}</span>
            {stopped && one.incidentRef !== undefined && (
              <span className="launchattn__why">{tf("auto.launch.attn.until", { ref: one.incidentRef })}</span>
            )}
            {stopped && incident !== undefined && one.incidentRef !== undefined && (
              <button
                type="button"
                className="launchattn__go"
                disabled={!nav.selectTask}
                onClick={() => nav.selectTask?.(incident)}
              >
                {tf("auto.launch.attn.openTask", { ref: one.incidentRef })}
              </button>
            )}
            {!stopped && onOpenSeat !== undefined && (
              <button type="button" className="launchattn__go" onClick={() => onOpenSeat(one.run)}>
                {t("auto.launch.attn.openSeat")}
              </button>
            )}
          </p>
        );
      })}
    </div>
  );
}
