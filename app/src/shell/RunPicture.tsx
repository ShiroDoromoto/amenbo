// **A launch's pane, shown by its roles.**
//
// An automation is split into roles, each with a headcount: so many seats, each taking the tasks
// under that role one at a time (`AMB-D-1123`). One launch is one pane, and what a reader watching it
// wants to know is which role is doing what, and what is waiting for it. So the pane's body draws the
// roles.
//
// **A role with a task is a desk**: one with a run in a seat, or with a task waiting for it to take.
// The desks stand across the whole width of the pane and wrap onto as many lines as they need, in the
// order the roles are defined in, and never move for what they are doing — a desk that jumped to the
// top when a seat asked for a person would be a desk the reader has to look for again. Only the room
// the desks are in scrolls; the pane's own place on the page stays.
//
// **A desk's head is one line, never wrapped**: the role's icon and name, "stops after the task" where
// the role is held, how many seats are at work out of its headcount, a small square per seat, and the
// role's "⋯". What the "⋯" opens is not drawn here. The count leads with its figures ("2 / 3 working"),
// so where the line is short of room it is cut down to its first figure and an ellipsis before anything
// else gives way; the head stays inside the desk, its squares and "⋯" with it.
//
// A seat whose round failed, is paused or waits holds its place but is not counted as at work, and its
// square is not filled as one at work.
//
// **Under the head, a row per seat at work**: its number, the task, the step and how long it has run,
// in whole minutes.
// A seat on a step that may wait for a person is in the person's colour, its border, its band and a
// chip saying so where the time would be; a desk with such a seat has its border in that colour too.
// A seat whose round passed a task on to another role says "→ role" under its step. Where the role is
// held — by itself, or the whole launch paused — each seat at work says it stops after this task. **The
// free seats are one row**, which cannot be pressed. **The tasks waiting** for the role are counted
// under the seats, with the first three of them by reference and title; one passed on from another
// role says which.
//
// **A role with no task is on the bar at the bottom**, by its icon, its name and its headcount.
//
// **A seat is pressed to turn this same pane to its terminal.** The terminal has a bar over it: the way
// back to the roles, and whose round it is — the role, the seat, the task, the step and how long it has
// run. It names no other seat: the way to another seat's terminal is back to the roles and that seat
// pressed. Where a step that may wait for a person turned the pane, the bar says so in one line.
//
// **A failed round that was filed as a task is one line under the pane**, naming the role, whom it was
// filed to and the task (`amenbo_core::ops::automation_stop`).
//
// **A round that needs a person, out of sight in the room, is named at the room's edge**: one that may
// wait for a person, one that failed, and one whose terminal has been quiet into the second stage. What
// is read is the mark that says so on the seat, not the seat: a seat partly in sight whose mark is cut
// off counts. Where such a mark is above what the room shows, a notice at its top says what is there
// and how many — "↑ Waiting for input 1" — and one at its bottom says what is below. Pressing it
// scrolls the room to the nearest of them; nothing scrolls by itself. The notices are read again only
// when the room scrolls or changes size, so a notice does not come and go under the reader's eye.
//
// **A round whose terminal has gone quiet is marked, in two stages**, and only by what was measured:
// how long it has been since anything came out of its terminal (`../talk/moving`). It is read of the
// seat in front — the pane's own run — whose terminal the pane holds. Past five minutes the seat says
// so with a clock in place of its time and takes the heed colour; past twice the time its step is
// expected to take, it takes the stop colour. Nothing is done about it — no round is stopped or turned
// for a silence (`AMB-D-748`). A paused or failed round is not measured.
//
// **The only thing that moves is a small dot on a seat at work**: it fades in and out, slowly, so the
// eye finds which seat is working without the pane moving at the edge of sight all day. A seat that
// waits for a person, is paused, failed or quiet says so in words and stays still. Where the reader
// asks for less motion the dot stays lit and does not fade.
//
// **A run kept from before launches were** sits in no seat: it is drawn as one desk of the role its
// round is under — or of the automation's one role, where the round wrote none down and the automation
// has only one; or of the automation, where it has more — with that run its one seat, and every other
// role of the automation on the bar.
//
// The role's name and icon are the definition's as it is now (`AMB-D-1015`) — the launch's, read with
// it, or the project's roles (`../core/projectRoles`) for a run kept from before launches. The icon is
// drawn from its seed (`AMB-D-686`).
import { memo, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode, type RefObject } from "react";
import { useProjectRoles } from "../core/projectRoles";
import { useLaunchState } from "../core/automations";
import { t, tf } from "../core/i18n";
import { formatNumber } from "../core/i18n/format";
import { Icon } from "../components/Icon";
import { RoleIcon } from "../screens/AutomationRoles";
import { clockOf, type Say } from "../talk/nameplate";
import { quietAt, type Quiet } from "../talk/moving";
import type { AutomationLaunchStateDto, AutomationRunCardDto, TaskRoleDto } from "../bindings/bindings";

