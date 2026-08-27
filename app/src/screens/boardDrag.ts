// Dragging a card from one column to another, made of pointer events rather than of HTML5 drag
// (`AMB-D-775`).
//
// **The board did not ask for this.** A file dragged in from the desktop now lands on the application
// rather than on the page (`../core/hostDrop`), and the OS drag handler that makes that possible is
// the same one that swallows HTML5 drag inside the webview on macOS and Windows. So the gesture is
// rebuilt out of the events nothing swallows: `pointerdown`, `pointermove` and `setPointerCapture`,
// which were measured to hold on all three operating systems while the handler is on (`AMB-T-3755`).
//
// Four things the browser used to do for free, and where each of them went:
//
// | | |
// |---|---|
// | telling a press from a drag | a threshold, `DRAG_SLOP` — without one, a press meant as a drag navigates |
// | what is under the pointer | {@link columnUnder}, asked afresh on every frame — a cached rect is 356px wrong once a column scrolls under the card (`AMB-T-3755`) |
// | the half-transparent card that follows | a clone of the card's own node, so what follows the pointer is the card rather than a drawing of one |
// | not selecting text, not opening a menu | 🚨 both are put back by hand below, and neither is optional |
//
// A fifth thing the browser never did: scrolling the board while a card is held against its edge.
// A column runs past the window and the board runs wider than it, so without it a card cannot be
// carried anywhere it cannot already see (`../core/edgeScroll`).
//
// **The two fences are the whole of what makes this usable.** Without the context menu one, a right
// click during a drag freezes the gesture on all three — macOS delivers no pointer event at all until
// the menu is dismissed, which reads as "it sometimes sticks". Without `user-select: none`, dragging
// down a column selects the text it passes over on macOS and Linux (`AMB-T-3755`).
import { useCallback, useEffect, useRef, useState } from "react";
import type { PointerEvent as CardPress } from "react";

import { flowEdges } from "../core/edgeScroll";
import { draggedFar, elementUnder } from "../core/pointerDrag";

/** The attribute a column answers a drop on, holding the key that says which column it is. */
export const DROP_ATTR = "data-drop-column";

/**
 * The column under a point, or none.
 *
 * The hit test is the one every pointer drag here shares (`../core/pointerDrag`); what belongs to
 * the board is which attribute names a column and that the answer is the key written on it rather
 * than the element carrying it — the gesture crosses two boards, and a string is what both spell.
 */
export function columnUnder(x: number, y: number): string | null {
  return elementUnder({ x, y }, DROP_ATTR)?.getAttribute(DROP_ATTR) ?? null;
}

/**
 * Take a column key apart into which board drew it and which of that board's columns it is.
 *
 * The two boards a card can be dragged on name their columns from different things — a status is a
 * word, a dimension's column is a value's id, and one column stands for the cards that carry no value
 * at all. One key spells all three, so the gesture carries a string and the board reads it back.
 */
export function splitColumn(key: string): [board: string, which: string] {
  const cut = key.indexOf(":");
  return cut < 0 ? [key, ""] : [key.slice(0, cut), key.slice(cut + 1)];
}

/** What a card being dragged looks like: the card's own node, following the pointer. */
interface Ghost {
  node: HTMLElement;
  /** Where in the card the pointer took hold, so the card does not jump under it. */
  grabX: number;
  grabY: number;
}

function raise(card: HTMLElement, at: { x: number; y: number }): Ghost {
  const box = card.getBoundingClientRect();
  const node = card.cloneNode(true) as HTMLElement;
  node.classList.add("card--ghost");
  node.style.width = `${box.width}px`;
  node.style.height = `${box.height}px`;
  document.body.append(node);
  const ghost = { node, grabX: at.x - box.left, grabY: at.y - box.top };
  place(ghost, at);
  return ghost;
}

function place(ghost: Ghost, at: { x: number; y: number }): void {
  ghost.node.style.transform = `translate(${at.x - ghost.grabX}px, ${at.y - ghost.grabY}px)`;
}

/**
 * The board's side of a card being moved: what is being dragged, what it is over, and the one handler
 * a card puts on its `pointerdown`.
 *
 * `onDrop` is given the key of the column the card was let go over, and is not called at all where
 * that is the place the card was already in — which the card says for itself, because on the status
 * board the column a card is drawn in is not always its own place (`./BoardScreen`). Which column a
 * card is over belongs to the board, not to a card and not to a column: the gesture crosses both.
 */
