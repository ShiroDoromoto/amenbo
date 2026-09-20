// How the panes of the workspace are arranged: the project they belong to, how much of a page each
// one takes, and the pages that fall out of laying them down in order.
//
// **A pane belongs to a project, and the project is chosen before the pane is.** The rail names the
// projects and the panes under each, picking one puts that project's panes on the screen, and there
// is no way to put a pane anywhere else. A face where every pane could be pointed at any folder is a
// face where the projects are a label rather than a division — the reason to have them at all is that
// what is on one screen is one piece of work.
//
// **A frame is a place, not a process.** It exists once a terminal has been opened in it and stays
// when one ends, which is what lets a pane keep its last output on the screen and what lets a name
// outlive the session that earned it (`./frames`). **A place outlives the app as well**: what is kept
// between runs is the size, the project and a row a pane — where it works, what was started in it,
// what it is called and the handle it is resumed from (`AMB-D-869`). **An id is drawn rather than
// counted** (`newFrameId`, `AMB-D-897`): a name and a way back into a session are both held against
// it, so an id handed out twice would put them on a place neither belongs to — which is what a count
// kept in the same row as the panes could not rule out.
//
// **A place is made by opening one, and by nothing else.** A place somebody started making and walked
// away from is a box nobody can say anything about, so the folder is answered first
// (`../shell/FolderChoice`) and the frame is made after it.
//
// **A pane holds an order and a size, and never a place** (`AMB-D-939`). Where it is drawn is worked
// out afresh every render by laying the panes down in order, so there is no arrangement a person can
// leave a hole in the middle of and no pane that has to be put back after another one goes. A hole is
// what is left over when the next pane does not fit the room that is left — never something somebody
// made.
//
// **A page with room draws one empty frame, and never one per gap.** A box in every gap is the same
// question asked as many times as the room allows, so there is a single one and it sits exactly where
// the next pane would land (`landing`). It is the page saying it has room rather than a button for
// opening a terminal, which is why a full page draws none at all — the frames are what is on the
// screen, and the empty one is a remark about them.
//
// **Pages are how a project's panes go past a screenful, and nobody makes one.** They are the order
// cut where the room runs out: a page holds what fits, the pane that does not fit starts the next
// one, and nothing jumps a page to fill a hole behind it. No page stands empty: asking for another
// pane where every page is full is the one thing that brings one into being, and it lasts as long as
// the asking (`addPane`).

/**
 * How much of a page one pane takes (`AMB-D-939`).
 *
 * **Where the small end stops is settled by columns of text, not by how many a person can watch.**
 * What an agent's TUI wants is eighty columns, which at the pane's monospace is about 626px and 676px
 * with the room around it — so an eighth, drawn four across two rows, is what the widest screen sold
 * still draws readably, and a twelfth would put a pane under eighty on every screen there is.
 * Watching is not the limit any more: a pane that wants somebody says so on its own — the plate, the
 * badge on the page and the lamp all point at it — so eight of them need no watching over
 * (`./plate`).
 *
 * **Width is spent before height.** A terminal runs short of columns before it runs short of lines,
 * so every size but two is one row tall and the page is never cut into a third row.
 *
 * **A half comes two ways round, and that is the one place height is spent first.** Half a page side
 * by side halves the columns, and half of a window with a column beside it is under the eighty a TUI
 * wants; half laid down the page leaves the columns whole and takes the lines instead. They are two
 * sizes and not one size with an answer on it, because that is what lets one pane sit the way its
 * work wants while the pane beside it sits the other way.
 */
export type Size = "whole" | "half" | "half-down" | "quarter" | "sixth" | "eighth";

/** The sizes a person can pick, from the whole page down. */
export const SIZES: readonly Size[] = ["whole", "half", "half-down", "quarter", "sixth", "eighth"];

/** What a pane nobody has sized is drawn at — the whole page, which is what one pane on its own
 *  fills and what a project nobody has answered for opens at. */
export const DEFAULT_SIZE: Size = "whole";

/**
 * The cells a page is cut into, across and down.
 *
 * Twelve is the smallest number that every size divides: a quarter and a sixth and an eighth are
 * three and two and one and a half rows of it — so an eighth is three cells, a sixth four, a quarter
 * six. Two rows is the whole of the height, because no size asks for a third (`Size`).
 *
 * The grid itself is drawn in the stylesheet (`.workspace__page-grid`). This is the same shape said
 * where it can be checked: a box is placed against these numbers on every render, and a stylesheet
 * can only be read one rule at a time.
 */
