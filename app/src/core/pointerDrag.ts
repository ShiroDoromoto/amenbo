// The arithmetic every pointer drag on this screen is made of (`AMB-D-775`).
//
// Two gestures were rebuilt out of pointer events when the application took over the drop — a card
// carried between the board's columns (`../screens/boardDrag`) and a row carried up a list
// (`../shell/rowDrag`) — and both had to answer for themselves the two questions the webview's own
// drag used to answer for them: **has this press become a drag**, and **what is under the pointer**.
// They answered them apart, with a threshold and a hit test each, which is two places for one answer
// to drift.
//
// What is not here is everything a gesture does with those answers. A card raises a ghost of itself
// and crosses columns; a row decides which side of a midline it came down on. That is what each of
// the two gestures *is*, and it stays where it is drawn.

/** A point in the viewport's own coordinates, which is what a pointer event reports. */
export interface Point {
  x: number;
  y: number;
}

/**
 * How far the pointer travels before a press counts as a drag, in CSS pixels.
 *
 * **There has to be one.** A card and a row are both buttons — one opens, the other navigates — so
 * without a threshold every press meant as a drag would do that instead (`AMB-D-775`). Five pixels is
 * what a hand at rest stays inside: small enough that a deliberate drag never feels stuck, wide
 * enough that a click on a trackpad is still a click.
 *
 * **One number for both gestures.** They were rebuilt a pixel apart, and nothing ever measured them
 * apart — what `AMB-T-3755` timed was the events, not the slop. A hand that reorders a list and then
 * moves a card should not find the line drawn in a different place on the second face.
 */
export const DRAG_SLOP = 5;

/** How far the press has travelled from where it went down. */
function travelled(from: Point, to: Point): number {
  return Math.hypot(to.x - from.x, to.y - from.y);
}

/** Whether it has travelled far enough for this to be a drag rather than a press. */
export function draggedFar(from: Point, to: Point): boolean {
  return travelled(from, to) >= DRAG_SLOP;
}

/**
 * What a point is over — the nearest thing above it carrying `attribute`, or nothing.
 *
 * **Nothing here is cached, and that is the whole point.** A list scrolls under a held pointer —
 * measured at 356 px in one wheel gesture — so a rectangle taken when the drag began names something
 * that is no longer there (`AMB-T-3755`). Asking the document each time costs one hit test and is
 * always right.
 *
 * The thing is named by a `data-` attribute rather than by a React tree, because the answer has to
 * come from what is actually painted at that point and a tree cannot say what is on top.
 *
 * `document` is a parameter so a test can answer for it: a headless DOM has no layout, and every
 * point in one is over nothing at all.
 */
export function elementUnder(
  point: Point,
  attribute: string,
  doc: Pick<Document, "elementFromPoint"> = document,
): HTMLElement | null {
  const at = doc.elementFromPoint(point.x, point.y);
  return at instanceof Element ? at.closest<HTMLElement>(`[${attribute}]`) : null;
}
