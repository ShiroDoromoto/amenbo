// How the panes of the terminal face are arranged: the project they belong to, the pages of one, and
// how many of them are drawn at once.
//
// **A pane belongs to a project, and the project is chosen before the pane is.** The rail names the
// projects and the panes under each, picking one puts that project's panes on the screen, and there
// is no way to put a pane anywhere else. A face where every pane could be pointed at any folder is a
// face where the projects are a label rather than a division — the reason to have them at all is that
// what is on one screen is one piece of work.
//
// **A frame is a place, not a process.** It exists once a terminal has been opened in it and stays
// when one ends, which is what lets a pane keep its last output on the screen and what lets a name
// outlive the session that earned it (`./frames`). Ids are handed out once and never reused within a
// run: a name is held against the id, so a reused one would put an old name on a new place. **A place
// does not outlive the app**, though — what is kept between runs is the split and the project, and
// not the frames (`AMB-T-3687`).
//
// **A place is made by opening one, and by nothing else.** A place somebody started making and walked
// away from is a box nobody can say anything about, so the folder is answered first
// (`../shell/FolderChoice`) and the frame is made after it.
//
// **A page with room draws one empty frame, and never one per gap.** A box in every gap is the same
// question asked as many times as the count allows, so there is a single one and it sits at the first
// gap on the page. It is the page saying it has room rather than a button for opening a terminal,
// which is why a full page draws none at all — the frames are what is on the screen, and the empty one
// is a remark about them. The gaps past it stay blank: a count is the shape that was pressed for, and
// what is not open there is nothing at all.
//
// **Pages are how a project's panes go past a screenful, and nobody makes one.** They are fixed slots
// in the sense that matters: what is open does not move about on its own. The count is the most that
// are drawn at once and not a number of boxes to fill, so a page holds up to that many and the last
// one holds what is left. No page stands empty: asking for another pane where every page is full is
// the one thing that brings one into being, and it lasts as long as the asking (`addPane`).

/**
 * Panes on one screen.
 *
 * **Where it stops is settled by columns of text, not by how many a person can watch.** What an
 * agent's TUI wants is eighty columns, which at the pane's monospace is about 624px and 645px with
 * the room around it — so eight across two rows is what the widest screen sold still draws readably,
 * and five across would put a pane under eighty on every screen there is. Watching is not the limit
 * any more: a pane that wants somebody says so on its own — the plate, the badge on the page and the
 * lamp all point at it — so eight of them need no watching over (`./plate`).
 */
export type Count = 1 | 2 | 4 | 6 | 8;

/** The counts a person can pick, in the order they are offered. */
export const COUNTS: readonly Count[] = [1, 2, 4, 6, 8];

/**
 * How many panes a count puts across, which is the whole of how it is laid out: the rows are
 * whatever is left over, and never more than two. **Width is spent before height** — a terminal runs
 * short of columns before it runs short of lines, and a third row would take the lines away first.
 *
 * The grid itself is drawn in the stylesheet (`.termface__page-grid--*`). This is the same shape
 * said where it can be checked: the rule that no count asks for a third row is a claim about every
 * count at once, and a stylesheet can only be read one class at a time. Nothing measures room
 * against it — what the columns beside the panes leave the middle is a pane's worth of floor and
 * not a count's worth (`./columns`).
 */
export const ACROSS: Readonly<Record<Count, number>> = { 1: 1, 2: 2, 4: 2, 6: 3, 8: 4 };

/** Two panes to start with: one is what a single terminal already was, and four is a screenful to
 *  arrive at rather than to be given. */
export const DEFAULT_COUNT: Count = 2;

/**
 * The arrangement as it is handed over — the wire shape of `TalkLayoutDto`.
 *
 * **It is how the two windows share one face**, and it lasts as long as the app is up: whichever
 * window is drawing the face writes it, and the one the terminal is split out into reads it as it
 * comes up (`app/src-tauri/src/frames.rs`).
 *
 * **What outlives the run is `count` and `project`, and nothing else** (`AMB-T-3687`). So an
 * arrangement read at the start of a run has no frames in it, and the face comes up on the project
 * the reader was looking at with one way in on it.
 */
export type SavedLayout = {
  count: number;
  /** The next id to hand out. It is this run's, like the frames it numbers: an arrangement that comes
   *  back with no frames starts again at the first. */
  nextId: number;
  /** The project whose panes the face was showing. It answers for the window the terminal was split
   *  out into, which has no ledger to have taken one from — and only where the arrangement came back
   *  with no panes in it, since a pane names its own project (`../shell/TerminalFace`). */
  project?: number;
  frames: { id: string; project?: number; folder?: string }[];
  /** The pane being worked in when the arrangement was last written. It is what the window split out
   *  of this face comes up on, so the reader lands where they left rather than on the first place of
   *  the first project (`AMB-D-753`). Read by that window and never by the board: which pane is
   *  being worked in *now* is the board's own state, and reading a written one back would move the
   *  person's place on the strength of an older write. */
  splitOut?: string;
};