export const ACROSS = 12;
export const DOWN = 2;

/** The rectangle a size takes on that grid. */
export type Box = { readonly across: number; readonly down: number };

/** What each size comes to in cells. Every one of them is `ACROSS * DOWN` divided by its name. */
export const BOXES: Readonly<Record<Size, Box>> = {
  whole: { across: 12, down: 2 },
  half: { across: 6, down: 2 },
  "half-down": { across: 12, down: 1 },
  quarter: { across: 6, down: 1 },
  sixth: { across: 4, down: 1 },
  eighth: { across: 3, down: 1 },
};

/**
 * The arrangement as it is handed over — the wire shape of `TalkLayoutDto`.
 *
 * **It is how the two windows share one face**, and it lasts as long as the app is up: whichever
 * window is drawing the face writes it, and the one the workspace is split out into reads it as it
 * comes up (`app/src-tauri/src/frames.rs`).
 *
 * **What outlives the run is the splits, `project` and the panes** (`AMB-D-869`). So an
 * arrangement read at the start of a run comes back with the places the reader left, on the project
 * they were looking at, at the sizes those panes were left at — and with nothing running in any of
 * them, because a session is a process and that one has ended.
 *
 * **It still speaks in splits, and a pane's size is not in it** (`laidOut`, `restored`). The store
 * and everything under it are moved to sizes in `AMB-T-5212`; until then this shape is the one the
 * host reads, so a project goes over as the split its first pane's size answers to and comes back
 * with every one of its panes at that size. A page of mixed sizes is the one thing that does not
 * survive the crossing.
 */
export type SavedLayout = {
  count: number;
  /** Which way a two-pane page sits, absent where it sits the way every other count does. It is kept
   *  at every count and not only at two: a person who went to four and asked for two again means the
   *  two they set up, not the default back. */
  orient?: "across" | "down";
  /** The split of each project that has one, by project — and the two above read at `project`, which
   *  is what the face was laid out from before sizes (`SplitDto`). A project nobody has answered for
   *  is not in it: what is kept is the answers, and a row for every project a reader ever walked
   *  through would say nothing about most of them. */
  splits?: Record<string, { count: number; orient?: "across" | "down" }>;
  /** The project whose panes the face was showing. It answers for the window the workspace was split
   *  out into, which has no ledger to have taken one from — and only where the arrangement came back
   *  with no panes in it, since a pane names its own project (`../shell/WorkspaceFace`). */
  project?: number;
  /** The panes, in the order they were opened — where each one is, what was started in it, and what
   *  has been written in the box under it. The draft is the one part that goes no further than the
   *  other window: a half-written sentence is the window's, and what the host writes down is the
   *  place (`app/src-tauri/src/frames.rs`), so an arrangement read at the start of a run carries
   *  panes and no drafts. */
  frames: {
    id: string;
    project?: number;
    folder?: string;
    agent?: string;
    written?: string;
    inserted?: string[];
    resumes?: boolean;
    composeOpen?: boolean;
  }[];
  /** The pane being worked in when the arrangement was last written. It is what the window split out
   *  of this face comes up on, so the reader lands where they left rather than on the first place of
   *  the first project (`AMB-D-753`). Read by that window and never by the board: which pane is
   *  being worked in *now* is the board's own state, and reading a written one back would move the
   *  person's place on the strength of an older write. */
  splitOut?: string;
};