/** How the run in a seat stands, as its row says it. */
export type SeatState =
  /** Under way. */
  | "running"
  /** Under way on a step that may stop and wait for a person (`AutomationRunCardDto.interactive`).
   *  Whether it is waiting is not something the app reads (`AMB-D-858`); the step's own flag is what
   *  it goes by. */
  | "asks"
  /** Before a built-in that waits for something, or an action that takes one run at a time. */
  | "waiting"
  /** Held by a pause. */
  | "paused"
  /** The run failed. */
  | "failed";

/** One seat at work, as its row says it. */
export type Seat = {
  readonly run: number;
  /** The task, as its reference and title; null before the round has taken one. */
  readonly task: string | null;
  /** The step it is on; null before the first has opened. */
  readonly step: string | null;
  readonly state: SeatState;
  /** When its round began, in milliseconds since the epoch; null where nothing is written. */
  readonly since: number | null;
  /** The roles its round passed a task on to, by name. */
  readonly to: readonly string[];
};

/** One task waiting for a role to take it. */
export type Waiting = {
  readonly task: number;
  readonly ref: string;
  readonly title: string;
  /** The role it was passed on from, by name; null where none passed it on. */
  readonly from: string | null;
};

/** One role with a task: its seats at work, its free seats and the tasks waiting for it. */
export type Desk = {
  /** The role's id, or null for a round under no role. */
  readonly role: number | null;
  readonly name: string;
  /** The role's icon seed; null for none written, where it is drawn from its name. */
  readonly icon: string | null;
  readonly headcount: number;
  /** Whether it takes no new task — held itself, or the whole launch paused. */
  readonly held: boolean;
  readonly seats: readonly Seat[];
  readonly free: number;
  /** How many tasks are waiting for it, and the first three of them. */
  readonly waiting: number;
  readonly first: readonly Waiting[];
};

/** One role with no task, on the bar at the bottom. */
export type IdleRole = {
  readonly role: number;
  readonly name: string;
  readonly icon: string | null;
  /** Its headcount; null for a run kept from before launches, which holds none. */
  readonly headcount: number | null;
};

/** How the run in a seat stands: off its row, and off the pane's own run where it is that one. */
export function seatStateOf(seat: AutomationRunCardDto, front: Say): SeatState {
  if (seat.status === "failed") return "failed";
  if (seat.status === "paused") return "paused";
  if (seat.waiting || seat.waitsForRoom) return "waiting";
  return seat.interactive === true || (seat.run === front.run && front.interactive) ? "asks" : "running";
}

/** When the round in a seat began, in milliseconds since the epoch, or null where nothing is written. */
function seatSince(seat: AutomationRunCardDto): number | null {
  const at = Date.parse(seat.roleStartedAt ?? seat.startedAt ?? "");
  return Number.isNaN(at) ? null : at;
}

/**
 * **The desks of the launch `launch` and the roles on the bar**, each in the order the roles are
 * defined in, with `front` the pane's own run. A role is a desk where a run is in one of its seats or
 * a task is waiting for it.
 */
export function desksOf(launch: AutomationLaunchStateDto, front: Say): { desks: Desk[]; idle: IdleRole[] } {
  const names = new Map(launch.roles.map((role) => [role.role, role.name]));
  const nameOf = (role: number | undefined) => (role === undefined ? undefined : names.get(role));
  const desks: Desk[] = [];
  const idle: IdleRole[] = [];
  for (const role of launch.roles) {
    if (role.runs.length === 0 && role.waiting === 0) {
      idle.push({ role: role.role, name: role.name, icon: role.icon ?? null, headcount: role.headcount });
      continue;
    }
    desks.push({
      role: role.role,
      name: role.name,
      icon: role.icon ?? null,
      headcount: role.headcount,
      held: role.held || launch.status === "paused",
      seats: role.runs.map((seat) => ({
        run: seat.run,
        task: seat.task === undefined ? null : `${seat.task.ref} ${seat.task.title}`,
        step: seat.stepName ?? null,
        state: seatStateOf(seat, front),
        since: seatSince(seat),
        to: [...new Set(seat.passes.flatMap((pass) => {
          const to = pass.current === true ? nameOf(pass.toRole) : undefined;
          return to === undefined ? [] : [to];
        }))],
      })),
      free: Math.max(0, role.headcount - role.runs.length),
      waiting: role.waiting,
      first: role.firstWaiting.map((task) => ({
        task: task.task,
        ref: task.taskRef,
        title: task.title,
        from: nameOf(task.fromRole) ?? null,
      })),
    });
  }
  return { desks, idle };
}