/** One place a terminal is drawn, whether or not one is running in it. */
export type Frame = {
  /** Handed out once and never reused — the id `./frames` keeps this frame's name against. */
  readonly id: string;
  /** The project this pane is one of. It is settled when the pane is made and never changes: a pane
   *  that could move between projects would be the one thing the rail promises cannot happen. */
  readonly project: number;
  /** The terminal running here, or null for a frame whose program has ended. */
  readonly session: string | null;
  /** The folder that terminal works in — one of the folders its project is bound to. It is null for
   *  the one pane that takes up a terminal somebody else started: where that one runs was settled
   *  when it started, and the pane learns it from the session rather than from the person. */
  readonly folder: string | null;
};

/** The arrangement of the terminal face, as it stands. */
export type Layout = {
  readonly frames: readonly Frame[];
  /** The next id to hand out. Frames are never renumbered, so this only ever goes up. */
  readonly nextId: number;
  readonly count: Count;
  /** The project whose panes are on the screen, or null before the face has been told of one. */
  readonly project: number | null;
  /** The page of that project being shown, counted from 1. */
  readonly page: number;
  /** The frame the person is working in, or null before they have picked one. */
  readonly focus: string | null;
  /**
   * Whether a page has been brought into being for a pane nobody has opened yet (`addPane`).
   *
   * It is the one page that exists without panes on it, and it exists only while the person who asked
   * for it is on it: going anywhere else, or opening the pane, takes it away again. Nothing about it
   * is kept — a page nobody put a terminal on is not part of the arrangement (`laidOut`).
   */
  readonly adding: boolean;
};

export const EMPTY_LAYOUT: Layout = {
  frames: [],
  nextId: 1,
  count: DEFAULT_COUNT,
  project: null,
  page: 1,
  focus: null,
  adding: false,
};

/** The panes of one project, in the order they were opened. A project nobody has opened anything in
 *  has none, which is the face with one way in on it. */
export function panesOf(layout: Layout, project: number | null): readonly Frame[] {
  return project === null ? [] : layout.frames.filter((frame) => frame.project === project);
}

/**
 * How many pages the shown project's panes make.
 *
 * At least one, because a project with nothing open is still a project a person is looking at, and
 * the page it shows is where the empty frame is put. One more while a page has been asked for and
 * nothing opened on it yet (`addPane`) — that page is reachable, so it is counted.
 */
export function pageCount(layout: Layout): number {
  return filledPages(layout) + (layout.adding ? 1 : 0);
}

/** The pages the shown project's panes actually fill, which is every page but the one `addPane` may
 *  have brought into being. */
function filledPages(layout: Layout): number {
  const panes = panesOf(layout, layout.project).length;
  return Math.max(1, Math.ceil(panes / layout.count));
}

/**
 * Whether this page of the shown project has a gap in it.
 *
 * It is what the empty frame is drawn from: a page with room says so with one, and a full page says
 * nothing. Only the last page can have a gap — the panes fill the pages in the order they were opened
 * — so this is false everywhere else without having to be told.
 */
export function roomOnPage(layout: Layout, page: number): boolean {
  return slotsOf(layout, page).length < layout.count;
}

/** The panes drawn on one page, in the order they were opened. There is one per pane and no more:
 *  what is not open is not a box on the screen. */
export function slotsOf(layout: Layout, page: number): readonly Frame[] {
  const panes = panesOf(layout, layout.project);
  return panes.slice((page - 1) * layout.count, page * layout.count);
}

/** The page a frame is on, within its own project, or null for an id no frame has. */
export function pageOfFrame(layout: Layout, frame: string): number | null {
  const one = layout.frames.find((each) => each.id === frame);
  if (!one) return null;
  const at = panesOf(layout, one.project).findIndex((each) => each.id === frame);
  return at < 0 ? null : Math.floor(at / layout.count) + 1;
}

/** The frame a session is running in, or null where none is. */
export function frameOfSession(layout: Layout, session: string): Frame | null {
  return layout.frames.find((one) => one.session === session) ?? null;
}

/**
 * The pane of this project already working in `folder`, or null where none is.
 *
 * **A folder already open is not opened beside itself.** It is what a folder handed in from the
 * ledger lands on (`../shell/TerminalFace`): pressing the first loop's one button on a project whose
 * pane is already up is a person going to that pane, and a second one in the same folder would be the
 * same work in two places with the reader looking in the one they are not on.
 */
