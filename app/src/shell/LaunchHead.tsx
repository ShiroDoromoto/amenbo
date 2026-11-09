// **The header of a launch's pane** — what the launch is and the moves made on the whole of it, over
// the seat in front (`./TerminalPane`). One launch is one pane, so these are the launch's and not any
// one run's: the run's own moves are on the "running" tab's row.
//
// Left to right: the "auto" chip, the automation's name, where the launch stands, how many of its seats
// are working a task out of how many it has, and then its two moves.
//
// - **"Pause after the current task"** asks every run of it to pause before it takes its next task
//   (`AMB-D-1019`): each finishes the task it is on and takes no new one, in every role alike. Once
//   asked, the launch is paused, and the same place says "Resume", pressed (`aria-pressed`). Resumed,
//   every role takes tasks again at its headcount — a role stopped on its own stays stopped
//   (`../core/automations`'s `resumeLaunch`). A run whose round takes no task (one of a role started
//   by a trigger) is asked too, and pauses where it comes back to where its round starts.
// - **"Stop"** is the force-cancel of every run of it (`AMB-D-1002`), so it asks first, as a run's own
//   force-cancel does.
//
// Neither move is drawn once the launch has ended.
import { useState } from "react";
import {
  launchHeadcount,
  launchWorking,
  pauseLaunchBeforeNextTask,
  resumeLaunch,
  stopLaunch,
  useLaunchState,
} from "../core/automations";
import { confirmDialog } from "../core/dialog";
import { errText, t } from "../core/i18n";
import { pushNotice } from "../core/notice";

export function LaunchHead({ launch }: { launch: number }) {
  const state = useLaunchState(launch);
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
  const paused = state.status === "paused";
  // The count is split around its number so the number alone is drawn bold.
  const total = String(launchHeadcount(state));
  const [before, after = ""] = t("auto.launch.working").split("{n}").map((part) => part.replace("{m}", total));

  return (
    <div className="launchhead">
      <span className="launchhead__chip launchhead__chip--auto">{t("face.auto")}</span>
      <span className="launchhead__name">{state.automationName}</span>
      {going && (
        <span className={`launchhead__chip launchhead__chip--${paused ? "hold" : "run"}`}>
          {t(paused ? "auto.run.pauseAfterTask" : "auto.launch.running")}
        </span>
      )}
      {going && (
        <span className="launchhead__count">
          {before}
          <b>{launchWorking(state)}</b>
          {after}
        </span>
      )}
      <span className="launchhead__gap" />
      {going && (
        <button
          type="button"
          className="btn launchhead__hold"
          aria-pressed={paused}
          disabled={pressing}
          onClick={() => void press(() => (paused ? resumeLaunch(launch) : pauseLaunchBeforeNextTask(launch)))}
        >
          {t(paused ? "auto.launch.resume" : "auto.run.pauseAfterTask")}
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
