// Carrying a row of the file panel to a pane, made of pointer events (`AMB-D-775`).
//
// **The panel is one thing beside as many panes as the page holds**, and until now nothing could be
// taken from the one to the other by hand: a path reached a pane either by being dragged in from the
// desktop, which lands wherever the pointer is (`../core/hostDrop`), or off the row's own menu, which
// lands in the pane being worked in and nowhere else (`../shell/TerminalFace`). A row in sight and a
// pane in sight, and no way to put one in the other.
//
// What is carried is paths and nothing else (`AMB-D-820`), so this is the gesture and not the
// handover: where the rows came down is all it answers, and the face does the rest.
//
// **Three things can be under the rows when they are let go**: a pane, which is handed the words; a
// folder of the tree, which takes the files themselves; and one of the git half's two lists, where
// what moves is what git has been told about them and nothing on the disk (`./GitPanel`). The first
// is the face's, the other two are the panel's own and each has its own slot below.
//
// **Paths and not a path**, because a reader can pick several rows out and carry them together
// (`AMB-T-4242`). Which rows a press is about is the panel's answer, so what arrives here is
// already the list — the gesture is the same whether it is one row or five.
//
// The two questions the webview's own drag used to answer are answered where every pointer drag on
// this screen answers them (`../core/pointerDrag`); the two fences are the ones a card needs for the
// same reasons (`../screens/boardDrag`), and the third is this gesture's own — a row is a button, and
// the click a finished drag leaves behind would open the file that was just handed over.
import { useCallback, useEffect, useRef, useState } from "react";
import type { PointerEvent as RowPress } from "react";

import { hostOs } from "../core/platform";
import { draggedFar, elementUnder } from "../core/pointerDrag";

/**
 * The attribute a pane answers a drop on, holding which frame it is.
 *
 * **The same one the host's drops are matched on** (`../shell/TerminalPane`): one pane, one mark,
 * whether what is coming down on it was dragged in from the desktop or off the panel beside it.
 */
export const HAND_ATTR = "data-hand";

/** The pane under a point, or none. */
export function paneUnder(x: number, y: number): string | null {
  return elementUnder({ x, y }, HAND_ATTR)?.getAttribute(HAND_ATTR) ?? null;
}

/**
 * The attribute a folder of the panel answers a carry on, holding the folder it is.
 *
 * **The same one a drop from the desktop and a paste land on** (`./FolderTree`): where a row goes
 * when it is let go over the panel is worked out once, by the panel, whichever of the three gestures
 * brought it there.
 */
export const INTO_ATTR = "data-into";

/** The folder of the panel under a point, or none. */
export function intoUnder(x: number, y: number): HTMLElement | null {
  return elementUnder({ x, y }, INTO_ATTR);
}

/**
 * What a press has taken hold of, said both ways round.
 *
 * The two ends of this gesture want different things of the same rows: a pane is handed whole paths
 * and moves nothing (`AMB-D-820`), and a folder of the panel is handed the rows as the project knows
 * them, because what it does is move or copy the files themselves.
 */
export interface Held {
  /** Whole paths, for the pane that is handed words rather than files. */
  wholes: string[];
  /** The bound folder these rows are in, as the panel names it. */
  root: string;
  /** Their paths inside that folder, in the order the rows were taken. */
  paths: string[][];
}

/**
 * The attribute one of the git half's two lists answers a carry on, holding which of them it is.
 *
 * **A third thing a row can be let go on, and the rail's own** (`AMB-D-906`, 2-8): a row dropped on
 * the list it is not in is staged or taken back out, which is the same act its box does. It is a
 * mark of its own rather than the folders' (`INTO_ATTR`) because what lands there is not a file
 * being moved — nothing on the disk is touched — and the two halves are never drawn at once
 * (`../shell/FolderRail`), so a reader is only ever offered one of the two meanings.
 */
export const STAGE_ATTR = "data-stage-into";

/** The list of the git half under a point, or none. */
export function stageUnder(x: number, y: number): HTMLElement | null {
  return elementUnder({ x, y }, STAGE_ATTR);
}