/**
 * **The desk of a run kept from before launches**, and its automation's other roles on the bar, from
 * the run `run` and the project's roles `roles`. Its one seat is the run itself.
 *
 * A run over has no round going, and nor does one waiting for its next task: what it still carries is
 * the round it closed before.
 */
export function runDesks(run: Say, roles: readonly TaskRoleDto[]): { desks: Desk[]; idle: IdleRole[] } {
  const status = run.state?.status ?? "running";
  const going = status !== "completed" && status !== "canceled" && run.state?.waiting !== true;
  const own = roles.filter((role) => role.automationId === run.automationId);
  // A round that wrote no role down is still under its automation's one role, where it has only one.
  const at = going ? run.round?.role ?? (own.length === 1 ? own[0]!.id : null) : null;
  const seat: Seat = {
    run: run.run,
    task: run.task === null ? null : `${run.task.ref} ${run.task.title}`,
    step: run.step === "" ? null : run.step,
    state: status === "failed" ? "failed" : status === "paused" ? "paused" : run.interactive ? "asks" : "running",
    since: run.round?.startedAt ?? null,
    to: [],
  };
  const deskOf = (role: number | null, name: string, icon: string | null): Desk => ({
    role, name, icon, headcount: 1, held: false, seats: [seat], free: 0, waiting: 0, first: [],
  });
  const idleOf = (role: TaskRoleDto): IdleRole => ({ role: role.id, name: role.name, icon: role.icon, headcount: null });
  if (at === null) {
    // A pane no step has arrived in, on a run that has not read back yet, has no round to draw.
    const drawn = going && (run.round !== undefined || run.step !== "");
    return { desks: drawn ? [deskOf(null, run.automation, null)] : [], idle: own.map(idleOf) };
  }
  return {
    desks: own.filter((role) => role.id === at).map((role) => deskOf(role.id, role.name, role.icon)),
    idle: own.filter((role) => role.id !== at).map(idleOf),
  };
}

/** The clock, read again each second while `ticking`. */
function useNow(ticking: boolean): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!ticking) return;
    setNow(Date.now());
    const tick = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(tick);
  }, [ticking]);
  return now;
}

/** Whether a seat standing at `state` is under way, and so has a time counted and a silence measured. */
function underWay(state: SeatState): boolean {
  return state === "running" || state === "asks";
}

/** How a round's silence stands, as its seat draws it. */
export type Silence = {
  readonly stage: Quiet;
  /** How long the terminal has been quiet, in milliseconds, where `stage` is past 0. */
  readonly for: number | null;
};

/** No silence measured. */
const UNMEASURED: Silence = { stage: 0, for: null };

/**
 * **How the silence of a round standing at `state` stands at `now`**, for a terminal whose last output
 * was at `lastOutput` on a step expected to take `expectedMinutes`. A round not under way is not
 * measured: a pause or a failure is what it is, and a quiet terminal says nothing more about it.
 */
export function silenceOf(
  state: SeatState,
  lastOutput: number | null,
  now: number,
  expectedMinutes: number | null,
): Silence {
  if (!underWay(state)) return UNMEASURED;
  const stage = quietAt(lastOutput, now, expectedMinutes);
  return { stage, for: stage === 0 || lastOutput === null ? null : now - lastOutput };
}

/** What a round needs a person for, as the notice at the room's edge counts it. */
export type Attn = "asks" | "failed" | "stalled";

/** The order the notice names them in. */
const ATTNS: readonly Attn[] = ["asks", "failed", "stalled"];

/** What the notice calls each, with how many. */
const ATTN_KEY = {
  asks: "auto.run.roles.attn.asks",
  failed: "auto.run.roles.attn.failed",
  stalled: "auto.run.roles.attn.stalled",
} as const;

/**
 * **What a round standing at `state` needs a person for**, with its silence at `silence`: one that may
 * wait for a person, one that failed, or one quiet into the second stage. Null for the rest.
 */
export function attnOf(state: SeatState, silence: Silence): Attn | null {
  if (state === "asks") return "asks";
  if (state === "failed") return "failed";
  return silence.stage === 2 ? "stalled" : null;
}

