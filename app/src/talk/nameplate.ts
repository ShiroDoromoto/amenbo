// The one line above a pane that says what is going on in it.
//
// There is room for four things and space for one, so the row is three places with a rank in each:
// what the pane is called, what its session is on, and the one thing worth saying about it. Without a
// rank a reader cannot predict what they are looking at — which of four notices won today's draw — and
// a label nobody can predict is one nobody reads.
//
// **The three places are ranked on the way out as well as on the way in.** A pane too narrow for all
// of them drops them from the right, and what is left is the name and the mark saying a person is
// needed (`../styles/global.css`) — the reason being unreadable does not stop "your turn" from being
// read. Whatever went is one hover away. What keeps that a last resort is the other end: a reason
// longer than a label is refused where it is said (`amenbo_core::session::WAITING_LIMIT`).
//
// In front of the three is the lamp the pane is known by (`./moving`). It is not a fourth place and
// takes no words, and it has three faces: **lit** while output is arriving, **blinking** while a
// person's turn is standing, and **out** the rest of the time. All three are read at a glance and none
// can push the others off the row, which is why they can share a mark this small when three sentences
// cannot share a line.
//
// **The one face that moves is the rare one.** Movement given to the commonest state would leave every
// pane on the screen going all day, and a mark that is always moving is one nobody can look away from
// or read anything into. A turn standing is rare, and when it happens somebody really is being called
// — so that is where the movement goes, and being lit is a glow held still.
//
// **What is shown is derived or declared, never guessed** (`AMB-D-748`). The reservations come off the
// ledger, where every write from inside a pane carries its session's id; a broken premise is
// `in_progress` and not ready, which means the same thing whoever is running the project; and the rest
// is what the agent said in so many words. Silence is left as silence — a pane that says nothing shows
// nothing, rather than being read for signs.
//
// **The pause is the agent's turn-taking and nothing else.** A task that is `blocked` gets a mark
// saying it has stopped, because that is a fact the ledger holds — but never the pause, whose whole
// meaning is that a person's turn has come. `blocked` means different things in different projects
// (`AMB-D-748`), and a pane cannot decide which of them was meant.
//
// The marks are the application's own icons and not characters (`AMB-D-686`): a glyph is drawn at
// whatever size and weight the machine's fonts happen to give it, and this row is where a mark has
// least room to be wrong about either.

import type { TaskCardDto } from "../bindings/bindings";
import { iconSvg, type DrawnIcon } from "../components/Icon";
import { statusLabel, t, tf, type Lang } from "../core/i18n";
import { BLINK_MS, hueOf, phaseDelay } from "./moving";
import type { Session } from "./sessions";

/** A task the session is holding, as much of it as the label needs. */
export type Held = Pick<TaskCardDto, "ref" | "title" | "status" | "ready">;

/** The middle of the row: what this session is on. */
export type Now =
  /** Nothing reserved. The pane is being talked to rather than working through the backlog. */
  | { readonly kind: "idle" }
  /** One reservation, named. `stopped` is a task that has been moved to `blocked`. */
  | { readonly kind: "one"; readonly ref: string; readonly title: string; readonly stopped: boolean }
  /** Several. The count is what is shown; the refs are what a reader gets on asking. */
  | { readonly kind: "many"; readonly count: number; readonly refs: readonly string[]; readonly stopped: number }
  /** Just ended some work. Shown for a moment at the changeover, then it goes back to idle. */
  | { readonly kind: "finished"; readonly count: number };

/** The right of the row: the one thing worth saying, in rank order. */
export type Say =
  /** A person's turn has come, and why. The agent's own words. */
  | { readonly kind: "waiting"; readonly text: string }
  /** Something it is holding is no longer ready — a blocker opened, or a premise came unsettled. */
  | { readonly kind: "premise" }
  /** What the agent last said it was doing. */
  | { readonly kind: "note"; readonly text: string }
  /** Nothing has been said and nothing has come out for a while — how long, in whole minutes. It is
   *  last because it is what fills the slot when there is nothing better in it: anything the session
   *  actually said outranks a measurement of its silence (`./moving`). */
  | { readonly kind: "quiet"; readonly minutes: number }
  /** Nothing to say. Not "nothing is happening" — only that nothing was said. */
  | { readonly kind: "silent" };

