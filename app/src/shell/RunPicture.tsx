// **A run's pane, shown by its roles.**
//
// An automation is split into roles, and a run works a round per task under the role the task names
// (`amenbo_core::model::AutomationRunTask`). What a reader watching a run wants to know is which role
// is doing what, and the automation's picture did not say it: it drew where the run had been, not who
// was at work. So the pane's body draws the roles instead.
//
// **A role with a round is a card**, and a card says the role, the task the round is on, the step
// under way and how long the round has run. A round that may wait for a person, and one that failed,
// say so on the card. **A failed round that was filed as a task is one line under the pane**, naming the
// role, whom it was filed to and the task (`amenbo_core::ops::automation_stop`). **A role with no
// round is an icon on the bar at the bottom**, and nothing else:
// the bar scrolls sideways inside itself where the roles do not fit, so it never pushes the cards out
// of the pane.
//
// **The cards stand in the order the roles are defined in**, and never move for what they are doing:
// a card that jumped to the top when its round asked for a person would be a card the reader has to
// look for again. Only the room the cards are in scrolls — the pane's own place on the page stays.
//
// **A round that needs a person, out of sight in the room, is named at the room's edge**: one that may
// wait for a person, one that failed, and one whose terminal has been quiet into the second stage. What
// is read is the mark that says so on the card or row, not the card: a card partly in sight whose mark
// is cut off counts. Where such a mark is above what the room shows, a notice at its top says what is
// there and how many — "↑ Waiting for input 1" — and one at its bottom says what is below. Pressing it
// scrolls the room to the nearest of them; nothing scrolls by itself. The notices are read again only
// when the room scrolls or changes size, so a notice does not come and go under the reader's eye while
// they read the cards.
//
// **The cards are a list where they do not fit.** Whether every card fits is read off the room the
// cards are drawn in, as large as it is now — a pane is narrowed by the window, by the panel at the
// right and by splitting the page (`AMB-D-939`), and all three are the same room getting smaller. A
// card needs 150px by 118px; where the room holds fewer cards than there are rounds, each round is one
// row of a list instead, and a narrow list puts a row on two lines. The list turns back to cards only
// once the room has grown 40px past where it turned, so a pane on the edge does not flip back and forth
// while it is being dragged. The reader can hold it to cards or to the list in the row above; it is on
// its own until they do.
//
// **A card draws only the lines that touch it**: what it hands on to another role, and the role its
// failure was filed to. A line is drawn only where both cards are in the room; where the other role
// has no card, the card says "→ role" instead, and so does a row of the list, which draws no line.
// The roles' own definitions say nothing of which role hands on to which — the next role is the one
// the handed task names when it is handed — so what the run carries today is the filing alone.
//
// **A round whose terminal has gone quiet is marked, in two stages**, and only by what was measured:
// how long it has been since anything came out of its terminal (`../talk/moving`). Past five minutes
// the card says so with a clock and its border takes the heed colour; past twice the time its step is
// expected to take, its border and its ground take the stop colour. The time on the clock counts every
// second, and the card's look changes only where the stage does. Nothing is done about it — no round is
// stopped or turned for a silence (`AMB-D-748`) — and it is the same for every agent, because what is
// read is the stream and not anything the agent says. A paused or failed round is not measured.
//
// **The only thing that moves is a small dot on a card whose round is under way**: it fades in and
// out, slowly, so the eye finds which role is at work without the pane moving at the edge of sight all
// day. A card that waits for a person, is paused or failed says so in words and stays still, and so
// does a row of the list. A card whose terminal has gone quiet loses its dot too: the clock says what
// the stream shows, and a dot would say something is happening that the stream does not. Where the
// reader asks for less motion the dot stays lit and does not fade.
// Nothing else is animated: a pane that changes size moves its cards only by where they stand.
//
// **The cards are the run in front's** — the pane's own run, or on a launch's pane the seat in front —
// and a run walks one round at a time, so the cards are its round's role or none. A round under no
// role — a task that names none, on an automation whose roles start nowhere — is drawn as one card
// named after the automation, its roles all on the bar.
//
// The role's name and icon are the definition's as it is now (`AMB-D-1015`), read by the project's
// roles (`../core/projectRoles`) — the run's copy holds only which role its round is under.
//
// **One launch is one pane, and its seats are listed under the room** (`AMB-D-1123`): each role with
// someone in a seat, and under it a row per seat — its number, the task, the step and how long it has
// run. A seat is pressed to turn this same pane to that seat's terminal; the seats nobody is in are one
// row that cannot be pressed. A card is pressed for the terminal of the seat in front.
//
// **The terminal has a bar over it**: the way back to the roles, and whose round it is — the role, the
// seat, the task, the step and how long it has run. It names no other seat: the way to another seat's
// terminal is back to the roles and that seat pressed. Where a step that may wait for a person turned
// the pane, the bar says so in one line.
import { memo, useEffect, useLayoutEffect, useMemo, useRef, useState, type RefObject } from "react";
import { useProjectRoles } from "../core/projectRoles";
import { useLaunchState } from "../core/automations";
import { t, tf } from "../core/i18n";
import { Icon, isIconName, type IconName } from "../components/Icon";
import { clockOf, type Say } from "../talk/nameplate";
import { quietAt, type Quiet } from "../talk/moving";
import type { AutomationLaunchStateDto, AutomationRunCardDto, TaskRoleDto } from "../bindings/bindings";