/** How far past the room's edge a mark may stand and still be in sight: a box is measured in
 *  fractions of a pixel, and one drawn flush with the edge can come out a little past it. */
const SLACK = 1;

/** The room left between the room's edge and the mark scrolled to. */
const MARGIN = 8;

/** Where a mark stands — the chip that says a round needs a person — with `top` and `bottom` on the page. */
export type Mark = { readonly attn: Attn; readonly top: number; readonly bottom: number };

/**
 * **The marks out of sight above and below the room** that shows from `top` to `bottom` on the page,
 * each in the order they stand. A mark is out of sight as soon as any of it is past the room's edge: a
 * chip cut in half is not read.
 */
export function unseenOf(top: number, bottom: number, marks: readonly Mark[]): { up: Mark[]; down: Mark[] } {
  return {
    up: marks.filter((mark) => mark.top < top - SLACK),
    down: marks.filter((mark) => mark.bottom > bottom + SLACK),
  };
}

/**
 * **What a notice says for `attns`**: each kind with how many, in the order of `ATTNS`. Null where there
 * are none; `stop` where a failure or a stall is among them, which draws the notice in the stop colour.
 */
export function noticeOf(attns: readonly Attn[]): { what: string; stop: boolean } | null {
  if (attns.length === 0) return null;
  const parts = ATTNS.flatMap((attn) => {
    const n = attns.filter((one) => one === attn).length;
    return n === 0 ? [] : [tf(ATTN_KEY[attn], { n })];
  });
  return { what: parts.join(" ・ "), stop: attns.some((attn) => attn !== "asks") };
}

/** The marks in the room `at`, where they stand on the page. */
function marksIn(at: HTMLElement): Mark[] {
  return [...at.querySelectorAll<HTMLElement>("[data-attn]")].map((el) => {
    const box = el.getBoundingClientRect();
    return { attn: el.dataset.attn as Attn, top: box.top, bottom: box.bottom };
  });
}

/** What is out of sight above and below the room `room`, read again only as it scrolls or changes size. */
function useUnseen(room: RefObject<HTMLDivElement | null>): { up: Attn[]; down: Attn[] } {
  const [unseen, setUnseen] = useState<{ up: Attn[]; down: Attn[] }>({ up: [], down: [] });
  useLayoutEffect(() => {
    const at = room.current;
    if (at === null) return;
    const measure = () => {
      const box = at.getBoundingClientRect();
      // A room not laid out yet — a pane on a page not shown — has nothing out of sight.
      const { up, down } = unseenOf(box.top, box.bottom, box.height === 0 ? [] : marksIn(at));
      const next = { up: up.map((mark) => mark.attn), down: down.map((mark) => mark.attn) };
      setUnseen((before) =>
        before.up.join() === next.up.join() && before.down.join() === next.down.join() ? before : next,
      );
    };
    measure();
    at.addEventListener("scroll", measure, { passive: true });
    if (typeof ResizeObserver === "undefined") return () => at.removeEventListener("scroll", measure);
    const watch = new ResizeObserver(measure);
    watch.observe(at);
    return () => {
      at.removeEventListener("scroll", measure);
      watch.disconnect();
    };
  }, [room]);
  return unseen;
}

/** Scroll the room `at` until the nearest mark out of sight `up` or below it is wholly in sight. */
function scrollToUnseen(at: HTMLElement, up: boolean): void {
  const box = at.getBoundingClientRect();
  const unseen = unseenOf(box.top, box.bottom, marksIn(at));
  const mark = up ? unseen.up[unseen.up.length - 1] : unseen.down[0];
  if (mark === undefined) return;
  const by = up ? mark.top - box.top - MARGIN : mark.bottom - box.bottom + MARGIN;
  const still = typeof window.matchMedia === "function" && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  at.scrollBy({ top: by, behavior: still ? "auto" : "smooth" });
}

/** The notice at the room's top or bottom edge, for the marks `attns` out of sight beyond it. */
function UnseenNotice({ attns, up, onPress }: { attns: readonly Attn[]; up: boolean; onPress: () => void }) {
  const notice = noticeOf(attns);
  if (notice === null) return null;
  return (
    <button
      type="button"
      className="runroles__unseen"
      data-edge={up ? "top" : "bottom"}
      data-stop={notice.stop || undefined}
      aria-label={tf(up ? "auto.run.roles.unseenUp" : "auto.run.roles.unseenDown", { what: notice.what })}
      onClick={onPress}
    >
      {up ? "↑" : "↓"} {notice.what}
    </button>
  );
}