/** One place a terminal is drawn, whether or not one is running in it. */
export type Frame = {
  /** Drawn once and never handed out again (`newFrameId`) — the id `./frames` keeps this frame's
   *  name against. */
  readonly id: string;
  /** The project this pane is one of. It is settled when the pane is made and never changes: a pane
   *  that could move between projects would be the one thing the rail promises cannot happen. */
  readonly project: number;
  /**
   * How much of a page this pane takes (`Size`).
   *
   * **It is the pane's and not the project's** (`AMB-D-939`). How much room a piece of work wants is
   * a fact about that piece of work — the agent and the shell it is watched from are not the same
   * size of thing — and one answer held for the whole project made every pane on it change together.
   */
  readonly size: Size;
  /** The terminal running here, or null for a frame whose program has ended. */
  readonly session: string | null;
  /** The folder that terminal works in — one of the folders its project is bound to. It is null for
   *  the one pane that takes up a terminal somebody else started: where that one runs was settled
   *  when it started, and the pane learns it from the session rather than from the person. */
  readonly folder: string | null;
  /**
   * The id the agent in this pane was started as — a catalogue row, or a command the reader
   * registered — and null for a plain prompt or a place nothing has been opened in yet.
   *
   * **It is read off the session and not off what was pressed for** (`./terminal`): a pane that
   * adopted a terminal never asked, and what is running in one was settled when it started. It rides
   * the arrangement because coming back to a pane means coming back to what was running in it —
   * which is the half of the row a folder cannot carry (`AMB-D-869`).
   */
  readonly agent: string | null;
  /**
   * Whether this place came back from the last run with a way into what was running in it
   * (`AMB-D-869`). The face opens those without being pressed, and asks nothing on the way in.
   *
   * **It is true of a place that came back, and of nothing else.** Opening a terminal here settles
   * it — the place has been come back to, and a page turned away from and back again is not a
   * second reason to start something. A place a person made in this run was never away.
   */
  readonly resumes: boolean;
  /**
   * What has been written in the box under this pane and not sent yet (`AMB-D-864`).
   *
   * **It is here rather than in the pane because the pane comes and goes and this must not.** A page
   * turned, a pane resized, a project moved to, the tasks face brought up — each of those takes the
   * drawing down and puts another one up later, and a half-written sentence kept in the drawing
   * would go with it. The terminal survives all of the same moves for the same kind of reason: what
   * is running is the host's and what is written is the window's, and the pane is only what draws
   * them (`AMB-D-753`).
   *
   * **It crosses to the other window with the arrangement, and stops there** (`laidOut`). The
   * arrangement is how the board and the window a terminal is split out into hand the face over, so
   * a draft left out of it would be one the person loses at exactly the press that moves their work
   * to the other screen. What goes on from there to the store is the splits and the project and
   * nothing else (`app/src-tauri/src/frames.rs`), so this is held for as long as the process is up
   * and is never written down.
   */
  readonly written: string;
  /**
   * The paths Amenbo itself put into the box under this pane, as far as they are still standing in
   * what is written (`../shell/TerminalPane`).
   *
   * **It is what the pane's agent is waited out for** (`./terminal`, `AMB-D-879`). Claude Code reads
   * a file a pasted body names, and drops a return that arrives while it is reading — so a send with
   * one of these still in it is left a longer pause than a send without. What is asked is whether
   * Amenbo put the path there, never whether the path looks like a picture: the agent's own way of
   * finding one is its own, and three of them disagree about it (`AMB-T-4719`).
   *
   * **It travels with the draft and is forgotten with it.** A path only means anything beside the
   * body it was put into, so the two move together — to the window a terminal is split out into, and
   * out of both on the send that empties the box. A path the person deleted while going on writing
   * is left here until then: what is asked at the send is whether the body still holds it, and this
   * is the list that is asked about (`./terminal`).
   */
  readonly inserted: readonly string[];
  /**
   * Whether the box under this pane is open (`AMB-D-890`).
   *
   * **It is here for the reason the draft is**: the pane comes and goes — a page turned, a pane
   * resized, the tasks face brought up, the workspace put in a window of its own — and a reader who
   * opened the box did not ask for it to shut at any of those. Kept in the drawing, it went down
   * with every one of them.
   *
   * **It goes on to the store, which the draft does not** (`app/src-tauri/src/frames.rs`). A
   * half-written sentence is the moment's and is better gone than stale; which panes a person writes
   * in is how they work, and does not go stale.
   *
   * **What a pane starts as is the machine's, and what it is now is this** (`AMB-D-889`,
   * `../core/composeStartsOpen`). The habit answers a pane being made and a row that came back
   * without this; after that the pane is the one the reader left. Read again later, a press in one
   * pane would fold another, and each fold wakes the program in it to repaint (`AMB-D-864`).
   */
  readonly composeOpen: boolean;
};

/** The arrangement of the workspace, as it stands. */
export type Layout = {
  readonly frames: readonly Frame[];
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
  project: null,
  page: 1,
  focus: null,
  adding: false,
};