/** What a role's round is doing, as its card says it. */
export type RoundState =
  /** Under way. */
  | "running"
  /** Under way on a step that may stop and wait for a person (`Say.interactive`). Whether it is
   *  waiting is not something the app reads (`AMB-D-858`); the step's own flag is what it goes by. */
  | "asks"
  /** Held by a pause. */
  | "paused"
  /** The run failed in it. */
  | "failed";

/** One card: the role, or the automation where its round is under none, and what its round is doing. */
export type RoleCard = {
  /** The role's id, or null for a round under no role. */
  readonly role: number | null;
  readonly name: string;
  readonly icon: IconName;
  readonly state: RoundState;
};

/** The icon a role is drawn with: the one it was given, where it is one drawn here. */
function iconOf(icon: string | null | undefined): IconName {
  return icon !== null && icon !== undefined && isIconName(icon) ? icon : "person";
}

/** The icon the role `role` is drawn with. */
function roleIcon(role: TaskRoleDto): IconName {
  return iconOf(role.icon);
}

/**
 * **Which roles are cards and which are on the bar**, for the run `run` and the project's roles
 * `roles`, each in the order the roles are defined in.
 *
 * A run over has no round going, and nor does one waiting for its next task: what it still carries is
 * the round it closed before.
 */
export function roleCards(run: Say, roles: readonly TaskRoleDto[]): { cards: RoleCard[]; idle: TaskRoleDto[] } {
  const status = run.state?.status ?? "running";
  const going = status !== "completed" && status !== "canceled" && run.state?.waiting !== true;
  const state: RoundState =
    status === "failed" ? "failed" : status === "paused" ? "paused" : run.interactive ? "asks" : "running";
  const own = roles.filter((role) => role.automationId === run.automationId);
  const at = going ? run.round?.role ?? null : null;
  if (at === null) {
    // A pane no step has arrived in, on a run that has not read back yet, has no round to draw.
    const drawn = going && (run.round !== undefined || run.step !== "");
    return { cards: drawn ? [{ role: null, name: run.automation, icon: "robot", state }] : [], idle: own };
  }
  return {
    cards: own
      .filter((role) => role.id === at)
      .map((role) => ({ role: role.id, name: role.name, icon: roleIcon(role), state })),
    idle: own.filter((role) => role.id !== at),
  };
}

/** How the room shows its rounds: a card each, or a row each of one list. */
export type RoomView = "cards" | "list";

/** What the reader holds the room to, or `auto` for the room's size to decide. */
export type ViewPin = "auto" | RoomView;

/** The least a card is drawn at, and the room between the cards and around them (`--s-2`). */
const CARD_W = 150;
const CARD_H = 118;
const GAP = 8;
const PAD = 8;
/** How far the room grows past where it turned to the list before it turns back to cards. */
const BACK = 40;