export function useCardDrag(onDrop: (column: string, id: number) => void): {
  /** The card being dragged, which is also what the move flourish reads to leave it alone (`./boardFlip`). */
  draggingId: number | null;
  /** The column the pointer is over while a card is held, drawn as the column that would take it. */
  overColumn: string | null;
  /** What a draggable card hands its `pointerdown` to, with the column it is being taken from. */
  press: (id: number, from: string, event: CardPress<HTMLElement>) => void;
} {
  const [draggingId, setDraggingId] = useState<number | null>(null);
  const [overColumn, setOverColumn] = useState<string | null>(null);
  // The gesture in flight. It is a ref and not state because it moves 60 to 165 times a second
  // (`AMB-T-3755`) and nothing on the screen reads most of what it holds.
  const held = useRef<{ stop: () => void } | null>(null);
  // What to do with a landed card, read at the landing rather than closed over. This is what keeps
  // `press` the same function across renders, and the cards are memoised on exactly that (`./BoardScreen`).
  const land = useRef(onDrop);
  land.current = onDrop;

  // A press outliving the board would go on listening on a window whose card is gone.
  useEffect(() => () => held.current?.stop(), []);

  const press = useCallback((id: number, from: string, event: CardPress<HTMLElement>) => {
    // The main button only. A right or middle press on a card is not a move, and taking it would put
    // the card in hand with no gesture to put it down.
    if (event.button !== 0 || held.current !== null) return;
    const card = event.currentTarget;
    const grabbedAt = { x: event.clientX, y: event.clientY };
    const pointerId = event.pointerId;
    card.setPointerCapture(pointerId);

    // 🚨 On the press, not on the threshold. The browser begins selecting the moment the pointer
    // moves, so a fence raised after the card has travelled its first few pixels arrives too late —
    // measured in the app, where the card's own words came up selected (`AMB-T-3794`). Held for a
    // plain click too, which costs nothing: a card's words were never selectable by dragging them,
    // HTML5 drag having taken the gesture before this did.
    document.body.classList.add("is-dragging");

    let ghost: Ghost | null = null;
    let at = grabbedAt;
    // One hit test to a frame. The pointer reports up to 165 times a second and `getCoalescedEvents`
    // gives nothing back to fold, so the frame is the natural clock (`AMB-T-3755`).
    let frame = 0;

    const stop = () => {
      held.current = null;
      stopFlow();
      if (frame !== 0) cancelAnimationFrame(frame);
      ghost?.node.remove();
      document.body.classList.remove("is-dragging");
      card.removeEventListener("pointermove", move);
      card.removeEventListener("pointerup", up);
      card.removeEventListener("pointercancel", cancel);
      window.removeEventListener("contextmenu", noMenu, true);
      if (card.hasPointerCapture(pointerId)) card.releasePointerCapture(pointerId);
      setDraggingId(null);
      setOverColumn(null);
    };

    // 🚨 The fence that keeps a right click from freezing the gesture. Without it the menu opens over
    // the held card and macOS stops delivering pointer events entirely until it is dismissed
    // (`AMB-T-3755`).
    const noMenu = (e: Event) => e.preventDefault();

    /**
     * The click a finished drag leaves behind.
     *
     * **Refusing it takes stopping it, not preventing it.** The card's own handler is React's, hung
     * on the tree's root rather than on the card, so a default that is merely prevented still reaches
     * it and the card opens under the reader's hand (`AMB-T-3794`).
     */
    const noClick = (e: Event) => {
      e.preventDefault();
      e.stopPropagation();
      e.stopImmediatePropagation();
    };

    // Where the ghost is drawn and what it is over — both read off `at`, which is why a board that
    // scrolled under a still hand needs this run again with nothing else having changed.
    const refresh = () => {
      if (ghost === null) return;
      place(ghost, at);
      setOverColumn(columnUnder(at.x, at.y));
    };

    const look = () => {
      frame = 0;
      refresh();
    };

    // The board flows while the card is held against one of its edges. Nothing before the threshold:
    // a press that has not travelled is a click, and a click must not scroll the board out from under
    // itself (`../core/edgeScroll`).
    const stopFlow = flowEdges(() => (ghost === null ? null : at), refresh);

    const move = (e: PointerEvent) => {
      at = { x: e.clientX, y: e.clientY };
      if (ghost === null) {
        if (!draggedFar(grabbedAt, at)) return;
        // Whatever the first few pixels managed to select before the fence was up.
        window.getSelection()?.removeAllRanges();
        ghost = raise(card, at);
        setDraggingId(id);
      }
      if (frame === 0) frame = requestAnimationFrame(look);
    };

    const up = (e: PointerEvent) => {
      const dragged = ghost !== null;
      const to = { x: e.clientX, y: e.clientY };
      stop();
      if (!dragged) return;
      // The press ends as a click as well, and a card that was carried across the board is not a card
      // somebody meant to open. Taken once, on the way up, so the next real click is untouched.
      window.addEventListener("click", noClick, { capture: true, once: true });
      // A card let go where it already was is not a move. That one comparison is the whole of the
      // guard, and it says the same thing on the status board and on a dimension's — where before
      // there were two different ways of asking whether the write would change anything.
      const column = columnUnder(to.x, to.y);
      if (column !== null && column !== from) land.current(column, id);
    };

    const cancel = () => stop();

    card.addEventListener("pointermove", move);
    card.addEventListener("pointerup", up);
    card.addEventListener("pointercancel", cancel);
    window.addEventListener("contextmenu", noMenu, true);
    held.current = { stop };
  }, []);

  return { draggingId, overColumn, press };
}