/**
 * **The line under the pane for a failed round**, where it was filed: whose round failed — the role by
 * its name, or the automation where the round was under none — whom it was filed to, and the task.
 * Null where the run did not fail or filed nothing.
 */
export function filedLine(run: Say, roles: readonly TaskRoleDto[]): string | null {
  const incident = run.round?.incident;
  if (incident === undefined || run.state?.status !== "failed") return null;
  const own = run.round?.role ?? null;
  const failed = (own === null ? undefined : roles.find((role) => role.id === own))?.name ?? run.automation;
  const to = incident.role === null ? undefined : roles.find((role) => role.id === incident.role);
  if (to !== undefined) return tf("auto.run.roles.filedRole", { role: failed, to: to.name, ref: incident.ref });
  if (incident.person) return tf("auto.run.roles.filedPerson", { role: failed, ref: incident.ref });
  return tf("auto.run.roles.filed", { role: failed, ref: incident.ref });
}

/** The sentence `key` with its `{n}` drawn bold. */
function withBold(key: string, n: number): ReactNode {
  const [before, after = ""] = t(key).split("{n}");
  return (
    <>
      {before}
      <b>{n}</b>
      {after}
    </>
  );
}

/** What a seat says where its time would be, where its round is doing more than working. */
function SeatChip({ state, silence }: { state: SeatState; silence: Silence }) {
  const attn = attnOf(state, silence);
  if (state === "asks" || state === "failed") {
    return (
      <span className="runseat__chip" data-state={state} data-attn={attn ?? undefined}>
        {t(state === "asks" ? "auto.run.roles.asks" : "auto.run.roles.failed")}
      </span>
    );
  }
  if (silence.stage === 0 || silence.for === null) return null;
  return (
    <span className="runseat__chip" data-quiet={silence.stage} data-attn={attn ?? undefined}>
      <Icon name="clock" />
      {tf("auto.run.roles.quiet", { time: clockOf(silence.for) })}
    </span>
  );
}

/** How long a seat's round has run, in whole minutes, the way the locale writes a number of minutes. */
export function minutesOf(ms: number): string {
  return formatNumber(Math.max(0, Math.floor(ms / 60_000)), { style: "unit", unit: "minute", unitDisplay: "narrow" });
}

/** One seat at work of the role `role`, pressed to turn the pane to its run's terminal where `onOpen` is given. */
function SeatRow({ role, n, seat, held, front, now, silence, onOpen }: {
  role: string; n: number; seat: Seat; held: boolean; front: boolean; now: number; silence: Silence;
  onOpen?: (run: number) => void;
}) {
  const quiet = silence.stage > 0;
  const going = underWay(seat.state);
  const ran = seat.since !== null && going && !quiet && seat.state !== "asks" ? minutesOf(now - seat.since) : null;
  const body = (
    <>
      <span className="runseat__n">{n}</span>
      <span className="runseat__body">
        {seat.task !== null && <span className="runseat__task" title={seat.task}>{seat.task}</span>}
        {seat.step !== null && <span className="runseat__step">{seat.step}</span>}
        {seat.to.map((to) => (
          <span key={to} className="runseat__hand">{tf("auto.run.roles.handsTo", { role: to })}</span>
        ))}
        {held && going && <span className="runseat__last">{t("auto.run.roles.lastTask")}</span>}
      </span>
      <span className="runseat__end">
        <SeatChip state={seat.state} silence={silence} />
        {ran !== null && (
          <span className="runseat__time">
            {tf("face.elapsed", { time: ran })}
            {seat.state === "running" && (
              <span className="runroles__pulse" role="img" aria-label={t("auto.run.roles.working")} />
            )}
          </span>
        )}
      </span>
    </>
  );
  const shared = {
    className: "runseat",
    "data-run": seat.run,
    "data-state": seat.state,
    "data-quiet": quiet ? silence.stage : undefined,
  };
  return (
    <li>
      {onOpen === undefined ? (
        <div {...shared}>{body}</div>
      ) : (
        <button
          type="button"
          {...shared}
          aria-current={front ? "true" : undefined}
          aria-label={tf("auto.run.seatOpen", { role, n, task: seat.task ?? seat.step ?? "" })}
          onClick={() => onOpen(seat.run)}
        >
          {body}
        </button>
      )}
    </li>
  );
}

/** How the square of a seat holding `seat` is filled: empty where it is free, or its round is paused or
 *  waiting; in the stop colour where it failed. */
