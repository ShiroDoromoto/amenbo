// Scrolling a box while something is being held near its edge — the last of the things the webview's
// own drag used to do, and the only one that was never there to begin with (`AMB-D-775`).
//
// A column runs longer than the window and the board runs wider than it, so a card picked up at the
// top has nowhere to go: the pointer reaches the edge of the screen with the target still off it, and
// the only way out is to put the card down somewhere it does not belong. HTML5 drag did not solve
// this either — the browsers that scroll for a held item do it for their own drag, and this gesture
// is made of pointer events. So it is added rather than restored, and `AMB-T-3755` measured that it
// holds on all three systems (0 → 356px, the drop target still correct at the end of it).
//
// **The loop is its own, and it has to be.** A pointer resting against an edge fires no `pointermove`
// at all, so a scroll driven off moves stops the moment the hand stops — which is exactly when the
// scroll is wanted. What drives it is a frame callback that runs for as long as the gesture does, and
// asks where the pointer is rather than being told.

/** A point in the viewport's own coordinates, which is what a pointer event reports. */
export interface Point {
  x: number;
  y: number;
}

/**
 * How near an edge the pointer has to come before the box moves, in CSS pixels.
 *
 * Wide enough to reach without aiming, and narrow enough that the middle of a short column is not
 * already inside it — a box shorter than two bands splits the difference instead ({@link edgePush}).
 */
export const EDGE_BAND = 50;

/** How far the box travels per frame, in CSS pixels. Measured at this size on all three systems (`AMB-T-3755`). */
export const EDGE_STEP = 8;

/**
 * How far a box wants to move along one axis this frame: towards `near`, towards `far`, or not at all.
 *
 * The pointer has to be *in* the box for the box to answer — a point beyond either edge is over
 * something else, and the walk up the ancestors ({@link scrollNearEdge}) is what finds out what.
 */
export function edgePush(at: number, near: number, far: number): number {
  if (at < near || at > far) return 0;
  // Half of a box that cannot hold two bands, so the near edge and the far edge never both claim the
  // same point — a 60px column would otherwise scroll up wherever the pointer was put in it.
  const band = Math.min(EDGE_BAND, (far - near) / 2);
  if (at < near + band) return -EDGE_STEP;
  if (at > far - band) return EDGE_STEP;
  return 0;
}

/**
 * What the flow needs of a box it might move. An element is one of these already — the names are the
 * DOM's, so that a test can hand over a plain object with a rectangle it decided on itself.
 */
export interface Scrollable {
  scrollLeft: number;
  scrollTop: number;
  scrollWidth: number;
  scrollHeight: number;
  clientWidth: number;
  clientHeight: number;
  getBoundingClientRect(): { left: number; right: number; top: number; bottom: number };
}

function clamp(v: number, hi: number): number {
  return v < 0 ? 0 : v > hi ? hi : v;
}

/**
 * Move one box one frame's worth, and say whether it actually moved.
 *
 * **Not moving is the answer that matters.** A box with nothing to scroll, or one already against the
 * stop the pointer is asking for, gives back `false` — and that is what sends the search on to the
 * box behind it, which is how a column inside a board hands the sideways travel to the board.
 *
 * 🚨 `flows` is asked whether the reader could have scrolled that axis, and it is not a formality:
 * **a box clipped with `overflow: hidden` moves perfectly well when a script writes to it** while no
 * reader can move it back. Truncated row titles are clipped that way all over this app, and without
 * the question a card carried past one would scroll its words out of sight for good. It is asked only
 * where the arithmetic already wants to move something, because reading a computed style is not free
 * and this runs every frame.
 *
 * The stops are applied here rather than left to the setter: an element clamps for itself, but a box
 * that cannot scroll at all takes the write and stays where it is, and both have to read the same.
 */
export function nudge(box: Scrollable, at: Point, flows: (axis: "x" | "y") => boolean): boolean {
  const rect = box.getBoundingClientRect();
  const roomX = box.scrollWidth - box.clientWidth;
  const roomY = box.scrollHeight - box.clientHeight;
  let dx = roomX > 0 ? edgePush(at.x, rect.left, rect.right) : 0;
  let dy = roomY > 0 ? edgePush(at.y, rect.top, rect.bottom) : 0;
  if (dx !== 0 && !flows("x")) dx = 0;
  if (dy !== 0 && !flows("y")) dy = 0;
  if (dx === 0 && dy === 0) return false;
  const wasX = box.scrollLeft;
  const wasY = box.scrollTop;
  box.scrollLeft = clamp(wasX + dx, roomX);
  box.scrollTop = clamp(wasY + dy, roomY);
  return box.scrollLeft !== wasX || box.scrollTop !== wasY;
}

/**
 * Scroll whatever box the pointer is over, if being where it is asks for it.
 *
 * Read off the document every frame and never remembered: the box the pointer is over changes as the
 * held thing crosses the screen, and a rectangle taken when the gesture began names a box 356 pixels
 * from where it now is (`AMB-T-3755`).
 *
 * `document` is a parameter so a test can answer for it — a headless DOM has no layout, and every
 * point in one is over nothing at all.
 */
export function scrollNearEdge(at: Point, doc: Pick<Document, "elementFromPoint"> = document): boolean {
  const hit = doc.elementFromPoint(at.x, at.y);
  for (let el = hit instanceof HTMLElement ? hit : null; el !== null; el = el.parentElement) {
    if (nudge(el, at, (axis) => scrolls(el, axis))) return true;
  }
  return false;
}

/** Whether a reader could scroll this axis of this box — which is not the same as whether a script could. */
function scrolls(el: HTMLElement, axis: "x" | "y"): boolean {
  const style = getComputedStyle(el);
  const how = axis === "x" ? style.overflowX : style.overflowY;
  return how === "auto" || how === "scroll" || how === "overlay";
}

/**
 * Keep the box under the pointer flowing for as long as the gesture lasts, and hand back the way to stop.
 *
 * `where` is asked once a frame and answers `null` where nothing is being held — a press that has not
 * yet become a drag is not a reason to scroll anything. `moved` is called on the frames that shifted
 * a box, and only on those: what the pointer is over has changed underneath it, so whatever the
 * caller drew from a hit test is a frame out of date even though the hand never moved.
 */
export function flowEdges(where: () => Point | null, moved: () => void): () => void {
  let frame = requestAnimationFrame(function tick() {
    frame = requestAnimationFrame(tick);
    const at = where();
    if (at !== null && scrollNearEdge(at)) moved();
  });
  return () => cancelAnimationFrame(frame);
}
