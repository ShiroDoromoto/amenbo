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
  /** What a row hands its `pointerdown`, with the paths the press is about. */
  press: (wholes: string[], event: RowPress<HTMLElement>) => void;
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

  const press = useCallback((wholes: string[], event: RowPress<HTMLElement>) => {
    // The main button only. A right press on a row is its menu, and taking it would put the row in
    // hand with no gesture to put it down.
    if (event.button !== 0 || held.current !== null) return;
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

    const stop = () => {
      held.current = null;
      if (frame !== 0) cancelAnimationFrame(frame);
      ghost?.node.remove();
      document.body.classList.remove("is-dragging");
      row.removeEventListener("pointermove", move);
      row.removeEventListener("pointerup", up);
      row.removeEventListener("pointercancel", cancel);
      window.removeEventListener("contextmenu", noMenu, true);
      if (row.hasPointerCapture(pointerId)) row.releasePointerCapture(pointerId);
      setOverFrame(null);
    };

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
    };

    const look = () => {
      frame = 0;
      refresh();
    };

    const move = (e: PointerEvent) => {
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
      const dragged = ghost !== null;
      const to = { x: e.clientX, y: e.clientY };
      stop();
      if (!dragged) return;
      window.addEventListener("click", noClick, { capture: true, once: true });
      const over = paneUnder(to.x, to.y);
      if (over !== null && can.current(over)) land.current(over, wholes);
    };

    const cancel = () => stop();

    row.addEventListener("pointermove", move);
    row.addEventListener("pointerup", up);
    row.addEventListener("pointercancel", cancel);
    window.addEventListener("contextmenu", noMenu, true);
    held.current = { stop };
  }, []);

  return { overFrame, press };
}