function squareOf(seat: Seat | undefined, silence: Silence | undefined): "on" | "asks" | "stall" | "failed" | undefined {
  if (seat === undefined) return undefined;
  if (seat.state === "failed") return "failed";
  if (!underWay(seat.state)) return undefined;
  if (seat.state === "asks") return "asks";
  return (silence?.stage ?? 0) > 0 ? "stall" : "on";
}

/** The "⋯" at the end of a desk's head: three dots, drawn. */
function Dots() {
  return (
    <svg viewBox="0 0 16 16" aria-hidden="true">
      <circle cx="3.5" cy="8" r="1.4" fill="currentColor" />
      <circle cx="8" cy="8" r="1.4" fill="currentColor" />
      <circle cx="12.5" cy="8" r="1.4" fill="currentColor" />
    </svg>
  );
}

/** One role with a task: its head, a row per seat at work, its free seats, and the tasks waiting for it. */
function DeskView({ desk, front, now, silenceAt, onOpen }: {
  desk: Desk; front: number; now: number; silenceAt: (seat: Seat) => Silence; onOpen?: (run: number) => void;
}) {
  const silences = desk.seats.map(silenceAt);
  const asks = desk.seats.some((seat) => seat.state === "asks");
  const stalled = silences.some((one) => one.stage > 0);
  // At work is under way: a seat failed, paused or waiting holds its place but is not counted.
  const working = desk.seats.filter((seat) => underWay(seat.state)).length;
  // A square per seat: as many as the headcount, or as many as are at work where that is more.
  const squares = Array.from({ length: Math.max(desk.headcount, desk.seats.length) }, (_, at) => desk.seats[at]);
  return (
    <li
      className="runroles__desk"
      data-role={desk.role ?? undefined}
      data-state={asks ? "asks" : stalled ? "stall" : undefined}
      data-held={desk.held || undefined}
    >
      <div className="runroles__head">
        <RoleIcon role={desk} size={22} />
        <span className="runroles__name">{desk.name}</span>
        {desk.held && <span className="runroles__held">{t("auto.run.roles.held")}</span>}
        <span className="runroles__staff">
          {tf("auto.run.roles.staff", { n: working, m: desk.headcount })}
        </span>
        <span className="runroles__squares" aria-hidden="true">
          {squares.map((seat, at) => (
            <i
              key={at}
              data-state={squareOf(seat, silences[at])}
            />
          ))}
        </span>
        <button
          type="button"
          className="runroles__more"
          aria-haspopup="dialog"
          aria-expanded={false}
          aria-label={tf("auto.run.roles.menu", { name: desk.name })}
        >
          <Dots />
        </button>
      </div>
      {(desk.seats.length > 0 || desk.free > 0) && (
        <ul className="runroles__seats">
          {desk.seats.map((seat, at) => (
            <SeatRow
              key={seat.run}
              role={desk.name}
              n={at + 1}
              seat={seat}
              held={desk.held}
              front={seat.run === front}
              now={now}
              silence={silences[at]!}
              onOpen={onOpen}
            />
          ))}
          {desk.free > 0 && (
            <li>
              <div className="runseat runseat--free">
                <span className="runseat__n">{desk.free}</span>
                <span className="runseat__body">
                  <span className="runseat__task">
                    {tf(desk.held ? "auto.run.seatsFreeHeld" : "auto.run.seatsFree", { n: desk.free })}
                  </span>
                </span>
              </div>
            </li>
          )}
        </ul>
      )}
      {desk.waiting > 0 && (
        <div className="runroles__stack">
          <span className="runroles__stackhead">
            <span className="runroles__sheets" aria-hidden="true" />
            <span>{withBold("auto.run.roles.waiting", desk.waiting)}</span>
          </span>
          {desk.first.map((task) => (
            <span
              key={task.task}
              className="runroles__waiting"
              data-from={task.from === null ? undefined : true}
              title={`${task.ref} ${task.title}`}
            >
              {task.ref} {task.title}
              {task.from !== null && `\u3000${tf("auto.run.roles.from", { role: task.from })}`}
            </span>
          ))}
          {desk.waiting > desk.first.length && (
            <span className="runroles__waiting">{tf("auto.run.roles.more", { n: desk.waiting - desk.first.length })}</span>
          )}
        </div>
      )}
    </li>
  );
}

/**
 * Drawn again only when what it reads of the run changes: the run's words are built afresh on each
 * draw of the workspace, and drawing every pane's desks again on each of those would slow the typing
 * in the panes around it.
 *
 * `lastOutput` reads when the run's terminal last printed (`../talk/plate`). It is read at each tick
 * rather than handed in as a value, because the terminal prints far more often than the desks are
 * drawn.
 */