export function paneIn(layout: Layout, project: number, folder: string): Frame | null {
  return panesOf(layout, project).find((one) => one.folder === folder) ?? null;
}

/** One frame replaced, everything else as it was. An id no frame has leaves the layout alone. */
function withFrame(layout: Layout, frame: string, change: (was: Frame) => Frame): Layout {
  const at = layout.frames.findIndex((one) => one.id === frame);
  if (at < 0) return layout;
  const frames = [...layout.frames];
  frames[at] = change(frames[at]!);
  return { ...layout, frames };
}

/**
 * Make a place for a terminal in this project, and answer with the layout and the frame.
 *
 * It is called once the folder has been answered for — a pane is made by opening one, so there is no
 * moment where a frame exists with the question still on it. The new pane is the one being worked in
 * and the screen moves to the page it landed on, because a person who opened a pane is looking at it.
 */
export function openedFrame(layout: Layout, project: number, folder: string | null): { layout: Layout; frame: Frame } {
  const frame: Frame = { id: String(layout.nextId), project, session: null, folder };
  const next: Layout = {
    ...layout,
    frames: [...layout.frames, frame],
    nextId: layout.nextId + 1,
    project,
    // The page asked for has a pane on it now, so it is a page like any other (`addPane`).
    adding: false,
  };
  return { layout: focusOn(next, frame.id), frame };
}

/** A terminal has started in a frame. The folder is the one it was started in, which a pane that
 *  took one up learns here and nowhere else. */
export function openedIn(layout: Layout, frame: string, session: string, folder: string | null): Layout {
  return withFrame(layout, frame, (was) => ({ ...was, session, folder: folder ?? was.folder }));
}

/** The folder an agent says it is in now. A pane works in the folder it was **started** in, so this
 *  is taken only by a frame that has none — the one that adopted a terminal and is still learning
 *  where it runs. An agent's own `cd` does not move a pane to another folder. */
export function movedTo(layout: Layout, session: string, folder: string): Layout {
  const frame = frameOfSession(layout, session);
  if (!frame || frame.folder !== null) return layout;
  return withFrame(layout, frame.id, (was) => ({ ...was, folder }));
}

/** The program in a terminal has exited. The frame stays — it is a place, and the place is still
 *  there — with nothing running in it. */
export function closedIn(layout: Layout, session: string): Layout {
  const frame = frameOfSession(layout, session);
  return frame === null ? layout : withFrame(layout, frame.id, (was) => ({ ...was, session: null }));
}

/**
 * The frame itself is gone — the place, not the program in it.
 *
 * **This is the one thing that takes a pane off the screen for good**, and it is why the control that
 * does it asks first (`../shell/TerminalPane`): a program exiting leaves the place and its last output
 * standing, and only a person saying so removes it. Nothing of it is kept, so it does not come back on
 * the next run.
 *
 * **What is left closes up.** The panes are one list and the pages are slices of it, so the pane after
 * the closed one moves into its place and the last page loses a slot. That is not the screen
 * rearranging itself under a reader — the promise is that what is open does not move on its own, and
 * this moved because they asked for it.
 *
 * The reader is left where the closed pane was: whatever moved up into its place, or the pane before
 * it where nothing did.
 */
export function closedFrame(layout: Layout, frame: string): Layout {
  const gone = layout.frames.find((one) => one.id === frame);
  if (!gone) return layout;
  const at = panesOf(layout, gone.project).findIndex((one) => one.id === frame);
  const next: Layout = {
    ...layout,
    frames: layout.frames.filter((one) => one.id !== frame),
    adding: false,
  };
  if (layout.focus !== frame) return { ...next, page: Math.min(next.page, pageCount(next)) };
  const left = panesOf(next, gone.project);
  const heir = left[Math.min(at, left.length - 1)];
  return heir === undefined
    ? { ...next, focus: null, page: Math.min(next.page, pageCount(next)) }
    : focusOn(next, heir.id);
}

/**
 * Somebody asked for another pane: go to where it would land, and draw the empty frame there.
 *
 * **The asking is what makes a page**, and it is the only thing that does. Where a page still has a
 * gap this is only a move — that page's empty frame is already the one being pressed towards. Where
 * every page is full there is nowhere to put the question, so a page comes into being to hold it, and
 * it lasts exactly as long as the person stays on it (`adding`).
 */
export function addPane(layout: Layout): Layout {
  const last = filledPages(layout);
  return roomOnPage(layout, last)
    ? { ...layout, page: last, adding: false }
    : { ...layout, page: last + 1, adding: true };
}

