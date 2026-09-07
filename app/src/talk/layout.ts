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
 * How many panes a count puts across when they go across, which with the orientation is the whole of
 * how it is laid out: the rows are whatever is left over, and never more than two. **Width is spent
 * before height** — a terminal runs short of columns before it runs short of lines, and a third row
 * would take the lines away first.
 *
 * **Two is the one count where that turns around**, which is why it is the one count a person is
 * asked about (`Orient`). Two across halves the columns of each pane, and half of a window with a
 * column beside it is under the eighty a TUI wants; two down leaves the columns whole and takes the
 * lines instead. So this is what a count puts across before the answer, and `acrossIn` is what it
 * puts across after it.
 *
 * The grid itself is drawn in the stylesheet (`.termface__page-grid--*`). This is the same shape
 * said where it can be checked: the rule that no count asks for a third row is a claim about every
 * count at once, and a stylesheet can only be read one class at a time. Nothing measures room
 * against it — what the columns beside the panes leave the middle is a pane's worth of floor and
 * not a count's worth (`./columns`).
 */
export const ACROSS: Readonly<Record<Count, number>> = { 1: 1, 2: 2, 4: 2, 6: 3, 8: 4 };

/**
 * Which way the panes of a two-pane page sit: side by side, or one above the other.
 *
 * **It is asked about two and about nothing else.** At four and above the rows are already spent
 * (`ACROSS`), and there is no arrangement of them left to choose between; at one there is nothing to
 * arrange. So this is not a second axis on every count — it is the one count where spending width
 * first stops being the right answer, said where a person can say otherwise.
 */
export type Orient = "across" | "down";

/** The orientations a person can pick, in the order they are offered. */
export const ORIENTS: readonly Orient[] = ["across", "down"];

/** Across, because it is what every count does and what two did before it could be asked. */
export const DEFAULT_ORIENT: Orient = "across";

/** Whether this count has an orientation to choose. Two, and only two (`Orient`). */
export function orientable(count: Count): boolean {
  return count === 2;
}

/** How many panes a count puts across, once the orientation has been taken into account. */
export function acrossIn(count: Count, orient: Orient): number {
  return orientable(count) && orient === "down" ? 1 : ACROSS[count];
}

/**
 * The grid a count and an orientation ask for, as the stylesheet names it
 * (`.termface__page-grid--*`).
 *
 * A count that has no orientation to choose is named by its number alone: the class is what the page
 * is laid out by, and a name with an answer in it that the count cannot be asked would be a second
 * class doing the same thing as the first.
 */
export function pageShape(count: Count, orient: Orient): string {
  return orientable(count) && orient === "down" ? `${count}-down` : String(count);
}

/**
 * What a project nobody has split opens at.
 *
 * **One, because the split is now an answer given on a project** and a project that has never been
 * answered for has not been given one. Two would put an empty box beside the first pane on every
 * project a reader walks into — a question about a second terminal, asked by the face rather than by
 * them — and the wide splits are arrived at the same way they always were, by pressing for them.
 *
 * It is also what a face has when it is on no project at all, where there is nothing to have been
 * answered about.
 */
export const DEFAULT_COUNT: Count = 1;

/**
 * How one project's page is split — one answer, kept against the project it was given on
 * (`SplitDto`).
 *
 * **The count and the orientation travel together** because they are one answer: two panes laid down
 * is not the same shape as two across, and a project remembered at the count without the way it sat
 * would come back at a grid nobody left it in.
 */
export type Split = { readonly count: Count; readonly orient: Orient };

/** What a project nobody has answered for is drawn at. */
const UNANSWERED: Split = { count: DEFAULT_COUNT, orient: DEFAULT_ORIENT };

/**
 * The arrangement as it is handed over — the wire shape of `TalkLayoutDto`.
 *
 * **It is how the two windows share one face**, and it lasts as long as the app is up: whichever
 * window is drawing the face writes it, and the one the terminal is split out into reads it as it
 * comes up (`app/src-tauri/src/frames.rs`).
 *
 * **What outlives the run is the splits and `project`, and nothing else** (`AMB-T-3687`). So an
 * arrangement read at the start of a run has no frames in it, and the face comes up on the project
 * the reader was looking at, at the split that project was left at, with one way in on it.
 */
