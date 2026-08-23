// The one line above a pane that says what is going on in it.
//
// There is room for four things and space for one, so the row is three places with a rank in each:
// what the pane is called, what its session is on, and the one thing worth saying about it. Without a
// rank a reader cannot predict what they are looking at — which of four notices won today's draw — and
// a label nobody can predict is one nobody reads.
//
// **What is shown is derived or declared, never guessed** (`AMB-D-748`). The reservations come off the
// ledger, where every write from inside a pane carries its session's id; a broken premise is
// `in_progress` and not ready, which means the same thing whoever is running the project; and the rest
// is what the agent said in so many words. Silence is left as silence — a pane that says nothing shows
// nothing, rather than being read for signs.
//
// **⏸ is the agent's turn-taking and nothing else.** A task that is `blocked` gets a mark saying it has
// stopped, because that is a fact the ledger holds — but never the ⏸, whose whole meaning is that a
// person's turn has come. `blocked` means different things in different projects (`AMB-D-748`), and a
// pane cannot decide which of them was meant.

import type { TaskCardDto } from "../bindings/bindings";
import { statusLabel, t, tf, type Lang } from "../core/i18n";
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
  /** Nothing to say. Not "nothing is happening" — only that nothing was said. */
  | { readonly kind: "silent" };

/** The whole row. */
export type Plate = {
  readonly name: string | null;
  readonly now: Now;
  readonly say: Say;
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
 * The words the middle is drawn with, the mark in front of them, and what a reader gets on asking.
 *
 * The mark for a stopped task is not the ⏸: it says the task has stopped, which the ledger holds, and
 * says nothing about whose turn it is, which it does not.
 */
export function nowText(now: Now, lang: Lang): { mark: string; text: string; title: string } {
  const stopped = (yes: boolean) => (yes ? "⏹" : "");
  switch (now.kind) {
    case "idle":
      return { mark: "", text: t("talk.idle", lang), title: "" };
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
      return { mark: "", text: tf("talk.finished", { n: now.count }, lang), title: "" };
  }
}

/** The words the right is drawn with, and the mark in front of them. */
export function sayText(say: Say, lang: Lang): { mark: string; text: string } {
  switch (say.kind) {
    case "waiting":
      return { mark: "⏸", text: say.text };
    case "premise":
      return { mark: "⚠", text: t("talk.premiseBroken", lang) };
    case "note":
      return { mark: "", text: say.text };
    case "silent":
      return { mark: "", text: "" };
  }
}

/**
 * Draw the row into `host`, and hand back the way to draw it again.
 *
 * The elements are made once and only their words change, so a redraw on every store write costs
 * nothing and nothing under the pointer moves out from under it.
 *
 * The language is handed in rather than asked for: the talk window loads no snapshot — the store is
 * what a startup migration holds shut, and a window with a terminal in it has no reason to wait on one
 * — so `currentLang` here would answer with a guess from the browser instead of the reader's choice.
 */
export function mountNameplate(host: HTMLElement): (plate: Plate, lang: Lang) => void {
  const row = document.createElement("div");
  row.className = "plate";
  const part = (name: string) => {
    const el = document.createElement("span");
    el.className = `plate__${name}`;
    row.append(el);
    return el;
  };
  const name = part("name");
  const nowMark = part("mark");
  const now = part("now");
  const sayMark = part("mark");
  const say = part("say");
  host.append(row);

  return (plate: Plate, lang: Lang) => {
    name.textContent = plate.name ?? "";
    const middle = nowText(plate.now, lang);
    nowMark.textContent = middle.mark;
    nowMark.title = middle.title;
    now.textContent = middle.text;
    now.title = middle.title;
    const right = sayText(plate.say, lang);
    sayMark.textContent = right.mark;
    say.textContent = right.text;
    // A turn that has been handed over is the one thing on this row a person is meant to act on, so it
    // is the one thing drawn as more than grey text.
    row.dataset.say = plate.say.kind;
  };
}