/**
 * The panel's side of a row let go inside the panel itself — the half of this gesture the face does
 * not own.
 *
 * **It is a subscription and not a callback down the tree** (`../core/notice`, `../core/hostDrop`),
 * because the two ends of the carry belong to two different faces: the pane's landing is the talk
 * face's, and what happens inside the panel is the panel's own. The gesture is one at a time, so one
 * watcher is all there ever is.
 */
export interface CarryWatch {
  /** The folder under the pointer while a row is held, or nothing — what draws the highlight. It is
   *  said only as the answer changes, rather than at every frame the pointer moves through. */
  over: (into: HTMLElement | null) => void;
  /** A row let go over a folder, and whether the keys held asked for a copy. */
  drop: (into: HTMLElement, held: Held, copy: boolean) => void;
}

/** The same two, for the git half's lists. There is nothing to copy there — what a drop moves is
 *  git's index and not a file — so the keys held say nothing. */
export interface StageWatch {
  over: (into: HTMLElement | null) => void;
  drop: (into: HTMLElement, held: Held) => void;
}

/** The two halves of the rail, each with its own slot: the tree watches one and the git half the
 *  other, and neither hears what was let go on the other's. */
let watcher: CarryWatch | null = null;
let stager: StageWatch | null = null;

/** Take up the panel's side of the carry, and hand back the way to put it down. */
export function watchCarry(watch: CarryWatch): () => void {
  watcher = watch;
  return () => {
    if (watcher === watch) watcher = null;
  };
}

/** The same, for the git half's lists. */
export function watchStage(watch: StageWatch): () => void {
  stager = watch;
  return () => {
    if (stager === watch) stager = null;
  };
}

/**
 * Say that a carried row is over this folder of the panel, or over none — the gesture's own side of
 * `watchCarry`, the way `pushNotice` is `subscribeNotice`'s (`../core/notice`).
 */
export function carriedOver(into: HTMLElement | null): void {
  watcher?.over(into);
}

/** And that one was let go there, with what the keys held asked for. */
export function carriedInto(into: HTMLElement, taken: Held, copy: boolean): void {
  watcher?.drop(into, taken, copy);
}

/** The same pair, said to the git half. */
export function carriedOverStage(into: HTMLElement | null): void {
  stager?.over(into);
}

export function carriedIntoStage(into: HTMLElement, taken: Held): void {
  stager?.drop(into, taken);
}

/**
 * Whether the keys held as a row was let go asked for a copy rather than a move.
 *
 * **The platform's own convention, unlevelled** (`crate::dropped`): a reader holding Option on a Mac
 * is asking for a copy in every application they own, and Amenbo is not the one to teach them
 * otherwise. What a plain carry does is the other one — both ends are the project's own folders, so
 * a move takes nothing out of a place Amenbo does not answer for, which is what makes a drop from
 * the desktop copy instead (`crate::folder_write`).
 */
function copyHeld(e: PointerEvent): boolean {
  return hostOs() === "macos" ? e.altKey : e.ctrlKey;
}

/** What a row being carried looks like: the row's own node, following the pointer. */
interface Ghost {
  node: HTMLElement;
  /** Where in the row the pointer took hold, so the row does not jump under it. */
  grabX: number;
  grabY: number;
}

function raise(row: HTMLElement, at: { x: number; y: number }): Ghost {
  const box = row.getBoundingClientRect();
  const node = row.cloneNode(true) as HTMLElement;
  node.classList.add("files__ghost");
  node.style.width = `${box.width}px`;
  document.body.append(node);
  const ghost = { node, grabX: at.x - box.left, grabY: at.y - box.top };
  place(ghost, at);
  return ghost;
}

function place(ghost: Ghost, at: { x: number; y: number }): void {
  ghost.node.style.transform = `translate(${at.x - ghost.grabX}px, ${at.y - ghost.grabY}px)`;
}

