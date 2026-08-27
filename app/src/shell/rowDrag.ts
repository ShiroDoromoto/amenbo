/**
 * Reordering a list of rows with the pointer, as the arithmetic behind it — free of React and of the
 * webview's own drag, and therefore testable.
 *
 * The webview's drag is not available here: the app itself takes the files dropped on it, and with
 * that switch thrown an in-window HTML5 drag does not fire at all on macOS and Windows (`AMB-D-775`).
 * A press and a move are what a reorder is made of, so two things are settled in this file: **when a
 * press counts as a drag**, and **which side of a row the pointer is on**.
 */

/**
 * How far the pointer travels before a press counts as a drag, in CSS pixels.
 *
 * **There has to be one.** A row is a button that navigates, so without a threshold every press meant
 * as a reorder would navigate instead (`AMB-D-775`). Five pixels is what a hand at rest stays
 * inside — small enough that the drag feels immediate, wide enough that a click on a trackpad is
 * still a click.
 */
export const DRAG_SLOP = 5;

/** A point in the viewport's own coordinates, which is what a pointer event reports. */
export interface Point {
  x: number;
  y: number;
}

/** Whether the pointer has moved far enough from where it went down for this to be a drag. */
export function draggedFar(from: Point, to: Point): boolean {
  return Math.hypot(to.x - from.x, to.y - from.y) >= DRAG_SLOP;
}

/** Which side of a row the pointer is on — the midline decides, as it did before. */
export function sideOfRow(clientY: number, rect: { top: number; height: number }): "before" | "after" {
  return clientY < rect.top + rect.height / 2 ? "before" : "after";
}

/**
 * The row under the pointer, read off the document rather than off anything remembered.
 *
 * **Nothing here is cached, and that is the whole point.** A list can scroll under a held pointer —
 * measured at 356 px in one wheel gesture — so a rectangle taken when the drag began names a row
 * that is no longer there (`AMB-T-3755`). Asking the document each time costs one hit test and is
 * always right.
 *
 * `document` is a parameter so a test can answer for it: a headless DOM has no layout, and every
 * point in one is over nothing at all.
 */
export function rowUnder(
  point: Point,
  attribute: string,
  doc: Pick<Document, "elementFromPoint"> = document,
): HTMLElement | null {
  const at = doc.elementFromPoint(point.x, point.y);
  return at instanceof Element ? at.closest<HTMLElement>(`[${attribute}]`) : null;
}

/**
 * Where a drag that ended here would put the row, or nothing where it would put it back.
 *
 * A row dropped on itself is not a move, and neither is one dropped off the list. Both come back as
 * nothing rather than as a no-op write, so the caller has one thing to check.
 */
export function landing(
  dragged: number,
  point: Point,
  attribute: string,
  idOf: (row: HTMLElement) => number | null,
  doc: Pick<Document, "elementFromPoint"> = document,
): { id: number; side: "before" | "after" } | null {
  const row = rowUnder(point, attribute, doc);
  if (row === null) return null;
  const id = idOf(row);
  if (id === null || id === dragged) return null;
  return { id, side: sideOfRow(point.y, row.getBoundingClientRect()) };
}