/** Which of the lamp's three faces it is showing. */
export type Face =
  /** Output is arriving. A glow, held still, in the pane's own hue. */
  | "lit"
  /** A person's turn is standing here. The warning colour, at the same beat and the same faintness as
   *  the mark at the other end of the row — the two are one signal drawn twice (`./moving`). */
  | "calling"
  /** Neither. **Out is not away**: the lamp sinks in place rather than going, because a mark that
   *  vanished would read as the pane having gone. Nothing is read into it (`AMB-D-748`) — a pane that
   *  is printing nothing may be building, thinking, or waiting on somebody who has not been told. */
  | "out";

/** The lamp in front of the name: which pane this is, and which face it is on (`./moving`). */
export type Dot = {
  /** The frame the row belongs to. Its hue is what tells one pane from another — on every face but
   *  the calling one, which leaves the hue for the colour that says come here. */
  readonly frame: string;
  /** Which of the three it is showing. */
  readonly face: Face;
};

/**
 * Which face the lamp is on.
 *
 * **A turn outranks the stream.** The two are not exclusive — a blocker can open on a task a pane is
 * holding while its build prints away — and when both are true the lamp says the one a person is meant
 * to act on. It is the same rank the row itself reads by, and it is worked out from the same answer,
 * so the lamp and the words beside it can never come to disagree.
 */
export function faceOf(say: Say, moving: boolean): Face {
  if (standsAsTurn(say)) return "calling";
  return moving ? "lit" : "out";
}

/** The whole row. */
export type Plate = {
  readonly name: string | null;
  readonly now: Now;
  readonly say: Say;
  readonly dot: Dot;
};

/**
 * How long "just finished" stays up after the last reservation ends.
 *
 * Short enough that a pane cannot go on reporting an ending long after it, and long enough that
 * someone who looked away as it happened still catches it on looking back. It has nothing to bridge
 * beyond that: the gap between ending one task and reserving the next is a session's own working time,
 * which runs to minutes, and a label that claimed to be busy through it would be lying for most of it.
 */
export const FINISHED_HOLD_MS = 4000;

/** What the middle has to remember between reads: the ending it is showing, and when it began. */
export type Changeover = {
  /** What the tally stood at last time it was read. `null` before the first read — which is not the
   *  same as zero, and the difference is what keeps a pane from opening with an announcement. */
  readonly finished: number | null;
  /** When the ending now being shown went up, or `null` when none is. */
  readonly shownAt: number | null;
};

export const NO_CHANGEOVER: Changeover = { finished: null, shownAt: null };

/**
 * The middle of the row, and what to remember for the next read.
 *
 * `finished` is what the ledger says this session has ended — a tally, not an event — so an ending is
 * noticed by the tally going up. That is the whole of the changeover: it is put up then, and taken down
 * again once FINISHED_HOLD_MS has passed or the session reserves something new.
 */
export function nowOf(
  held: readonly Held[],
  finished: number,
  was: Changeover,
  at: number,
): { now: Now; changeover: Changeover } {
  // The first read is not an ending happening now, whatever the tally says: there is nothing to have
  // been looked away from, and a pane would open by announcing work that was over before it existed.
  const changeover: Changeover =
    was.finished === null
      ? { finished, shownAt: null }
      : finished > was.finished
        ? { finished, shownAt: at }
        : was;

  if (held.length > 0) {
    const stopped = held.filter((one) => one.status === "blocked").length;
    const now: Now =
      held.length === 1
        ? { kind: "one", ref: held[0].ref, title: held[0].title, stopped: stopped > 0 }
        : { kind: "many", count: held.length, refs: held.map((one) => one.ref), stopped };
    // Working again: whatever ended before this is no longer the changeover.
    return { now, changeover: { finished: changeover.finished, shownAt: null } };
  }
  if (changeover.shownAt !== null && at - changeover.shownAt < FINISHED_HOLD_MS) {
    return { now: { kind: "finished", count: changeover.finished ?? 0 }, changeover };
  }
  return { now: { kind: "idle" }, changeover: { finished: changeover.finished, shownAt: null } };
}

