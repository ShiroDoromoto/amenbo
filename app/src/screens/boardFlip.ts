// The board's move flourish — a first prototype. When an outside write (AI/CLI) shifts a card's
// status, the card slides from its old column to its new one instead of teleporting. It is a FLIP animation
// (First-Last-Invert-Play): we snapshot every card's screen rect the instant the outside write is reflected,
// let the refetch re-render place the cards at their new positions, then translate each moved card back to
// where it was and transition it to identity.
//
// Everything the effect needs lives in this file plus a `data-flip-id` attribute on the card and one hook call
// in BoardScreen — removal is deleting those three. It builds on nothing else: card positions are drawn by the
// source of truth (the board holds no optimistic state), so this only interpolates between two already-correct
// layouts. Peel it off and cards teleport again, exactly as before.
//
// It is deliberately best-effort and degrades to the current (no-animation) behaviour whenever it cannot do
// better: `prefers-reduced-motion`, a card mounted in only one of the two layouts, a card outside the viewport,
// the card being dragged (a local move, not an outside one), or too many cards moving at once.
import { useEffect, useLayoutEffect, useRef, type RefObject } from "react";
import { subscribeStoreChangeReflected } from "../core/snapshot";

// The one flag. Off ≡ removed: no snapshot is taken, no attribute is emitted, and the board behaves exactly as
// it did before this file existed.
export const BOARD_FLIP = true;

// How long a card takes to slide to its new column.
const FLIP_MS = 260;
// Ceiling on cards animated in one reflected write. A high-frequency AI burst can move many at once; past this
// we place the rest instantly rather than storm the compositor.
const FLIP_MAX_CARDS = 8;
// How long an armed snapshot waits for the refetch re-render to land before it gives up and places instantly.
const FLIP_ARM_WINDOW_MS = 1500;

export interface FlipRect {
  left: number;
  top: number;
  width: number;
  height: number;
}

export interface FlipMove {
  id: number;
  /** Screen delta from the new position back to the old one — the "Invert" the card starts at. */
  dx: number;
  dy: number;
}

function intersectsViewport(r: FlipRect, vw: number, vh: number): boolean {
  return r.left < vw && r.top < vh && r.left + r.width > 0 && r.top + r.height > 0;
}

/**
 * The pure core: given each card's rect before (`first`) and after (`last`) an outside write, decide which cards
 * to slide and by how much. A card is animated only when it is mounted in both layouts, actually moved, sits in
 * the viewport in both, and is not the one being dragged. The result is clipped to `maxCards` (in DOM order —
 * `last` preserves it) so a burst cannot animate without bound.
 */
export function planFlip(
  first: Map<number, FlipRect>,
  last: Map<number, FlipRect>,
  opts: { draggingId?: number | null; viewport: { width: number; height: number }; maxCards: number },
): FlipMove[] {
  const { draggingId, viewport, maxCards } = opts;
  const moves: FlipMove[] = [];
  for (const [id, l] of last) {
    if (id === draggingId) continue; // a local drag, not an outside move.
    const f = first.get(id);
    if (!f) continue; // not mounted before — a card entering the view, not a move within it.
    const dx = f.left - l.left;
    const dy = f.top - l.top;
    if (dx === 0 && dy === 0) continue; // did not move.
    if (!intersectsViewport(f, viewport.width, viewport.height)) continue;
    if (!intersectsViewport(l, viewport.width, viewport.height)) continue;
    moves.push({ id, dx, dy });
    if (moves.length >= maxCards) break;
  }
  return moves;
}

function prefersReducedMotion(): boolean {
  return typeof window !== "undefined"
    && typeof window.matchMedia === "function"
    && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

function snapshotRects(board: HTMLElement): Map<number, FlipRect> {
  const m = new Map<number, FlipRect>();
  board.querySelectorAll<HTMLElement>("[data-flip-id]").forEach((el) => {
    const id = Number(el.dataset.flipId);
    if (!Number.isFinite(id)) return;
    const r = el.getBoundingClientRect();
    m.set(id, { left: r.left, top: r.top, width: r.width, height: r.height });
  });
  return m;
}

function playMove(board: HTMLElement, move: FlipMove): void {
  const el = board.querySelector<HTMLElement>(`[data-flip-id="${move.id}"]`);
  if (!el) return;
  // Invert: drop the card back at its old position with no transition…
  el.style.willChange = "transform";
  el.style.transition = "none";
  el.style.transform = `translate(${move.dx}px, ${move.dy}px)`;
  // …force the start transform to commit before we play, or the browser coalesces both into a no-op.
  void el.offsetWidth;
  // Play: transition to identity, i.e. the card's real (new) position.
  el.style.transition = `transform ${FLIP_MS}ms cubic-bezier(0.2, 0.8, 0.2, 1)`;
  el.style.transform = "";
  const cleanup = () => {
    el.style.transition = "";
    el.style.transform = "";
    el.style.willChange = "";
    el.removeEventListener("transitionend", cleanup);
  };
  el.addEventListener("transitionend", cleanup);
}

/**
 * Wires the flourish to a board container. On each reflected outside write it snapshots the cards' positions,
 * then plays the FLIP once the refetch re-render places them anew. The snapshot stays armed across renders until
 * a move is seen or the window elapses, so the async refetch has time to land. Outside Tauri the reflect
 * notification never fires, so this is inert.
 */
export function useBoardFlip(boardRef: RefObject<HTMLDivElement | null>, draggingId: number | null): void {
  const dragging = useRef(draggingId);
  dragging.current = draggingId;
  const armed = useRef<{ rects: Map<number, FlipRect>; at: number } | null>(null);

  useEffect(() => {
    if (!BOARD_FLIP) return;
    return subscribeStoreChangeReflected(() => {
      const board = boardRef.current;
      if (!board) return;
      if (prefersReducedMotion()) return;
      // The board's data is behind an async refetch, so at this instant the DOM still shows the old layout — this
      // captures "First" before the outside write's rows arrive on screen.
      armed.current = { rects: snapshotRects(board), at: Date.now() };
    });
  }, [boardRef]);

  useLayoutEffect(() => {
    if (!BOARD_FLIP) return;
    const a = armed.current;
    if (!a) return;
    const board = boardRef.current;
    if (!board) { armed.current = null; return; }
    const last = snapshotRects(board);
    const moves = planFlip(a.rects, last, {
      draggingId: dragging.current,
      viewport: { width: window.innerWidth, height: window.innerHeight },
      maxCards: FLIP_MAX_CARDS,
    });
    if (moves.length > 0) {
      armed.current = null;
      for (const m of moves) playMove(board, m);
    } else if (Date.now() - a.at > FLIP_ARM_WINDOW_MS) {
      armed.current = null; // the refetch never moved a visible card; stop waiting.
    }
    // else: positions have not changed yet — keep armed for the refetch render still in flight.
  });
}