/**
 * Show a project's panes.
 *
 * The whole screen is that project's from here on: its panes, its pages, and the pane being worked in
 * is one of them. Coming to a project lands on its first page and on the pane it opened first — a
 * project remembered where it was left would be a screen a person cannot predict from the row they
 * pressed.
 */
export function goProject(layout: Layout, project: number): Layout {
  if (layout.project === project) return layout;
  const first = panesOf(layout, project)[0] ?? null;
  return { ...layout, project, page: 1, focus: first?.id ?? null, adding: false };
}

/** Show a page of the project that is up, as far as there are pages to show. A page asked for and not
 *  opened on goes away as soon as the reader is somewhere else — it was the question, not a page. */
export function goPage(layout: Layout, page: number): Layout {
  if (page < 1 || page > pageCount(layout)) return layout;
  return { ...layout, page, adding: layout.adding && page === pageCount(layout) };
}

/** Work in a frame, bringing its project and page up with it — the rail's rows reach panes that are
 *  not on the screen, and reaching one has to show it. */
export function focusOn(layout: Layout, frame: string): Layout {
  const one = layout.frames.find((each) => each.id === frame);
  if (!one) return layout;
  const shown: Layout = { ...layout, project: one.project, adding: false };
  const page = pageOfFrame(shown, frame);
  return page === null ? layout : { ...shown, page, focus: frame };
}

/**
 * Show a different number of panes.
 *
 * The panes do not move: a project's are one list, and the count is how much of it a page shows. What
 * has to be carried across is the pane being worked in — a person who asks for one pane means the one
 * they were looking at — so the page follows the focus rather than the number.
 */
export function setCount(layout: Layout, count: Count): Layout {
  // A page asked for is measured against the old count, so it does not survive a change of it: what
  // the reader gets back is the pane they were on, on the page it is now.
  const next: Layout = { ...layout, count, adding: false };
  const page = next.focus === null ? null : pageOfFrame(next, next.focus);
  return { ...next, page: Math.min(page ?? layout.page, pageCount(next)) };
}

/**
 * The arrangement as it is written down, for the other window to read.
 *
 * **What is written is the shape**: how many panes to a page, the panes in the order they were
 * opened, and for each the project it is one of and the folder it is working in. What is running is
 * not — a session is a process, and a pane drawn as though one were still in it would be the window
 * saying something untrue. So a pane comes over as a place with its folder on it, and nothing is
 * started until somebody presses.
 *
 * **The project this face is on goes with it**, and is the one part of the shape this face never
 * reads back (`restored`). It is written for the window with no ledger: a terminal split out into
 * one of its own has nobody to have asked it which project it is drawing, so an arrangement with no
 * panes in it opens as the project the board was on (`../talk.tsx`).
 */
export function laidOut(layout: Layout): SavedLayout {
  return {
    count: layout.count,
    nextId: layout.nextId,
    ...(layout.project === null ? {} : { project: layout.project }),
    frames: layout.frames.map((frame) => ({
      id: frame.id,
      project: frame.project,
      ...(frame.folder === null ? {} : { folder: frame.folder }),
    })),
    // The pane being worked in, written down for the window the terminal is split out into: the
    // press says nothing, so where the reader was is theirs to read back out of the shape.
    ...(layout.focus === null ? {} : { splitOut: layout.focus }),
  };
}

/**
 * The layout an arrangement comes back as.
 *
 * **An arrangement with no frames in it still says something**, and it is what every window that
 * comes up after a run reads: the split the person chose is theirs, and it comes back whether or not
 * there is anything to draw with it (`AMB-T-3687`). What that leaves is the empty face, laid out the
 * way they laid it out.
 *
 * `onto` is the project the window is on, and it answers for the frames an older build wrote without
 * one: a pane whose project nothing records is put where the person is rather than dropped, and where
 * there is nowhere to put it there is nothing to draw.
 */
export function restored(saved: SavedLayout, onto: number | null): Layout {
  const count = COUNTS.find((one) => one === saved.count) ?? DEFAULT_COUNT;
  const frames: Frame[] = [];
  for (const frame of saved.frames) {
    const project = frame.project ?? onto;
    if (project === null) continue;
    frames.push({ id: frame.id, project, session: null, folder: frame.folder ?? null });
  }
  const first = frames[0];
  return {
    frames,
    // Ids are never reused, so the next one has to clear every frame that came with the arrangement
    // — one written by a newer build, or an id list nobody can vouch for, must not hand a fresh
    // frame the name of one already up.
    nextId: Math.max(saved.nextId, ...frames.map((frame) => Number(frame.id) + 1 || 0)),
    count,
    project: first?.project ?? onto,
    page: 1,
    focus: first?.id ?? null,
    adding: false,
  };
}