/**
 * The right of the row: the first of four that applies.
 *
 * The order is the order a person is needed in. A turn that has been handed over is the only thing that
 * cannot wait; a premise that has broken is the pane saying so before anyone asks; a note is the agent
 * talking about its own work; and below that there is nothing, which is not a claim that all is well.
 */
export function sayOf(held: readonly Held[], session: Session | undefined): Say {
  if (session?.waiting) return { kind: "waiting", text: session.waiting };
  if (held.some((one) => !one.ready)) return { kind: "premise" };
  if (session?.note) return { kind: "note", text: session.note };
  return { kind: "silent" };
}

/**
 * Whether what the row leads with is a person's turn standing.
 *
 * The two that are: the agent handing one over, and the ledger saying something the pane is holding
 * is no longer ready. The two that are not: what the agent last said it was doing, and silence —
 * which is not a claim about anything (`AMB-D-748`). It is one line and it is here rather than at
 * the two places that draw it, so the dot on a page and the badge on the face switch cannot come to
 * mean something the row does not (`AMB-T-3610`).
 */
export function standsAsTurn(say: Say): boolean {
  return say.kind === "waiting" || say.kind === "premise";
}

/**
 * The words the middle is drawn with, the mark in front of them, and what a reader gets on asking.
 *
 * The mark for a stopped task is not the pause: it says the task has stopped, which the ledger holds,
 * and says nothing about whose turn it is, which it does not.
 */
export function nowText(now: Now, lang: Lang): { mark: Mark; text: string; title: string } {
  const stopped = (yes: boolean) => (yes ? "stop" : null) as Mark;
  switch (now.kind) {
    case "idle":
      return { mark: null, text: t("talk.idle", lang), title: "" };
    case "one":
      return {
        mark: stopped(now.stopped),
        text: `${now.ref} ${now.title}`,
        title: now.stopped ? statusLabel("blocked", lang) : "",
      };
    case "many":
      // The breakdown is not stacked on the pane: the row is one line, and a list of refs would push
      // out the thing it is there to say. It is one hover away instead.
      return {
        mark: stopped(now.stopped > 0),
        text: tf("talk.holding", { n: now.count }, lang),
        title: now.refs.join("\n"),
      };
    case "finished":
      return { mark: null, text: tf("talk.finished", { n: now.count }, lang), title: "" };
  }
}

/**
 * The words the right is drawn with, the mark in front of them, and what a reader gets on asking.
 *
 * The row is one line and gives this place what is left of it, so what is said here is elided where
 * the pane is narrow and dropped altogether where it is narrower still — the mark stays either way,
 * because "a person is needed here" survives the reason being unreadable (`AMB-T-3673`). The whole of
 * it is one hover away instead, the same way the breakdown of several reservations is.
 */
export function sayText(say: Say, lang: Lang): { mark: Mark; text: string; title: string } {
  switch (say.kind) {
    case "waiting":
      return { mark: "pause", text: say.text, title: say.text };
    case "premise":
      return { mark: "warning", text: t("talk.premiseBroken", lang), title: "" };
    case "note":
      return { mark: null, text: say.text, title: say.text };
    case "quiet":
      return { mark: null, text: tf("talk.quiet", { n: say.minutes }, lang), title: "" };
    case "silent":
      return { mark: null, text: "", title: "" };
  }
}

/** Which mark stands in front of a part of the row, or nothing where the part asks for none. */
export type Mark = DrawnIcon | null;