/** Where one box sits: which page, and the cell of that page's grid its top left corner is in,
 *  counted from 0. */
export type Spot = { readonly page: number; readonly across: number; readonly down: number };

/** A pane and the spot it was laid down in. */
export type Placed = Spot & { readonly frame: Frame };

/** The cells of one page, as they fill up. */
function blank(): boolean[][] {
  return Array.from({ length: DOWN }, () => Array<boolean>(ACROSS).fill(false));
}

/** Whether a box put down here would land on cells that are all still free. */
function clear(taken: boolean[][], across: number, down: number, box: Box): boolean {
  for (let y = down; y < down + box.down; y += 1) {
    for (let x = across; x < across + box.across; x += 1) if (taken[y]![x]) return false;
  }
  return true;
}

/**
 * The first cell at or after `from` that a box still fits in, reading the top row left to right and
 * then the one under it, or null where it fits nowhere.
 *
 * **The search never goes back.** `from` is where the pane before this one was put, so a hole left
 * behind by a wider pane stays a hole rather than being filled by a narrower pane further down the
 * list. Backfilling would put the panes on the screen in an order that is not the order they are in,
 * which is the one thing a reader reads the pages as (`AMB-D-939`).
 */
function room(taken: boolean[][], box: Box, from: number): { across: number; down: number } | null {
  for (let cell = from; cell < ACROSS * DOWN; cell += 1) {
    const across = cell % ACROSS;
    const down = Math.floor(cell / ACROSS);
    if (across + box.across > ACROSS || down + box.down > DOWN) continue;
    if (clear(taken, across, down, box)) return { across, down };
  }
  return null;
}

/** That box written into the page. */
function fill(taken: boolean[][], at: { across: number; down: number }, box: Box) {
  for (let y = at.down; y < at.down + box.down; y += 1) {
    for (let x = at.across; x < at.across + box.across; x += 1) taken[y]![x] = true;
  }
}

/**
 * Sizes laid down in order, each one answered with the spot it landed in.
 *
 * **A size that does not fit what is left starts the next page, and everything after it follows.**
 * The page is not held open for a smaller pane further down the list: a pane that jumped a page to
 * fill a hole behind it would put the order on the screen out of the order it is in, which is the one
 * thing the reader is reading the pages as (`AMB-D-939`). So the holes are wherever the fitting ran
 * out, and every page is a run of the list.
 */
function spots(sizes: readonly Size[]): Spot[] {
  const out: Spot[] = [];
  let page = 1;
  let taken = blank();
  // Where the pane before this one was put, which is where the search for this one starts (`room`).
  let from = 0;
  for (const size of sizes) {
    const box = BOXES[size];
    let at = room(taken, box, from);
    if (at === null) {
      page += 1;
      taken = blank();
      // Every size fits an empty page, so this one cannot come back null.
      at = room(taken, box, 0)!;
    }
    fill(taken, at, box);
    out.push({ page, across: at.across, down: at.down });
    from = at.down * ACROSS + at.across;
  }
  return out;
}

/** These panes laid down in order (`spots`). */
export function placing(panes: readonly Frame[]): readonly Placed[] {
  const laid = spots(panes.map((one) => one.size));
  return panes.map((frame, at) => ({ frame, ...laid[at]! }));
}

/** Where a box of this size sits on the grid, as the stylesheet is handed it. Both are one-based,
 *  which is how CSS counts grid lines. */
export function gridAt(size: Size, across: number, down: number): {
  readonly gridColumn: string;
  readonly gridRow: string;
} {
  const box = BOXES[size];
  return {
    gridColumn: `${across + 1} / span ${box.across}`,
    gridRow: `${down + 1} / span ${box.down}`,
  };
}

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
  const laid = placing(panesOf(layout, layout.project));
  return Math.max(1, laid[laid.length - 1]?.page ?? 1);
}

/**
 * The pane the size control is about (`AMB-D-939`).
 *
 * **It is one of the panes on the page being read** — the pane being worked in where that is on this
 * page, and the last pane of the page where it is not. A pane the reader cannot see is not one they
 * can be resizing, and a page with no panes on it has nothing to be about.
 */
export function sizing(layout: Layout): Frame | null {
  const here = slotsOf(layout, layout.page);
  return here.find((one) => one.frame.id === layout.focus)?.frame
    ?? here[here.length - 1]?.frame
    ?? null;
}