export type SavedLayout = {
  count: number;
  /** Which way a two-pane page sits, absent where it sits the way every other count does
   *  (`Orient`). It is kept at every count and not only at two: a person who went to four and asked
   *  for two again means the two they set up, not the default back. */
  orient?: Orient;
  /** The split of each project that has one, by project — and the two above read at `project`, which
   *  is what the face is laid out from every render (`SplitDto`). A project nobody has answered for
   *  is not in it: what is kept is the answers, and a row for every project a reader ever walked
   *  through would say nothing about most of them. */
  splits?: Record<string, { count: number; orient?: Orient }>;
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
  /** How many panes a page of the project on the screen holds — that project's own answer, or what
   *  a project nobody has answered for is drawn at (`splits`). */
  readonly count: Count;
  /** Which way a two-pane page sits (`Orient`). It stands at every count, and is drawn on at two. */
  readonly orient: Orient;
  /**
   * The split each project has been left at, by project (`Split`).
   *
   * **A project is drawn at its own answer, and moving to one moves the face to that answer.** How
   * many panes a person wants is a fact about the work, not about the face: a repository with an
   * agent and its shell wants two, and the one they read in wants one, and a face with a single
   * count made every move between them rewrite whichever they came from.
   *
   * `count` and `orient` above are this read at `project`, kept beside it because the page is laid
   * out from them on every render. Nothing is in here for a project nobody has answered for —
   * `DEFAULT_COUNT` is what that project is drawn at, and a row saying so would be an answer put in
   * a person's mouth.
   */
  readonly splits: Readonly<Record<number, Split>>;
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
  orient: DEFAULT_ORIENT,
  splits: {},
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
  const shown = showing(layout, project);
  const first = panesOf(shown, project)[0] ?? null;
  return { ...shown, page: 1, focus: first?.id ?? null, adding: false };
}

/** The split a project is drawn at: its own answer, or what a project nobody has answered for gets
 *  (`Layout.splits`). */
function splitOf(layout: Layout, project: number | null): Split {
  return (project === null ? undefined : layout.splits[project]) ?? UNANSWERED;
}

/**
 * Put the face on a project, laid out the way that project was left.
 *
 * Every way onto a project goes through here, `goProject` and `focusOn` alike: a pane reached from
 * the rail is as much a move between projects as a tab is, and a split that followed only one of the
 * two would draw the same project at two different counts depending on how the reader got to it.
 */
function showing(layout: Layout, project: number): Layout {
  // Staying where you are is not a move, and the split is not re-read for one: what the face is laid
  // out at right now is the answer for this project, whether or not one has been kept yet.
  if (layout.project === project) return layout;
  const split = splitOf(layout, project);
  return { ...layout, project, count: split.count, orient: split.orient };
}

/** The answers with this project's put in. A face on no project keeps none — an answer is given on a
 *  project, and there is nothing here to hold one against. */
function answered(layout: Layout, split: Split): Readonly<Record<number, Split>> {
  return layout.project === null ? layout.splits : { ...layout.splits, [layout.project]: split };
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
  const shown: Layout = { ...showing(layout, one.project), adding: false };
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
  const next: Layout = {
    ...layout,
    count,
    // The press is an answer about the project on the screen, and it is kept as one: going to
    // another project and coming back brings this shape with it (`Layout.splits`).
    splits: answered(layout, { count, orient: layout.orient }),
    adding: false,
  };
  const page = next.focus === null ? null : pageOfFrame(next, next.focus);
  return { ...next, page: Math.min(page ?? layout.page, pageCount(next)) };
}

/**
 * Lay a two-pane page the other way.
 *
 * **Nothing moves but the grid.** How many panes a page holds is the count, so the pages are the same
 * pages, the panes are on the ones they were on, and the reader stays in the pane they were working
 * in — what changes is where the two of them are drawn.
 */
export function setOrient(layout: Layout, orient: Orient): Layout {
  if (layout.orient === orient) return layout;
  return { ...layout, orient, splits: answered(layout, { count: layout.count, orient }) };
}

/**
 * One pane put before or after another, within the list its project's panes make.
 *
 * **The list is the whole of what a reorder moves.** The pages are that list cut at the count
 * (`slotsOf`), so a pane carried onto another page is the same move as one carried across a page —
 * there is no second operation for crossing one, and none is drawn (`AMB-D-853`).
 *
 * A pane dropped on itself, and an id no pane in the list has, both come back as the list unchanged:
 * a gesture that settled nowhere is not a new order.
 */
export function movedWithin(
  panes: readonly Frame[],
  moved: string,
  target: string,
  side: "before" | "after",
): readonly Frame[] {
  if (moved === target) return panes;
  const one = panes.find((each) => each.id === moved);
  if (!one || !panes.some((each) => each.id === target)) return panes;
  const rest = panes.filter((each) => each.id !== moved);
  const at = rest.findIndex((each) => each.id === target);
  rest.splice(side === "before" ? at : at + 1, 0, one);
  return rest;
}