/** **How many cards a room of `width` by `height` holds**, the room's padding included in both. */
export function cardsFit(width: number, height: number): number {
  const across = Math.max(0, Math.floor((width - PAD * 2 + GAP) / (CARD_W + GAP)));
  const down = Math.max(0, Math.floor((height - PAD * 2 + GAP) / (CARD_H + GAP)));
  return across * down;
}

/** What the room is showing, and the size it was when it turned to the list; null before it did. */
export type Fit = { readonly view: RoomView; readonly width: number | null; readonly height: number | null };

/**
 * **What the room shows for `need` rounds at `width` by `height`**, where it showed `before`. Cards
 * that no longer all fit turn to the list at once; the list turns back only once they fit and the room
 * is `BACK` wider or taller than where it turned.
 */
export function fitOf(before: Fit | null, width: number, height: number, need: number): Fit {
  const fits = cardsFit(width, height) >= need;
  if (before === null) return { view: fits ? "cards" : "list", width: null, height: null };
  if (before.view === "cards") return fits ? before : { view: "list", width, height };
  if (!fits) return before;
  const grown =
    before.width === null || before.height === null || width >= before.width + BACK || height >= before.height + BACK;
  return grown ? { view: "cards", width: null, height: null } : before;
}

/** What the room `room` shows for `need` rounds, measured again as it changes size. */
function useFit(room: RefObject<HTMLDivElement | null>, need: number): RoomView {
  const [fit, setFit] = useState<Fit | null>(null);
  useLayoutEffect(() => {
    const at = room.current;
    if (at === null) return;
    const measure = () => {
      const width = at.clientWidth, height = at.clientHeight;
      // A room not laid out yet — a pane on a page not shown — says nothing of what fits.
      if (width === 0 || height === 0) return;
      setFit((before) => fitOf(before, width, height, need));
    };
    measure();
    if (typeof ResizeObserver === "undefined") return;
    const watch = new ResizeObserver(measure);
    watch.observe(at);
    return () => watch.disconnect();
  }, [room, need]);
  return fit?.view ?? "cards";
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

/** Whether the round of `card` is under way, and so has a time counted and a silence measured. */
function underWay(card: RoleCard): boolean {
  return card.state === "running" || card.state === "asks";
}

/** How a round's silence stands, as its card and its row draw it. */
export type Silence = {
  readonly stage: Quiet;
  /** How long the terminal has been quiet, in milliseconds, where `stage` is past 0. */
  readonly for: number | null;
};

/**
 * **How the silence of `card`'s round stands at `now`**, for a terminal whose last output was at
 * `lastOutput` on a step expected to take `expectedMinutes`. A round not under way is not measured: a
 * pause or a failure is what it is, and a quiet terminal says nothing more about it.
 */
export function silenceOf(
  card: RoleCard,
  lastOutput: number | null,
  now: number,
  expectedMinutes: number | null,
): Silence {
  if (!underWay(card)) return { stage: 0, for: null };
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
 * **What `card`'s round needs a person for**, with its silence at `silence`: one that may wait for a
 * person, one that failed, or one quiet into the second stage. Null for the rest.
 */
export function attnOf(card: RoleCard, silence: Silence): Attn | null {
  if (card.state === "asks") return "asks";
  if (card.state === "failed") return "failed";
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

/** What is out of sight above and below the room `room`, read again only as it scrolls or changes
 *  size — and as it turns between cards and the list, which only a change of size or a press does. */
function useUnseen(room: RefObject<HTMLDivElement | null>, view: RoomView): { up: Attn[]; down: Attn[] } {
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
  }, [room, view]);
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

/** The clock a quiet round is marked with: how long its terminal has printed nothing. It is the mark the
 *  notice at the room's edge reads where `attn` is a stall. */
function QuietChip({ silence, attn }: { silence: Silence; attn: Attn | null }) {
  if (silence.stage === 0 || silence.for === null) return null;
  return (
    <span
      className="runroles__chip runroles__quiet"
      data-quiet={silence.stage}
      data-attn={attn === "stalled" ? attn : undefined}
    >
      <Icon name="clock" />
      {tf("auto.run.roles.quiet", { time: clockOf(silence.for) })}
    </span>
  );
}

/** The chip a card or row says `chipOf` in. It is the mark the notice at the room's edge reads where
 *  `attn` is a wait or a failure. */
function Chip({ state, attn, children }: { state: RoleCard["state"]; attn: Attn | null; children: string }) {
  return (
    <span
      className="runroles__chip"
      data-state={state}
      data-attn={attn === "asks" || attn === "failed" ? attn : undefined}
    >
      {children}
    </span>
  );
}

/** What a card says under the time, where its round is doing more than working. */
function chipOf(card: RoleCard, run: Say): string | null {
  if (card.state === "asks") return t("auto.run.roles.asks");
  if (card.state === "failed") return t("auto.run.roles.failed");
  if (card.state === "paused") return run.state?.word ?? null;
  return null;
}

/**
 * **The line under the pane for a failed round**, where it was filed: whose round failed, whom it was
 * filed to — the role by its name, or a person — and the task. Null where no failed card was filed.
 */
export function filedLine(cards: readonly RoleCard[], run: Say, roles: readonly TaskRoleDto[]): string | null {
  const incident = run.round?.incident;
  const failed = cards.find((card) => card.state === "failed");
  if (incident === undefined || failed === undefined) return null;
  const to = incident.role === null ? undefined : roles.find((role) => role.id === incident.role);
  if (to !== undefined) return tf("auto.run.roles.filedRole", { role: failed.name, to: to.name, ref: incident.ref });
  if (incident.person) return tf("auto.run.roles.filedPerson", { role: failed.name, ref: incident.ref });
  return tf("auto.run.roles.filed", { role: failed.name, ref: incident.ref });
}

/** One line out of a card: to the role it hands on to, or to the role its failure was filed to. */
export type Hand = {
  /** The card's role, or null for a round under no role. */
  readonly from: number | null;
  readonly to: number;
  /** The name of the role at the other end. */
  readonly name: string;
  /** Where the failure was filed, drawn in the stop colour; otherwise a hand-on. */
  readonly filed: boolean;
};

/**
 * **The lines out of the cards `cards`**: the role each failed round was filed to, where it was filed
 * to a role of the project. A failure filed to a person, or to a role since gone, has no line.
 */
export function handsOf(cards: readonly RoleCard[], run: Say, roles: readonly TaskRoleDto[]): Hand[] {
  const incident = run.round?.incident;
  if (incident === undefined || incident.role === null) return [];
  const to = roles.find((role) => role.id === incident.role);
  if (to === undefined) return [];
  return cards
    .filter((card) => card.state === "failed")
    .map((card) => ({ from: card.role, to: to.id, name: to.name, filed: true }));
}

/**
 * **The roles a card or a row writes as "→ role"**: the ends of the lines out of the role `from` that
 * have no card among `shown`. A row of the list passes no card, and so writes every end.
 */
export function handsAway(from: number | null, hands: readonly Hand[], shown: readonly RoleCard[]): Hand[] {
  return hands.filter((hand) => hand.from === from && !shown.some((card) => card.role === hand.to));
}

/** One line drawn between two cards, in the room's own coordinates. */
type Stroke = { key: string; path: string; head: string; filed: boolean };

/** A card's box within the room. */
type Box = { l: number; t: number; r: number; b: number; cx: number };

/**
 * **The line from the card at `a` to the card at `b`**: straight across where they stand side by
 * side, and otherwise down or up from the middle of one to the middle of the other, turning halfway.
 */
function strokeOf(a: Box, b: Box): { path: string; head: string } {
  if (Math.abs(a.t - b.t) < 4) {
    const y = a.t + 24;
    const [x0, x1, d] = b.l >= a.r ? [a.r, b.l - 2, -6] : [a.l, b.r + 2, 6];
    return { path: `M${x0} ${y} H${x1}`, head: `M${x1 + d} ${y - 4} L${x1} ${y} L${x1 + d} ${y + 4}` };
  }
  const down = b.t > a.t;
  const [y0, y1, d] = down ? [a.b, b.t - 2, -6] : [a.t, b.b + 2, 6];
  const ym = (y0 + y1) / 2;
  return {
    path: `M${a.cx} ${y0} V${ym} H${b.cx} V${y1}`,
    head: `M${b.cx - 4} ${y1 + d} L${b.cx} ${y1} L${b.cx + 4} ${y1 + d}`,
  };
}

/** The lines between the cards in `room`, measured again as the room changes size. */
function useStrokes(room: RefObject<HTMLDivElement | null>, hands: readonly Hand[], view: RoomView): Stroke[] {
  const [strokes, setStrokes] = useState<Stroke[]>([]);
  useLayoutEffect(() => {
    const at = room.current;
    if (at === null) return;
    const measure = () => {
      const box = (role: number | null): Box | null => {
        const card = at.querySelector<HTMLElement>(`.runroles__card[data-role="${role}"]`);
        if (card === null) return null;
        const l = card.offsetLeft, t = card.offsetTop;
        return { l, t, r: l + card.offsetWidth, b: t + card.offsetHeight, cx: l + card.offsetWidth / 2 };
      };
      const next: Stroke[] = [];
      for (const hand of hands) {
        const a = box(hand.from), b = box(hand.to);
        if (a === null || b === null || hand.from === hand.to) continue;
        next.push({ key: `${hand.from}-${hand.to}-${hand.filed}`, filed: hand.filed, ...strokeOf(a, b) });
      }
      setStrokes((before) => (before.length === 0 && next.length === 0 ? before : next));
    };
    measure();
    if (typeof ResizeObserver === "undefined") return;
    const watch = new ResizeObserver(measure);
    watch.observe(at);
    return () => watch.disconnect();
  }, [room, hands, view]);
  return strokes;
}

/** One role's card, pressed to read its round's terminal — the pane's own run's — where `onOpen` is given. */
function Card({ card, run, now, silence, away, onOpen }: {
  card: RoleCard; run: Say; now: number; silence: Silence; away: readonly Hand[]; onOpen?: (run: number) => void;
}) {
  // The time is the round's, counted while it is under way: a paused or failed round has stopped at
  // a moment the pane does not know.
  const since = run.round?.startedAt ?? null;
  const ran = since !== null && underWay(card) ? clockOf(now - since) : null;
  const chip = chipOf(card, run);
  const attn = attnOf(card, silence);
  const quiet = silence.stage > 0;
  return (
    <li
      className="runroles__card"
      data-role={card.role ?? undefined}
      data-state={card.state}
      data-quiet={quiet ? silence.stage : undefined}
    >
      {/* The whole card is the press, laid over it so that what the card says stays read as text. */}
      {onOpen !== undefined && (
        <button
          type="button"
          className="runroles__open"
          aria-label={tf("auto.run.roundTerminal", { name: card.name })}
          onClick={() => onOpen(run.run)}
        />
      )}
      <span className="runroles__top">
        <Icon name={card.icon} />
        <span className="runroles__name">{card.name}</span>
        {/* A quiet terminal is not drawn as one at work: the dot would say something is happening
            that the stream does not show. */}
        {card.state === "running" && !quiet && (
          <span className="runroles__pulse" role="img" aria-label={t("auto.run.roles.working")} />
        )}
      </span>
      {run.task !== null && (
        <span className="runroles__task" title={`${run.task.ref} ${run.task.title}`}>
          {run.task.ref} {run.task.title}
        </span>
      )}
      {run.step !== "" && <span className="runroles__step">{run.step}</span>}
      {(ran !== null || chip !== null || quiet || away.length > 0) && (
        <span className="runroles__foot">
          {ran !== null && <span className="runroles__time">{tf("face.elapsed", { time: ran })}</span>}
          {chip !== null && <Chip state={card.state} attn={attn}>{chip}</Chip>}
          <QuietChip silence={silence} attn={attn} />
          {away.map((hand) => (
            <span key={`${hand.to}-${hand.filed}`} className="runroles__hand" data-to={hand.to}>
              → {hand.name}
            </span>
          ))}
        </span>
      )}
    </li>
  );
}

/** One round as a row of the list: what its card says, on one line, with no dot and no line drawn —
 *  pressed, like the card, to read its round's terminal where `onOpen` is given. */
function Row({ card, run, now, silence, away, onOpen }: {
  card: RoleCard; run: Say; now: number; silence: Silence; away: readonly Hand[]; onOpen?: (run: number) => void;
}) {
  const since = run.round?.startedAt ?? null;
  const ran = since !== null && underWay(card) ? clockOf(now - since) : null;
  const chip = chipOf(card, run);
  const attn = attnOf(card, silence);
  return (
    <li
      className="runroles__row"
      data-role={card.role ?? undefined}
      data-state={card.state}
      data-quiet={silence.stage > 0 ? silence.stage : undefined}
    >
      {onOpen !== undefined && (
        <button
          type="button"
          className="runroles__open"
          aria-label={tf("auto.run.roundTerminal", { name: card.name })}
          onClick={() => onOpen(run.run)}
        />
      )}
      <Icon name={card.icon} />
      <span className="runroles__rowname">
        <span className="runroles__name">{card.name}</span>
        {away.map((hand) => (
          <span key={`${hand.to}-${hand.filed}`} className="runroles__to">
            → {hand.name}
          </span>
        ))}
      </span>
      <span className="runroles__rowbody">
        {run.task !== null && (
          <span className="runroles__task" title={`${run.task.ref} ${run.task.title}`}>
            {run.task.ref} {run.task.title}
          </span>
        )}
        {run.step !== "" && <span className="runroles__step">{run.step}</span>}
      </span>
      <span className="runroles__rowend">
        {chip !== null && <Chip state={card.state} attn={attn}>{chip}</Chip>}
        <QuietChip silence={silence} attn={attn} />
        {away.map((hand) => (
          <span key={`${hand.to}-${hand.filed}`} className="runroles__hand" data-to={hand.to}>
            → {hand.name}
          </span>
        ))}
        {ran !== null && <span className="runroles__time">{tf("face.elapsed", { time: ran })}</span>}
      </span>
    </li>
  );
}

/**
 * Drawn again only when what it reads of the run changes: the run's words are built afresh on each
 * draw of the workspace, and drawing every pane's cards again on each of those would slow the typing
 * in the panes around it.
 *
 * `lastOutput` reads when the run's terminal last printed (`../talk/plate`). It is read at each tick
 * rather than handed in as a value, because the terminal prints far more often than the cards are
 * drawn.
 */
export const RunRoles = memo(
  function RunRoles({ run, project, launch = null, pin = "auto", onOpen, lastOutput }: {
    run: Say;
    project: number;
    /** The launch the pane draws, whose seats are listed under the room; null for a run kept from
     *  before launches were, which sits in no seat. */
    launch?: number | null;
    pin?: ViewPin;
    /** Turn the pane to the terminal of the run `run` — its own, or another seat's of its launch. */
    onOpen?: (run: number) => void;
    lastOutput?: () => number | null;
  }) {
    const roles = useProjectRoles(project);
    const launchState = useLaunchState(launch);
    const seats = useMemo(() => seatsOf(launchState), [launchState]);
    const { cards, idle } = useMemo(() => roleCards(run, roles), [run, roles]);
    const filed = useMemo(() => filedLine(cards, run, roles), [cards, run, roles]);
    const hands = useMemo(() => handsOf(cards, run, roles), [cards, run, roles]);
    const room = useRef<HTMLDivElement>(null);
    const fit = useFit(room, cards.length);
    const view = pin === "auto" ? fit : pin;
    const desk = useRef<HTMLDivElement>(null);
    const strokes = useStrokes(desk, hands, view);
    const unseen = useUnseen(room, view);
    const jump = (up: boolean) => {
      if (room.current !== null) scrollToUnseen(room.current, up);
    };
    // The clock ticks while a round is under way: for its time, and for how long its terminal has
    // been quiet, which is measured whether or not the round's start was read.
    const now = useNow(cards.some(underWay) || seats.some((role) => role.runs.some((one) => {
      const state = seatStateOf(one, run);
      return state === "running" || state === "asks";
    })));
    const last = lastOutput?.() ?? null;
    const expected = run.expectedMinutes ?? null;
    return (
      <div className="runroles" data-view={view} aria-label={t("auto.run.faceRoles")} role="region">
        <div className="runroles__view">
          <div className="runroles__room" ref={room}>
            {cards.length === 0 ? (
              <p className="runroles__none">{t("auto.run.roles.none")}</p>
            ) : view === "list" ? (
              <ul className="runroles__rows">
                {cards.map((card) => (
                  <Row
                    key={card.role ?? "run"}
                    card={card}
                    run={run}
                    now={now}
                    silence={silenceOf(card, last, now, expected)}
                    away={handsAway(card.role, hands, [])}
                    onOpen={onOpen}
                  />
                ))}
              </ul>
            ) : (
              <div className="runroles__desk" ref={desk}>
                <ul className="runroles__cards">
                  {cards.map((card) => (
                    <Card
                      key={card.role ?? "run"}
                      card={card}
                      run={run}
                      now={now}
                      silence={silenceOf(card, last, now, expected)}
                      away={handsAway(card.role, hands, cards)}
                      onOpen={onOpen}
                    />
                  ))}
                </ul>
                {strokes.length > 0 && (
                  <svg className="runroles__strokes" aria-hidden="true">
                    {strokes.map((stroke) => (
                      <g key={stroke.key} data-filed={stroke.filed || undefined}>
                        <path d={stroke.path} />
                        <path d={stroke.head} />
                      </g>
                    ))}
                  </svg>
                )}
              </div>
            )}
          </div>
          <UnseenNotice attns={unseen.up} up onPress={() => jump(true)} />
          <UnseenNotice attns={unseen.down} up={false} onPress={() => jump(false)} />
        </div>
        {onOpen !== undefined && seats.length > 0 && (
          <LaunchSeats seats={seats} front={run} now={now} onOpen={onOpen} />
        )}
        {idle.length > 0 && (
          <div className="runroles__idle">
            <span className="runroles__idlecount">{tf("auto.run.roles.idle", { n: idle.length })}</span>
            <ul className="runroles__idlelist">
              {idle.map((role) => (
                <li key={role.id} data-role={role.id} title={role.name}>
                  <Icon name={roleIcon(role)} label={tf("auto.run.roles.idleOne", { name: role.name })} />
                </li>
              ))}
            </ul>
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
    before.pin === after.pin &&
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

/** How the run in a seat stands, as its row says it. */
export type SeatState =
  /** Under way. */
  | "running"
  /** Under way on a step that may wait for a person — said of the seat whose terminal is the pane's
   *  now, the only one whose step the pane reads (`Say.interactive`). */
  | "asks"
  /** Before a built-in that waits for something, or an action that takes one run at a time. */
  | "waiting"
  /** Held by a pause. */
  | "paused"
  /** The run failed. */
  | "failed";

/** One role of a launch with runs in its seats, as the roles face lists them. */
export type RoleSeats = {
  readonly role: number;
  readonly name: string;
  readonly icon: IconName;
  /** The runs in its seats, oldest first: seat 1 is the first of them. */
  readonly runs: readonly AutomationRunCardDto[];
  /** How many of its seats have no run in them. */
  readonly free: number;
};

/**
 * **The roles of the launch `launch` that have someone in a seat**, in the order the roles are
 * defined in. A role with no run in a seat is not listed: it has nothing to press.
 */
export function seatsOf(launch: AutomationLaunchStateDto | null): RoleSeats[] {
  if (launch === null) return [];
  return launch.roles
    .filter((role) => role.runs.length > 0)
    .map((role) => ({
      role: role.role,
      name: role.name,
      icon: iconOf(role.icon),
      runs: role.runs,
      free: Math.max(0, role.headcount - role.runs.length),
    }));
}

/** **Which seat the run `run` sits in** in the launch `launch` — its role and its number from 1 — or
 *  null where it sits in none of them. */
export function seatOf(launch: AutomationLaunchStateDto | null, run: number): { role: string; n: number } | null {
  for (const role of launch?.roles ?? []) {
    const at = role.runs.findIndex((one) => one.run === run);
    if (at >= 0) return { role: role.name, n: at + 1 };
  }
  return null;
}

/** How the run in a seat stands: off its row, and off the pane's own run where it is that one. */
export function seatStateOf(seat: AutomationRunCardDto, front: Say): SeatState {
  if (seat.status === "failed") return "failed";
  if (seat.status === "paused") return "paused";
  if (seat.waiting || seat.waitsForRoom) return "waiting";
  return seat.run === front.run && front.interactive ? "asks" : "running";
}

/** When the round in a seat began, in milliseconds since the epoch, or null where nothing is written. */
function seatSince(seat: AutomationRunCardDto): number | null {
  const at = Date.parse(seat.roleStartedAt ?? seat.startedAt ?? "");
  return Number.isNaN(at) ? null : at;
}

/** One seat of a role, pressed to turn the pane to its run's terminal. */
function SeatRow({ role, n, seat, state, front, now, onOpen }: {
  role: string; n: number; seat: AutomationRunCardDto; state: SeatState; front: boolean; now: number;
  onOpen: (run: number) => void;
}) {
  const task = seat.task === undefined ? null : `${seat.task.ref} ${seat.task.title}`;
  const since = seatSince(seat);
  const going = state === "running" || state === "asks";
  const ran = since !== null && going ? clockOf(now - since) : null;
  const chip = state === "asks" ? t("auto.run.roles.asks") : state === "failed" ? t("auto.run.roles.failed") : null;
  return (
    <li>
      <button
        type="button"
        className="runseat"
        data-run={seat.run}
        data-state={state}
        aria-current={front ? "true" : undefined}
        aria-label={tf("auto.run.seatOpen", { role, n, task: task ?? seat.stepName ?? "" })}
        onClick={() => onOpen(seat.run)}
      >
        <span className="runseat__n">{n}</span>
        <span className="runseat__body">
          {task !== null && <span className="runseat__task" title={task}>{task}</span>}
          {seat.stepName !== undefined && <span className="runseat__step">{seat.stepName}</span>}
        </span>
        <span className="runseat__end">
          {chip !== null && <span className="runroles__chip" data-state={state}>{chip}</span>}
          {ran !== null && <span className="runroles__time">{tf("face.elapsed", { time: ran })}</span>}
          {state === "running" && (
            <span className="runroles__pulse" role="img" aria-label={t("auto.run.roles.working")} />
          )}
        </span>
      </button>
    </li>
  );
}

/**
 * **The seats of a launch, by role** — each a row pressed to turn the pane to that seat's terminal,
 * and the seats nobody is in as one row that cannot be pressed.
 */
function LaunchSeats({ seats, front, now, onOpen }: {
  seats: readonly RoleSeats[]; front: Say; now: number; onOpen: (run: number) => void;
}) {
  return (
    <div className="runseats">
      {seats.map((role) => (
        <section key={role.role} className="runseats__role" data-role={role.role}>
          <h3 className="runseats__head">
            <Icon name={role.icon} />
            <span className="runseats__name">{role.name}</span>
          </h3>
          <ul className="runseats__list">
            {role.runs.map((seat, at) => (
              <SeatRow
                key={seat.run}
                role={role.name}
                n={at + 1}
                seat={seat}
                state={seatStateOf(seat, front)}
                front={seat.run === front.run}
                now={now}
                onOpen={onOpen}
              />
            ))}
            {role.free > 0 && (
              <li>
                <div className="runseat runseat--free">
                  <span className="runseat__n">{role.free}</span>
                  <span className="runseat__body">
                    <span className="runseat__task">{tf("auto.run.seatsFree", { n: role.free })}</span>
                  </span>
                </div>
              </li>
            )}
          </ul>
        </section>
      ))}
    </div>
  );
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
  // The time is the round's, counted while it is under way, as on its card.
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
        <Icon name={role === undefined ? "robot" : roleIcon(role)} />
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
 * off the round's incident alone, not off the role cards: a round of an automation that gives it no
 * role has no card, and is filed all the same.
 */
function filedInTerminal(run: Say, roles: readonly TaskRoleDto[]): string | null {
  const incident = run.round?.incident;
  if (incident === undefined) return null;
  const to = incident.role === null ? undefined : roles.find((role) => role.id === incident.role);
  if (to !== undefined) return tf("auto.run.term.filedRole", { to: to.name });
  if (incident.person) return t("auto.run.term.filedPerson");
  return t("auto.run.term.filed");
}