/**
 * Draw the row into `host`, and hand back the way to draw it again.
 *
 * The elements are made once and only their words change, so a redraw on every store write costs
 * nothing and nothing under the pointer moves out from under it.
 *
 * **Handing over nothing takes the row down.** A label is about a session, and a pane that has never
 * had one has nothing to be labelled — the face there is the invitation to choose a folder
 * (`AMB-T-3606`), and a row saying the session is idle would be saying it of a session that does not
 * exist. The row is hidden rather than removed for the same reason it is redrawn rather than rebuilt.
 *
 * The language is handed in rather than asked for: the talk window loads no snapshot — the store is
 * what a startup migration holds shut, and a window with a terminal in it has no reason to wait on one
 * — so `currentLang` here would answer with a guess from the browser instead of the reader's choice.
 */
export function mountNameplate(host: HTMLElement): (plate: Plate | null, lang: Lang) => void {
  const row = document.createElement("div");
  row.className = "plate";
  const part = (name: string, of?: string) => {
    const el = document.createElement("span");
    el.className = of === undefined ? `plate__${name}` : `plate__${name} plate__${name}--${of}`;
    row.append(el);
    return el;
  };
  // The dot goes in first, so it is to the left of the name: it is what the row belongs to, and the
  // row reads from what it is towards what is happening in it.
  const dot = part("dot");
  dot.setAttribute("aria-hidden", "true");
  // The turn's length, handed to the stylesheet rather than written there: it is what the phase is
  // measured against, and the two have to be the same number or the panes beat out of step. It goes on
  // the row rather than on the lamp because the mark at the other end blinks to it too.
  row.style.setProperty("--blink", `${BLINK_MS}ms`);
  const name = part("name");
  // The two marks are told apart in the markup because the row drops its places one at a time as the
  // pane narrows, and a mark goes with the words it belongs to (`../styles/global.css`).
  const nowMark = part("mark", "now");
  const now = part("now");
  const sayMark = part("mark", "say");
  const say = part("say");
  host.append(row);

  // Which face the lamp was on last time. Setting the phase again on a row that is already blinking
  // would start the turn over, which is the one thing the shared phase exists to prevent — so it is
  // written only where the answer has changed, which is also the only moment it can be out of step.
  let face: Face | null = null;

  return (plate: Plate | null, lang: Lang) => {
    row.hidden = plate === null;
    if (plate === null) return;
    dot.style.setProperty("--dot-hue", String(hueOf(plate.dot.frame)));
    if (plate.dot.face !== face) {
      face = plate.dot.face;
      dot.dataset.face = face;
      // Joining where every other blinking mark already is, rather than starting where this one was
      // noticed. The lamp and the mark on the right both read it off the row (`./moving`).
      if (face === "calling") row.style.setProperty("--phase", phaseDelay(Date.now()));
    }
    name.textContent = plate.name ?? "";
    const middle = nowText(plate.now, lang);
    drawMark(nowMark, middle.mark);
    nowMark.title = middle.title;
    now.textContent = middle.text;
    now.title = middle.title;
    const right = sayText(plate.say, lang);
    drawMark(sayMark, right.mark);
    // The mark carries the whole of it too: where the row is narrow the words beside it are not drawn
    // at all, and a hover has to have something left to land on.
    sayMark.title = right.title;
    say.textContent = right.text;
    say.title = right.title;
    // A turn that has been handed over is the one thing on this row a person is meant to act on, so it
    // is the one thing drawn as more than grey text.
    row.dataset.say = plate.say.kind;
  };
}

/**
 * Put a mark in its place, or take the place away.
 *
 * The element is left **empty** where there is no mark, rather than holding a hidden one: the
 * stylesheet folds an empty mark out of the row with `:empty`, so a box kept there with nothing drawn
 * in it would leave a gap in front of words that have no mark (`../styles/global.css`).
 *
 * What is there already is read off `data-icon` rather than remembered, so nothing has to be kept in
 * step with what was drawn last time — and the common redraw, where the mark has not changed, touches
 * no elements at all.
 */
function drawMark(host: HTMLElement, mark: Mark): void {
  const drawn = host.firstElementChild?.getAttribute("data-icon") ?? null;
  if (drawn === mark) return;
  host.replaceChildren();
  if (mark !== null) host.append(iconSvg(mark));
}