/**
 * The face's side of a row being carried to a pane: which pane it is over, and the one handler a row
 * puts on its `pointerdown`.
 *
 * **The face holds this and not the panel**, because what the gesture is about is a pane: the surface
 * that says a pane would take it is drawn by the pane, and where the path goes is a session only the
 * face knows the pane has.
 *
 * `takes` is asked afresh at every hit test rather than at the press. A pane whose program has ended
 * has nothing to hand a path to, so it neither lights up nor receives — and the one that ended while
 * a row was being carried across the page is exactly the pane a press-time answer would get wrong.
 */
export function useHandDrag(
  onLand: (frame: string, wholes: string[]) => void,
  takes: (frame: string) => boolean,
): {
  /** The pane the pointer is over while a row is held, or nothing — which is what draws the surface
   *  on that pane and on no other. */
  overFrame: string | null;
  /** What a row hands its `pointerdown`, with what the press is about. */
  press: (taken: Held, event: RowPress<HTMLElement>) => void;
} {
  const [overFrame, setOverFrame] = useState<string | null>(null);
  // The gesture in flight. A ref rather than state: it moves with the pointer, and nothing on the
  // screen reads what it holds (`../screens/boardDrag`).
  const held = useRef<{ stop: () => void } | null>(null);
  // Both read at the moment they are needed rather than closed over, which is what keeps `press` the
  // same function across renders — the panel hands it down through every row it draws.
  const land = useRef(onLand);
  land.current = onLand;
  const can = useRef(takes);
  can.current = takes;

  // A press outliving the face would go on listening for a row that is gone.
  useEffect(() => () => held.current?.stop(), []);

  const press = useCallback((taken: Held, event: RowPress<HTMLElement>) => {
    // The main button only. A right press on a row is its menu, and taking it would put the row in
    // hand with no gesture to put it down.
    if (event.button !== 0) return;
    // 🚨 A row still in hand when a new press lands is one whose ending never came: the pointer was
    // let go somewhere this page never heard of — over the menu bar a window at the top of the
    // screen reveals, or over another application. Turning the new press away, which is what this
    // did, made that permanent: the ghost stayed on the page, `is-dragging` stayed on the body, and
    // no row could be taken up again until the page was reloaded (`AMB-T-4624`). Only ever one row
    // is in hand, so a press arriving while one is held is a person starting over — put down what
    // is held and take the new one.
    held.current?.stop();
    const row = event.currentTarget;
    const grabbedAt = { x: event.clientX, y: event.clientY };
    const pointerId = event.pointerId;
    row.setPointerCapture(pointerId);
    // 🚨 On the press and not on the threshold, for the reason a card raises it there: the browser
    // begins selecting the moment the pointer moves (`../screens/boardDrag`).
    document.body.classList.add("is-dragging");

    let ghost: Ghost | null = null;
    let at = grabbedAt;
    let frame = 0;
    // The folder of the panel the pointer was last over, so the panel is told when that changes and
    // not on every frame the pointer travels through one. The git half's list beside it, for the
    // same reason.
    let overInto: HTMLElement | null = null;
    let overList: HTMLElement | null = null;

    const stop = () => {
      held.current = null;
      if (frame !== 0) cancelAnimationFrame(frame);
      ghost?.node.remove();
      document.body.classList.remove("is-dragging");
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", cancel);
      window.removeEventListener("blur", away);
      window.removeEventListener("contextmenu", noMenu, true);
      if (row.hasPointerCapture(pointerId)) row.releasePointerCapture(pointerId);
      setOverFrame(null);
      if (overInto !== null) {
        overInto = null;
        carriedOver(null);
      }
      if (overList !== null) {
        overList = null;
        carriedOverStage(null);
      }
    };

    /** Whether an event is the pointer this gesture is holding — the window hears every one. */
    const mine = (e: PointerEvent) => e.pointerId === pointerId;

    // 🚨 Without it a right click during the drag freezes the gesture on macOS, which delivers no
    // pointer event at all until the menu is dismissed (`../screens/boardDrag`).
    const noMenu = (e: Event) => e.preventDefault();

    /**
     * The click a finished drag leaves behind.
     *
     * A row opens the file it names, so without this the file just handed to a pane opens in the
     * panel over it. Stopped rather than prevented: the row's handler is React's, hung on the tree's
     * root rather than on the row, so a default merely prevented still reaches it.
     */
    const noClick = (e: Event) => {
      e.preventDefault();
      e.stopPropagation();
      e.stopImmediatePropagation();
    };

    const refresh = () => {
      if (ghost === null) return;
      place(ghost, at);
      const over = paneUnder(at.x, at.y);
      setOverFrame(over !== null && can.current(over) ? over : null);
      // The panel's own landings, said only as the answer changes: what they draw is one highlight,
      // and a frame that named the same one again would draw it a second time. Which of the two it
      // is is whichever half of the rail is drawn — they are never both (`../shell/FolderRail`).
      const into = over === null ? intoUnder(at.x, at.y) : null;
      if (into !== overInto) {
        overInto = into;
        carriedOver(into);
      }
      const onList = over === null && into === null ? stageUnder(at.x, at.y) : null;
      if (onList !== overList) {
        overList = onList;
        carriedOverStage(onList);
      }
    };

    const look = () => {
      frame = 0;
      refresh();
    };

    const move = (e: PointerEvent) => {
      if (!mine(e)) return;
      at = { x: e.clientX, y: e.clientY };
      if (ghost === null) {
        if (!draggedFar(grabbedAt, at)) return;
        // Whatever the first few pixels managed to select before the fence was up.
        window.getSelection()?.removeAllRanges();
        ghost = raise(row, at);
      }
      // One hit test to a frame. The pointer reports far more often than that, and the answer is
      // only ever drawn (`../screens/boardDrag`).
      if (frame === 0) frame = requestAnimationFrame(look);
    };

    const up = (e: PointerEvent) => {
      if (!mine(e)) return;
      const dragged = ghost !== null;
      const to = { x: e.clientX, y: e.clientY };
      stop();
      if (!dragged) return;
      window.addEventListener("click", noClick, { capture: true, once: true });
      const over = paneUnder(to.x, to.y);
      if (over !== null && can.current(over)) {
        land.current(over, taken.wholes);
        return;
      }
      // Or a folder of the panel it came from, which is the other thing this gesture can mean: there
      // the rows are the files themselves rather than words about them (`./FolderTree`).
      const into = intoUnder(to.x, to.y);
      if (into !== null) {
        carriedInto(into, taken, copyHeld(e));
        return;
      }
      // Or one of the git half's lists, where what moves is what git has been told about the rows
      // and not the rows themselves (`./GitPanel`).
      const onList = stageUnder(to.x, to.y);
      if (onList !== null) carriedIntoStage(onList, taken);
    };

    const cancel = (e: PointerEvent) => { if (mine(e)) stop(); };

    /**
     * The page losing the pointer altogether.
     *
     * 🚨 An application put in the background mid-carry is one that may never be told the button
     * came up, and a row held on a page nobody is looking at is held for ever (`AMB-T-4624`). Put
     * down rather than landed: where the pointer went is not this page's to say any more.
     */
    const away = () => stop();

    // 🚨 Hung on the window and not on the row, because **the row does not outlive the gesture**.
    // The tree draws a window of its lines at a time (`./FolderTree`), so a list that scrolls under
    // a held pointer takes the grabbed row out of the document. Listeners that went with it could
    // end the gesture only if the browser fired one last `pointercancel` at the node it had just
    // removed — WebKit does, and that unstated favour was all that held this together. Where it is
    // not done, `stop` never runs: the ghost stays on the page, `is-dragging` stays on the body,
    // and `held` stays full, which turns every later press away until the page is reloaded
    // (`AMB-T-4619`). The window is there for as long as the press is, whatever becomes of the row.
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", cancel);
    window.addEventListener("blur", away);
    window.addEventListener("contextmenu", noMenu, true);
    held.current = { stop };
  }, []);

  return { overFrame, press };
}