export const RunRoles = memo(
  function RunRoles({ run, project, launch = null, onOpen, lastOutput }: {
    run: Say;
    project: number;
    /** The launch the pane draws; null for a run kept from before launches were, which sits in no seat. */
    launch?: number | null;
    /** Turn the pane to the terminal of the run `run` — its own, or another seat's of its launch. */
    onOpen?: (run: number) => void;
    lastOutput?: () => number | null;
  }) {
    const roles = useProjectRoles(project);
    const launchState = useLaunchState(launch);
    // A launch not read back yet draws nothing rather than the run alone, which it would then take back.
    const reading = launch !== null && launchState === null;
    const { desks, idle } = useMemo(
      () => (launchState !== null ? desksOf(launchState, run) : reading ? { desks: [], idle: [] } : runDesks(run, roles)),
      [launchState, reading, run, roles],
    );
    const filed = useMemo(() => filedLine(run, roles), [run, roles]);
    const room = useRef<HTMLDivElement>(null);
    const unseen = useUnseen(room);
    const jump = (up: boolean) => {
      if (room.current !== null) scrollToUnseen(room.current, up);
    };
    // The clock ticks while a seat is at work: for its time, and for how long the terminal in front
    // has been quiet, which is measured whether or not the round's start was read.
    const now = useNow(desks.some((desk) => desk.seats.some((seat) => underWay(seat.state))));
    const last = lastOutput?.() ?? null;
    const expected = run.expectedMinutes ?? null;
    // Only the terminal in front is the pane's to hear; another seat's silence is not measured here.
    const silenceAt = (seat: Seat) => (seat.run === run.run ? silenceOf(seat.state, last, now, expected) : UNMEASURED);
    return (
      <div className="runroles" aria-label={t("auto.run.faceRoles")} role="region">
        <div className="runroles__view">
          <div className="runroles__room" ref={room}>
            {desks.length === 0 ? (
              !reading && <p className="runroles__none">{t("auto.run.roles.none")}</p>
            ) : (
              <ul className="runroles__desks">
                {desks.map((desk) => (
                  <DeskView
                    key={desk.role ?? "run"}
                    desk={desk}
                    front={run.run}
                    now={now}
                    silenceAt={silenceAt}
                    onOpen={onOpen}
                  />
                ))}
              </ul>
            )}
          </div>
          <UnseenNotice attns={unseen.up} up onPress={() => jump(true)} />
          <UnseenNotice attns={unseen.down} up={false} onPress={() => jump(false)} />
        </div>
        {idle.length > 0 && (
          <div className="runroles__idle">
            <span className="runroles__idlecount">{tf("auto.run.roles.idle", { n: idle.length })}</span>
            {idle.map((role) => (
              <span key={role.role} className="runroles__idleone" data-role={role.role}>
                <RoleIcon role={role} size={18} />
                {role.name}
                {role.headcount !== null && (
                  <span className="runroles__idlecountone">{tf("auto.run.roles.headcount", { n: role.headcount })}</span>
                )}
              </span>
            ))}
          </div>
        )}
        {filed !== null && (
          <p className="runroles__filed" role="status">
            {filed}
          </p>
        )}
      </div>
    );
  },
  // `onOpen` and `lastOutput` are made afresh on each draw and do the same thing on each, so they are
  // not compared — only whether there is one.
  (before, after) =>
    before.project === after.project &&
    before.launch === after.launch &&
    (before.onOpen === undefined) === (after.onOpen === undefined) &&
    (before.lastOutput === undefined) === (after.lastOutput === undefined) &&
    before.run.run === after.run.run &&
    before.run.automation === after.run.automation &&
    before.run.automationId === after.run.automationId &&
    before.run.step === after.run.step &&
    before.run.interactive === after.run.interactive &&
    before.run.expectedMinutes === after.run.expectedMinutes &&
    before.run.task?.ref === after.run.task?.ref &&
    before.run.task?.title === after.run.task?.title &&
    before.run.round?.role === after.run.round?.role &&
    before.run.round?.startedAt === after.run.round?.startedAt &&
    before.run.round?.incident?.ref === after.run.round?.incident?.ref &&
    before.run.round?.incident?.role === after.run.round?.incident?.role &&
    before.run.round?.incident?.person === after.run.round?.incident?.person &&
    before.run.state?.status === after.run.state?.status &&
    before.run.state?.word === after.run.state?.word &&
    (before.run.state?.waiting === true) === (after.run.state?.waiting === true),
);