/**
 * The shown project's panes put in this order, with everything else left as it stands.
 *
 * **Only the shown project's panes move.** A frame's project is settled when it is made (`Frame`), so
 * the reorder is written back into the places this project's frames already hold in the whole list
 * and the frames of every other project stay where they are.
 *
 * **The page and the pane being worked in are not touched.** A pane that has moved onto another page
 * is still the pane the person was in, and taking them to it would be the reorder deciding where they
 * are looking (`AMB-D-853`, `goPage`).
 *
 * An order that is not this project's panes — one short, one over, or one from another project — is
 * refused whole rather than written in part.
 */
export function reordered(layout: Layout, order: readonly Frame[]): Layout {
  const panes = panesOf(layout, layout.project);
  if (order.length !== panes.length) return layout;
  const ids = new Set(panes.map((one) => one.id));
  if (!order.every((one) => ids.has(one.id))) return layout;
  const places = layout.frames.flatMap((one, at) => (one.project === layout.project ? [at] : []));
  const frames = [...layout.frames];
  places.forEach((place, i) => { frames[place] = order[i]!; });
  return { ...layout, frames };
}

/**
 * The arrangement as it is written down, for the other window to read.
 *
 * **What is written is the shape**: the split each project has been answered at, the panes in the
 * order they were opened, and for each the project it is one of and the folder it is working in. What is running is
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
  const splits = Object.entries(layout.splits).map(([project, split]) => [
    project,
    // Across is what a page does when nothing says otherwise, so it is left out of the row the way
    // the face's own orientation is — the shape a person asked for, and no more.
    split.orient === DEFAULT_ORIENT ? { count: split.count } : { count: split.count, orient: split.orient },
  ] as const);
  return {
    count: layout.count,
    nextId: layout.nextId,
    ...(layout.orient === DEFAULT_ORIENT ? {} : { orient: layout.orient }),
    ...(splits.length === 0 ? {} : { splits: Object.fromEntries(splits) }),
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
 * comes up after a run reads: the splits the person chose are theirs, and they come back whether or
 * not there is anything to draw with them (`AMB-T-3687`). What that leaves is the empty face, on the
 * project they were on and laid out the way they laid that project out.
 *
 * `onto` is the project the window is on, and it answers for the frames an older build wrote without
 * one: a pane whose project nothing records is put where the person is rather than dropped, and where
 * there is nowhere to put it there is nothing to draw.
 */
export function restored(saved: SavedLayout, onto: number | null): Layout {
  const splits = answers(saved.splits);
  const frames: Frame[] = [];
  for (const frame of saved.frames) {
    const project = frame.project ?? onto;
    if (project === null) continue;
    frames.push({ id: frame.id, project, session: null, folder: frame.folder ?? null });
  }
  const first = frames[0];
  const project = first?.project ?? onto;
  // The split the face opens at. It is read out of the answers where the project it lands on has
  // one, and off the pair beside them where it has not — which is what an arrangement written before
  // the answers were kept by project has, and all it has.
  const opening = (project === null ? undefined : splits[project]) ?? {
    count: COUNTS.find((one) => one === saved.count) ?? DEFAULT_COUNT,
    orient: ORIENTS.find((one) => one === saved.orient) ?? DEFAULT_ORIENT,
  };
  return {
    frames,
    // Ids are never reused, so the next one has to clear every frame that came with the arrangement
    // — one written by a newer build, or an id list nobody can vouch for, must not hand a fresh
    // frame the name of one already up.
    nextId: Math.max(saved.nextId, ...frames.map((frame) => Number(frame.id) + 1 || 0)),
    count: opening.count,
    orient: opening.orient,
    splits,
    project,
    page: 1,
    focus: first?.id ?? null,
    adding: false,
  };
}

/**
 * The answers an arrangement is carrying, as this build can draw them.
 *
 * A row whose count this build has no grid for is dropped rather than rounded: it was written by a
 * build that offered some other split, and a project put back at a count the stylesheet cannot lay
 * out is a page with no rule for it. What is dropped comes back as the unanswered shape, which is
 * the honest reading — this build does not know what that project was left at.
 */
function answers(kept: SavedLayout["splits"]): Record<number, Split> {
  const splits: Record<number, Split> = {};
  for (const [key, split] of Object.entries(kept ?? {})) {
    const project = Number(key);
    const count = COUNTS.find((one) => one === split.count);
    if (count === undefined || !Number.isInteger(project)) continue;
    splits[project] = { count, orient: ORIENTS.find((one) => one === split.orient) ?? DEFAULT_ORIENT };
  }
  return splits;
}
