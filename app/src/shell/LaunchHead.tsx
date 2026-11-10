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
// **At the header's end, "⋯" opens the launch's menu**, built as a role's menu is (`./RunPicture`): the
// most runs that go at once in this launch alone, across its roles (`AMB-D-1145`). "No limit" with a
// press to set one, or a stepper from 1 to the cap core sets with a press to take it off; beside it the
// default — the definition's — which the launch goes back to when it ends, and a press back to it where
// the two differ. It is not drawn once the launch has ended either.
//
// **Under the header, what needs a person's hand now** (`LaunchAttention`): a task stopped until the
// task its round's failure was filed as closes, and a seat on a step that waits for a person — one line
// each, the stopped tasks first. Nothing is drawn where there is none.
import { useRef, useState, type RefObject } from "react";
import {
  launchPause,
  pauseLaunchBeforeNextTask,
  resetLaunchLimit,
  resumeLaunch,
  setLaunchLimit,
  stopLaunch,
  useLaunchState,
} from "../core/automations";
import { confirmDialog } from "../core/dialog";
import { errText, t, tf } from "../core/i18n";
import { pushNotice } from "../core/notice";
import { useRefNav } from "../core/refNav";
import { Menu } from "../components/Menu";
import { Stepper } from "../components/Stepper";
import { Dots, underEnd } from "./RunPicture";
import type { AutomationLaunchStateDto } from "../bindings/bindings";

/** The fewest runs a limit lets go at once. The most is core's (`AutomationLaunchStateDto.maxRunLimit`). */
const MIN_RUN_LIMIT = 1;

/** "Default: N", or "Default: no limit", for the run limit the definition gives. */
function definedLimit(limit: number | undefined): string {
  return limit === undefined ? t("auto.run.limit.definedNone") : tf("auto.run.limit.defined", { n: limit });
}

/**
 * **The launch's menu**, hung under the right end of its "⋯" at `at`: the run limit for this launch
 * alone. One press at a time, as a role's menu: a second landing while the first is on its way would be
 * answered off a launch that has already moved. A refusal is said on the toast.
 */
function LaunchMenu({ state, at, from, onClose }: {
  state: AutomationLaunchStateDto; at: { x: number; y: number };
  from: RefObject<HTMLButtonElement | null>; onClose: () => void;
}) {
  const [pressing, setPressing] = useState(false);
  const press = async (act: () => Promise<void>) => {
    setPressing(true);
    try {
      await act();
    } catch (e) {
      pushNotice(errText(e));
    } finally {
      setPressing(false);
    }
  };
  const write = (limit: number | null) => {
    if (!pressing) void press(() => setLaunchLimit(state.launch, limit));
  };
  const limit = state.runLimit;
  // A limit is first set at the headcounts added up, the most that go at once without one, within the cap.
  const headcounts = state.roles.reduce((sum, role) => sum + role.headcount, 0);
  return (
    <Menu
      at={at}
      align="end"
      kind="dialog"
      label={t("auto.run.limit.menu")}
      from={from}
      className="menu--panel runmenu"
      onClose={onClose}
    >
      <div className="menu__head">
        <span>{state.automationName}</span>
      </div>
      <div className="menu__sec">
        <span className="menu__label">{t("auto.run.limit.title")}</span>
        <div className="menu__row">
          {limit === undefined ? (
            <>
              <span>{t("auto.run.limit.none")}</span>
              <button
                type="button"
                className="btn"
                disabled={pressing}
                onClick={() => write(Math.min(Math.max(headcounts, MIN_RUN_LIMIT), state.maxRunLimit))}
              >
                {t("auto.run.limit.set")}
              </button>
            </>
          ) : (
            <Stepper
              small
              value={limit}
              min={MIN_RUN_LIMIT}
              max={state.maxRunLimit}
              label={t("auto.run.limit.of")}
              valueText={tf("auto.run.limit.value", { n: limit })}
              lessLabel={t("auto.side.headcountLess")}
              moreLabel={t("auto.side.headcountMore")}
              onStep={write}
            />
          )}
          <span className="menu__hint">{definedLimit(state.definedRunLimit)}</span>
        </div>
        <span className="menu__hint">{tf("auto.run.limit.underWay", { n: state.underWay })}</span>
        <p className="menu__help">{t("auto.run.limit.help")}</p>
        {limit !== undefined && (
          <button type="button" className="menu__reset" disabled={pressing} onClick={() => write(null)}>
            {t("auto.run.limit.clear")}
          </button>
        )}
        {limit !== state.definedRunLimit && (
          <button
            type="button"
            className="menu__reset"
            disabled={pressing}
            onClick={() => void press(() => resetLaunchLimit(state.launch))}
          >
            {t("auto.run.limit.reset")}
          </button>
        )}
      </div>
    </Menu>
  );
}

export function LaunchHead({ launch, project }: { launch: number; project: number }) {
  const state = useLaunchState(launch);
  const ledger = useRefNav();
  // One move at a time: a second press landing while the first is still on its way would be answered
  // off a launch that has already moved.
  const [pressing, setPressing] = useState(false);
  // Where the launch's menu is open, or null where it is not.
  const [menuAt, setMenuAt] = useState<{ x: number; y: number } | null>(null);
  const more = useRef<HTMLButtonElement>(null);
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
      {going && (
        <button
          type="button"
          className="runroles__more"
          ref={more}
          aria-haspopup="dialog"
          aria-expanded={menuAt !== null}
          aria-label={t("auto.run.limit.menu")}
          onClick={(e) => {
            // Focused by hand, as a press does not focus a button in every web view: the menu gives the
            // focus back to what held it when it opened.
            e.currentTarget.focus();
            setMenuAt(menuAt === null ? underEnd(e.currentTarget) : null);
          }}
        >
          <Dots />
        </button>
      )}
      {going && menuAt !== null && (
        <LaunchMenu state={state} at={menuAt} from={more} onClose={() => setMenuAt(null)} />
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