/** **Which seat the run `run` sits in** in the launch `launch` — its role and its number from 1 — or
 *  null where it sits in none of them. */
export function seatOf(launch: AutomationLaunchStateDto | null, run: number): { role: string; n: number } | null {
  for (const role of launch?.roles ?? []) {
    const at = role.runs.findIndex((one) => one.run === run);
    if (at >= 0) return { role: role.name, n: at + 1 };
  }
  return null;
}

/**
 * **The bar over a run's terminal**: the way back to the roles, whose round this is — its role, its
 * seat, its task — what step it is on, said to have failed where the run failed on it, how long the
 * round has run and, where a step that may wait for a person turned the pane (`byStep`), the line
 * that says so.
 */
export function RunTermBar({ run, project, launch, byStep, onBack }: {
  run: Say;
  project: number;
  /** The launch the pane draws, which the seat's number is read off; null for a run kept from before
   *  launches were. */
  launch: number | null;
  byStep: boolean;
  onBack: () => void;
}) {
  const roles = useProjectRoles(project);
  const state = useLaunchState(launch);
  const seat = useMemo(() => seatOf(state, run.run), [state, run.run]);
  const own = run.round?.role ?? null;
  const role = own === null ? undefined : roles.find((one) => one.id === own);
  const name = role?.name ?? run.automation;
  // The time is the round's, counted while it is under way, as on its seat.
  const status = run.state?.status ?? "running";
  const going = status === "running" && run.state?.waiting !== true;
  const since = run.round?.startedAt ?? run.startedAt;
  const now = useNow(going && since !== null);
  const ran = going && since !== null ? clockOf(now - since) : null;
  const head = [
    name,
    seat === null ? null : tf("auto.run.seat", { n: seat.n }),
    run.task === null ? null : `${run.task.ref} ${run.task.title}`,
  ].filter((one) => one !== null).join(" ・ ");
  return (
    <div className="runterm">
      <div className="runterm__head">
        <button type="button" className="runterm__back" onClick={onBack}>
          {t("auto.run.backToRoles")}
        </button>
        {role === undefined ? <Icon name="robot" /> : <RoleIcon role={role} size={18} />}
        <span className="runterm__body">
          <span className="runterm__task">{head}</span>
          {run.step !== "" && (
            <span className="runterm__step">
              {run.state?.status === "failed" ? tf("auto.run.stepFailed", { step: run.step }) : run.step}
            </span>
          )}
        </span>
        {ran !== null && (
          <span className="runterm__time">
            {tf("face.elapsed", { time: ran })}
            {!run.interactive && (
              <span className="runroles__pulse" role="img" aria-label={t("auto.run.roles.working")} />
            )}
          </span>
        )}
      </div>
      {byStep && (
        <p className="runterm__notice" role="status">
          {tf("auto.run.turnedForStep", { role: name })}
        </p>
      )}
    </div>
  );
}

/**
 * **How a failed round's terminal ended**, as the lines written under what it left to be read: that its
 * program ended — with the code it ended with, where that was heard — and where Amenbo filed the
 * failure. They are the terminal's last lines (`../talk/terminal` `mountTail`), on the terminal face of a
 * failed run only, the one a person opens from the task the round was filed as
 * (`../screens/TaskDetailPane`).
 */
export function useRoundFellLines(run: Say | null, project: number, code: number | null): string[] {
  const roles = useProjectRoles(project);
  return useMemo(() => {
    if (run === null) return [];
    const ended = code === null ? t("face.endedNoCode") : tf("face.endedCode", { code: String(code) });
    const filed = filedInTerminal(run, roles);
    return filed === null ? [ended] : [ended, filed];
  }, [run, roles, code]);
}

/**
 * **Where Amenbo filed a failed round, as its terminal's last line says it** — to the role it was filed
 * to, to a person, or to neither where that role is gone. Null where the run filed nothing. It is read
 * off the round's incident alone, not off the desks: a round of an automation that gives it no role
 * sits at no role's desk, and is filed all the same.
 */
function filedInTerminal(run: Say, roles: readonly TaskRoleDto[]): string | null {
  const incident = run.round?.incident;
  if (incident === undefined) return null;
  const to = incident.role === null ? undefined : roles.find((role) => role.id === incident.role);
  if (to !== undefined) return tf("auto.run.term.filedRole", { to: to.name });
  if (incident.person) return t("auto.run.term.filedPerson");
  return t("auto.run.term.filed");
}