/**
 * The size a pane opened on this page would be: the pane the size control is about (`sizing`), or,
 * on a page brought into being by `addPane` and so having no panes at all, the last pane of the
 * project. The first pane of a project has nothing to be measured against and takes the whole page.
 */
function sizeOn(layout: Layout, page: number): Size {
  const here = slotsOf(layout, page);
  const panes = panesOf(layout, layout.project);
  const one = here.find((each) => each.frame.id === layout.focus)?.frame
    ?? here[here.length - 1]?.frame
    ?? panes[panes.length - 1];
  return one?.size ?? DEFAULT_SIZE;
}

/** How many of this project's panes are laid down by the end of this page, which is where a pane
 *  opened on it goes in the order. */
function upTo(layout: Layout, page: number): number {
  return placing(panesOf(layout, layout.project)).filter((one) => one.page <= page).length;
}

/**
 * Where a pane opened on this page would land, and at what size — or null where the page has no room
 * left for one.
 *
 * **It goes after the last pane of this page, and is looked for from where that pane was put**
 * (`room`). So the panes already on the page do not move for it: everything before it keeps the place
 * it has, and everything after it is on a later page. It is the same search a real pane goes through,
 * which is what lets the empty frame stand exactly where the pane it offers will be.
 */
export function landingOn(
  layout: Layout,
  page: number,
): { readonly across: number; readonly down: number; readonly size: Size } | null {
  const panes = panesOf(layout, layout.project);
  const at = upTo(layout, page);
  const size = sizeOn(layout, page);
  const got = spots([...panes.slice(0, at).map((one) => one.size), size])[at]!;
  return got.page === page ? { across: got.across, down: got.down, size } : null;
}

/**
 * Whether this page of the shown project has room for another pane.
 *
 * It is what the empty frame is drawn from: a page a pane still fits on says so with one, and a page
 * it does not fit says nothing. So it is about the pane that would be made and not about whether a
 * hole is left anywhere — a hole too small for the next pane is not room (`landingOn`).
 */
export function roomOnPage(layout: Layout, page: number): boolean {
  return landingOn(layout, page) !== null;
}

/** The panes drawn on one page, each with the spot it was laid down in. There is one per pane and no
 *  more: what is not open is not a box on the screen. */
export function slotsOf(layout: Layout, page: number): readonly Placed[] {
  return placing(panesOf(layout, layout.project)).filter((one) => one.page === page);
}

/** The page a frame is on, within its own project, or null for an id no frame has. */
export function pageOfFrame(layout: Layout, frame: string): number | null {
  const one = layout.frames.find((each) => each.id === frame);
  if (!one) return null;
  const at = placing(panesOf(layout, one.project)).find((each) => each.frame.id === frame);
  return at?.page ?? null;
}

/** The frame a session is running in, or null where none is. */
export function frameOfSession(layout: Layout, session: string): Frame | null {
  return layout.frames.find((one) => one.session === session) ?? null;
}

/**
 * The pane of this project already working in `folder`, or null where none is.
 *
 * **A folder already open is not opened beside itself.** It is what a folder handed in from the
 * ledger lands on (`../shell/WorkspaceFace`): pressing the first loop's one button on a project whose
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
 * A frame id, drawn afresh.
 *
 * **Drawn and not counted** (`AMB-D-897`). A count has to be kept somewhere to be counted from, and
 * the only place it could be kept was the row the panes are in — so an arrangement that would not
 * parse was dropped whole and the count began at the first id again, onto ids a pane's name and the
 * way back into its session were already held against. Sixteen bytes of the machine's randomness
 * cannot do that, whatever becomes of the row.
 *
 * What comes out is a UUID and not merely something unique: that is the shape the host reads a
 * pane's home by (`app/src-tauri/src/pane_home.rs`), and the same one it writes a session handle in
 * (`amenbo_core::harness::uuid_v4`). The window is served over a scheme the host registers as a
 * secure one, which is what `randomUUID` is offered by — the same thing the clipboard is read
 * through on Linux (`../core/clipFiles`).
 */
export function newFrameId(): string {
  return crypto.randomUUID();
}

