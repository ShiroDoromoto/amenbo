/**
 * The arithmetic behind moving and sizing a pane on the page it is drawn on (`AMB-D-939`) — free of
 * React, and therefore testable where jsdom has no layout to measure.
 *
 * Two gestures live on the face. **A pane's header is carried onto another pane** and lands before or
 * after it, which is the same question a row of a list settles and is settled by the same midline
 * (`./rowDrag`). **A pane's bottom-right corner is pulled**, and what it lands on is one of the six
 * sizes rather than a rectangle of its own: a pane holds a size and never a shape (`../talk/layout`).
 *
 * What is not here is what the face does with the answers. Which pane is held, what is drawn while it
 * is, and when the order is written are the face's own (`./WorkspaceFace`).
 */
import { ACROSS, BOXES, DOWN, SIZES, type Size, type Spot } from "../talk/layout";
import type { Point } from "../core/pointerDrag";
import { sideOfBox } from "./rowDrag";

/** A rectangle as `getBoundingClientRect` gives one, which is all either gesture reads off an
 *  element. */
export type Rect = { readonly left: number; readonly top: number; readonly width: number; readonly height: number };

/**
 * Which way a pane of this size has its neighbours, and therefore which midline settles a drop on it.
 *
 * A pane that takes the whole width has the pane before it above rather than beside it, and every
 * narrower one has its neighbours to the left and right (`../talk/layout`).
 */
export function axisOnPane(size: Size): "across" | "down" {
  return BOXES[size].across === ACROSS ? "down" : "across";
}

/** Which side of a pane the pointer is on, and therefore where a pane dropped here would go. */
export function sideOnPane(point: Point, rect: Rect, size: Size): "before" | "after" {
  return sideOfBox(point, rect, axisOnPane(size));
}

/**
 * How many cells the corner is being pulled to, from the cell the pane starts in.
 *
 * Rounded rather than truncated, so the cell the pointer is nearest to the far edge of is the one it
 * has reached. It is not held to the room left on the page: a pane that grows past what is left of
 * its row is laid down again from the order, wherever that puts it (`resized`), and a corner that
 * stopped at the edge of the room would say the page is what a size is measured against.
 */
function cellsTo(grid: Rect, spot: Pick<Spot, "across" | "down">, point: Point): { across: number; down: number } {
  const across = Math.round((point.x - grid.left) / (grid.width / ACROSS)) - spot.across;
  const down = Math.round((point.y - grid.top) / (grid.height / DOWN)) - spot.down;
  return {
    across: Math.min(Math.max(across, 1), ACROSS),
    down: Math.min(Math.max(down, 1), DOWN),
  };
}

/**
 * The size a corner let go of here lands on.
 *
 * **The rows come off the drag down and the columns off the drag across**, which is the two halves of
 * the gesture answering the two halves of a size. Every one of the six is reachable that way, and
 * neither half is decided by the other.
 *
 * **Within a row count it snaps to the widest size the pointer has covered**, and to the narrowest
 * where it has covered none. So dragging right grows the pane a step at a time through the sizes
 * there are, and a pointer between two of them has not reached the wider one yet — which is what the
 * outline drawn under the hand is saying while it is being dragged.
 */
export function sizeStretchedTo(grid: Rect, spot: Pick<Spot, "across" | "down">, point: Point): Size {
  const want = cellsTo(grid, spot, point);
  const tall = SIZES.filter((size) => BOXES[size].down === want.down);
  const widest = tall
    .filter((size) => BOXES[size].across <= want.across)
    .sort((a, b) => BOXES[b].across - BOXES[a].across)[0];
  return widest ?? tall.reduce((a, b) => (BOXES[b].across < BOXES[a].across ? b : a));
}
