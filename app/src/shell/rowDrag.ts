/**
 * Reordering a list of rows with the pointer, as the arithmetic behind it — free of React and of the
 * webview's own drag, and therefore testable.
 *
 * The webview's drag is not available here: the app itself takes the files dropped on it, and with
 * that switch thrown an in-window HTML5 drag does not fire at all on macOS and Windows (`AMB-D-775`).
 * A press and a move are what a reorder is made of, and what is settled *here* is the half that
 * belongs to a list: **which side of a row the pointer is on**, and therefore where a drop would put
 * the row. Telling a press from a drag and asking what is under the pointer are the same two
 * questions the board's cards ask, and both are answered once in `../core/pointerDrag`.
 */
import { elementUnder, type Point } from "../core/pointerDrag";

/** Which side of a row the pointer is on — the midline decides, as it did before. */
export function sideOfRow(clientY: number, rect: { top: number; height: number }): "before" | "after" {
  return clientY < rect.top + rect.height / 2 ? "before" : "after";
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
  const row = elementUnder(point, attribute, doc);
  if (row === null) return null;
  const id = idOf(row);
  if (id === null || id === dragged) return null;
  return { id, side: sideOfRow(point.y, row.getBoundingClientRect()) };
}