/**
 * Make a place for a terminal in this project, and answer with the layout and the frame.
 *
 * It is called once the folder has been answered for — a pane is made by opening one, so there is no
 * moment where a frame exists with the question still on it. The new pane is the one being worked in
 * and the screen moves to the page it landed on, because a person who opened a pane is looking at it.
 *
 * **It goes in at the end of the page the reader is on, and at that page's size** (`landingOn`,
 * `AMB-D-939`): where the empty frame was drawn is where the pane appears, so the press lands the
 * pane where the reader was looking rather than at the far end of the arrangement. A pane opened on
 * a project the face is not showing has no page to land on, so it goes at the end of that project's
 * list at the size of its last pane.
 *
 * **`id` is the one place a frame does not name itself.** A pane being opened again from a record
 * comes back under the id it had (`AMB-D-897`): the id is what a provider's own home is named after,
 * so a place opened again under a new one would be a different place (`crate::pane_home`).
 */
export function openedFrame(
  layout: Layout,
  project: number,
  folder: string | null,
  composeOpen = false,
  id = newFrameId(),
): { layout: Layout; frame: Frame } {
  const here = project === layout.project;
  const panes = panesOf(layout, project);
  const at = here ? upTo(layout, layout.page) : panes.length;
  const size = here ? sizeOn(layout, layout.page) : panes[panes.length - 1]?.size ?? DEFAULT_SIZE;
  const frame: Frame = {
    id,
    project,
    size,
    session: null,
    folder,
    agent: null,
    // Made in this run, so there is nothing to come back into.
    resumes: false,
    written: "",
    inserted: [],
    // The one moment the machine's habit is asked (`Frame.composeOpen`). The caller is what knows it
    // — this module reads nothing of its own — and a caller that does not say opens the pane folded,
    // which is where `AMB-D-889` starts one.
    composeOpen,
  };
  // Where that place in this project's own list falls in the one list every project's frames share.
  const places = layout.frames.flatMap((one, i) => (one.project === project ? [i] : []));
  const into = at < places.length
    ? places[at]!
    : (places[places.length - 1] ?? layout.frames.length - 1) + 1;
  const next_: Layout = {
    ...layout,
    frames: [...layout.frames.slice(0, into), frame, ...layout.frames.slice(into)],
    project,
    // The page asked for has a pane on it now, so it is a page like any other (`addPane`).
    adding: false,
  };
  return { layout: focusOn(next_, frame.id), frame };
}

/** A terminal has started in a frame. The folder and the agent are the ones the session says it was
 *  started with, which a pane that took one up learns here and nowhere else. */
/**
 * What is written in the box under a pane, as far as it has been written (`Frame.written`).
 *
 * Emptied by the send, which is what makes the next sentence a sentence of its own rather than the
 * tail of the one before it, and left alone by everything else — a pane taken down for a page turn
 * has not been written in, it has been put away.
 *
 * `put` names the paths Amenbo has just put into the body, where this write is one of those
 * (`Frame.inserted`). They are let go of when the box is emptied and kept through everything else:
 * an empty box holds nothing, and whether one is still standing in a body that is not empty is asked
 * at the send, which is where the answer is wanted and where the quoting is understood (`./terminal`).
 */
export function writing(
  layout: Layout,
  frame: string,
  written: string,
  put: readonly string[] = [],
): Layout {
  return withFrame(layout, frame, (was) => ({
    ...was,
    written,
    inserted:
      written === ""
        ? []
        : [...was.inserted, ...put.filter((path) => !was.inserted.includes(path))],
  }));
}

/**
 * The box under a pane opened or folded away, which is the press on that pane's own band
 * (`Frame.composeOpen`).
 *
 * **It moves this pane and no other.** What the next pane will start as is the machine's answer and
 * is written down beside the theme (`../core/composeStartsOpen`); a pane already on the screen is
 * left where its reader put it, the box taking room from the terminal above it.
 */
export function folding(layout: Layout, frame: string, open: boolean): Layout {
  return withFrame(layout, frame, (was) => ({ ...was, composeOpen: open }));
}

