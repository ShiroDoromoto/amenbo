// **A terminal off the screen does not redraw every row for its selection** — a stand-in for what
// xterm.js 6.0.0 does not do itself (xtermjs/xterm.js#6208).
//
// xterm.js stops drawing a terminal whose element has left the screen: an IntersectionObserver sets
// the render service's `_isPaused`, a refresh asked for meanwhile only marks `_needsFullRefresh`, and
// every row is drawn once when it comes back. The selection does not go through that gate. Each line
// of output that scrolls the buffer asks the selection to be redrawn, and the DOM renderer answers
// that by drawing every row (`renderRows(0, rows - 1)`) — for a terminal nobody can see, on every line.
// With every page's panes kept mounted (`AMB-D-1028`) that is most of them.
//
// The stand-in puts the selection behind the same gate. While paused, it keeps the selection where
// the render service keeps it and marks both what is owed: the selection to draw, and every row. The
// render service already pays both when the terminal comes back — the full redraw, and the selection
// along with it — so nothing is drawn here at all.
//
// **This reaches into xterm.js's private fields**, which no release promises to keep.
// `./offscreenSelection.test.ts` opens a real terminal and fails when any of them moves, which is the
// moment to look whether #6208 was fixed upstream and this can go.

import type { Terminal } from "@xterm/xterm";

type Range = [number, number] | undefined;

/** The part of xterm.js's render service this stands in front of. */
interface RenderService {
  _isPaused: boolean;
  _needsFullRefresh: boolean;
  _needsSelectionRefresh: boolean;
  _selectionState: { start: Range; end: Range; columnSelectMode: boolean };
  handleSelectionChanged(start: Range, end: Range, columnSelectMode: boolean): void;
}

/** The render service of an opened terminal, or nothing if xterm.js no longer keeps it there. */
export function renderServiceOf(term: Terminal): RenderService | undefined {
  const core = (term as unknown as { _core?: { _renderService?: RenderService } })._core;
  return core?._renderService;
}

/**
 * Hold a paused terminal's selection redraws until it is drawn again. Called once, after `open`, which
 * is when the render service exists.
 *
 * Says whether it took. A terminal whose internals no longer look like this is left as it was rather
 * than broken — it only draws more than it has to.
 */
export function holdSelectionOffscreen(term: Terminal): boolean {
  const render = renderServiceOf(term);
  if (
    !render ||
    typeof render.handleSelectionChanged !== "function" ||
    typeof render._isPaused !== "boolean" ||
    typeof render._needsFullRefresh !== "boolean" ||
    typeof render._needsSelectionRefresh !== "boolean" ||
    typeof render._selectionState !== "object"
  ) {
    return false;
  }
  const draw = render.handleSelectionChanged.bind(render);
  render.handleSelectionChanged = (start, end, columnSelectMode) => {
    if (!render._isPaused) {
      draw(start, end, columnSelectMode);
      return;
    }
    render._selectionState.start = start;
    render._selectionState.end = end;
    render._selectionState.columnSelectMode = columnSelectMode;
    render._needsSelectionRefresh = true;
    render._needsFullRefresh = true;
  };
  return true;
}