export function openedIn(
  layout: Layout,
  frame: string,
  session: string,
  folder: string | null,
  agent: string | null,
): Layout {
  // And the place has been come back to, whatever it came back holding: what happens in it now is
  // this run's, and a page turned away from and back again must not start a second terminal here
  // (`Frame.resumes`).
  return withFrame(layout, frame, (was) => ({
    ...was,
    session,
    folder: folder ?? was.folder,
    agent,
    resumes: false,
  }));
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
 * **What is left closes up.** The panes are one list and the pages are that list laid down in order,
 * so the panes after the closed one move up into the room it gave back and the last page may lose
 * one. That is not the screen rearranging itself under a reader — the promise is that what is open
 * moves only when a person presses (`AMB-D-939`), and this moved because they asked for it.
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
 * **The asking is what makes a page**, and it is the only thing that does. Where the next pane still
 * fits the last page this is only a move — that page's empty frame is already the one being pressed
 * towards. Where it fits nowhere there is no page to put the question on, so one comes into being to
 * hold it, and it lasts exactly as long as the person stays on it (`adding`).
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
 * Give one pane a different size.
 *
 * **Everything after it in the order moves, and nothing before it does.** The panes are one list laid
 * down in order, so a pane that grew takes room the ones behind it were in and pushes the overflow
 * onto the next page, and a pane that shrank lets them come back up. That is the cost of holding no
 * places (`AMB-D-939`), and it happens only on a press.
 *
 * **The reader follows the pane they resized**, wherever laying the list down again has put it: the
 * press was about that pane, so the page it is on now is the page to be looking at.
 */
export function resized(layout: Layout, frame: string, size: Size): Layout {
  const was = layout.frames.find((one) => one.id === frame);
  if (!was || was.size === size) return layout;
  const next: Layout = {
    ...withFrame(layout, frame, (one) => ({ ...one, size })),
    adding: false,
  };
  const page = pageOfFrame(next, frame);
  return { ...next, page: Math.min(page ?? layout.page, pageCount(next)) };
}

/**
 * One pane put before or after another, within the list its project's panes make.
 *
 * **The list is the whole of what a reorder moves.** The pages are that list laid down in order
 * (`placing`), so a pane carried onto another page is the same move as one carried across a page —
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

/** The split a size answers to, for as long as the shape handed over speaks in splits
 *  (`SavedLayout`). */
const SPLIT_OF: Readonly<Record<Size, { count: number; orient?: "across" | "down" }>> = {
  whole: { count: 1 },
  half: { count: 2 },
  "half-down": { count: 2, orient: "down" },
  quarter: { count: 4 },
  sixth: { count: 6 },
  eighth: { count: 8 },
};

/** And the size a split comes back as (`AMB-D-939`). A count this build has no size for is read as
 *  no answer at all: it was written by a build that offered some other split, and a pane put back at
 *  a guess would be this one inventing what the reader left. */
function sizeOfSplit(count: number, orient?: string): Size | null {
  if (count === 2) return orient === "down" ? "half-down" : "half";
  return SIZES.find((size) => size !== "half-down" && SPLIT_OF[size].count === count) ?? null;
}

/**
 * The arrangement as it is written down, for the other window to read.
 *
 * **What is written is the shape**: the split each project has been answered at, the panes in the
 * order they were opened, and for each the project it is one of, the folder it is working in, what
 * was started in it and whatever is written in the box under it. What is running is not — a session
 * is a process, and a pane drawn as though one were still in it would be the window saying something
 * untrue. So a pane comes over as a place, and nothing is started until somebody presses.
 *
 * **A project goes over as one split, which is the size of its first pane** (`SavedLayout`). The
 * shape the host reads has no room for a size on a pane, so a page of mixed sizes is the one thing
 * that cannot be said in it — until `AMB-T-5212` moves the store to sizes, what comes back is the
 * whole project at the size the first pane was left at.
 *
 * **The draft is here and the session is not, for the same reason in either direction.** A sentence
 * somebody is part-way through writing is theirs and exists nowhere else, so it has to travel with
 * the pane; a process belongs to the host, so it must not be drawn as though it travelled at all.
 *
 * **The project this face is on goes with it**, and is the one part of the shape this face never
 * reads back (`restored`). It is written for the window with no ledger: a terminal split out into
 * one of its own has nobody to have asked it which project it is drawing, so an arrangement with no
 * panes in it opens as the project the board was on (`../talk.tsx`).
 */
export function laidOut(layout: Layout): SavedLayout {
  const first = new Map<number, Size>();
  for (const frame of layout.frames) if (!first.has(frame.project)) first.set(frame.project, frame.size);
  const splits = [...first].map(([project, size]) => [String(project), SPLIT_OF[size]] as const);
  const opening = SPLIT_OF[(layout.project === null ? undefined : first.get(layout.project)) ?? DEFAULT_SIZE];
  return {
    count: opening.count,
    ...(opening.orient === undefined ? {} : { orient: opening.orient }),
    ...(splits.length === 0 ? {} : { splits: Object.fromEntries(splits) }),
    ...(layout.project === null ? {} : { project: layout.project }),
    frames: layout.frames.map((frame) => ({
      id: frame.id,
      project: frame.project,
      ...(frame.folder === null ? {} : { folder: frame.folder }),
      // What was started in it, left out where nothing has been: a place nobody has opened anything
      // in has nothing to come back to.
      ...(frame.agent === null ? {} : { agent: frame.agent }),
      // Left out where the box is empty, the way the folder is: what is written down is what there
      // is to say, and an empty box has nothing.
      ...(frame.written === "" ? {} : { written: frame.written }),
      // And what Amenbo put into that body goes with it, for the same reason: the two mean nothing
      // apart, and the window this is written for is the one the draft is being carried to
      // (`Frame.inserted`).
      ...(frame.inserted.length === 0 ? {} : { inserted: [...frame.inserted] }),
      // Written both ways round rather than left out on one of them: this is an answer with two
      // sides, and a row missing it is a row that predates it — which the reading back answers with
      // the machine's habit, and would answer a folded pane with on the day the habit is to open
      // (`restored`).
      composeOpen: frame.composeOpen,
    })),
    // The pane being worked in, written down for the window the workspace is split out into: the
    // press says nothing, so where the reader was is theirs to read back out of the shape.
    ...(layout.focus === null ? {} : { splitOut: layout.focus }),
  };
}

/**
 * The layout an arrangement comes back as.
 *
 * **The places come back and nothing is running in any of them** (`AMB-D-869`). A window that comes
 * up after a run reads the panes the reader left — each with its folder and what was started in it —
 * on the project they were on, at the size the split that project was left at answers to
 * (`sizeOfSplit`). An arrangement with no panes in it says nothing at all any more: a size is a fact
 * about a pane, so a project with no panes has nobody to have answered for it.
 *
 * `onto` is the project the window is on, and it answers for the frames an older build wrote without
 * one: a pane whose project nothing records is put where the person is rather than dropped, and where
 * there is nowhere to put it there is nothing to draw.
 */
export function restored(saved: SavedLayout, onto: number | null, composeOpen = false): Layout {
  const kept = saved.splits ?? {};
  const frames: Frame[] = [];
  for (const frame of saved.frames) {
    const project = frame.project ?? onto;
    if (project === null) continue;
    // The split this project was left at, or the pair beside the splits where it has no row of its
    // own — which is what an arrangement written before the answers were kept by project has, and
    // all it has.
    const split = kept[String(project)] ?? { count: saved.count, orient: saved.orient };
    // The box comes over as it was left, which is what carries a half-written sentence to the window
    // the workspace is split out into (`Frame.written`). An arrangement that came from the store has
    // no frames in it at all, so a run that has just started has nothing here to take.
    frames.push({
      id: frame.id,
      project,
      size: sizeOfSplit(split.count, split.orient) ?? DEFAULT_SIZE,
      session: null,
      folder: frame.folder ?? null,
      agent: frame.agent ?? null,
      // Only the host can answer this, and only for an arrangement that came out of the store: what
      // it stands for is a handle no window holds (`crate::frames`). The arrangement the other
      // window sends carries none, and a place in it is one this run has already opened.
      resumes: frame.resumes === true && frame.folder !== undefined && frame.agent !== undefined,
      written: frame.written ?? "",
      // Kept only where the body it was put into came too: an empty box holds nothing, so a path
      // remembered over one would have the next send wait out an agent with no file to read.
      inserted: (frame.written ?? "") === "" ? [] : (frame.inserted ?? []),
      // The box as the reader left it (`AMB-D-890`). A row from before this was kept has no answer
      // of its own, so it opens on the machine's habit — which is what every pane did until then.
      composeOpen: frame.composeOpen ?? composeOpen,
    });
  }
  const first = frames[0];
  return {
    frames,
    project: first?.project ?? onto,
    page: 1,
    focus: first?.id ?? null,
    adding: false,
  };
}
