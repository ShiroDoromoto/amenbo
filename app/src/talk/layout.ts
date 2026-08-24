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
// outlive the session that earned it (`./frames`). Ids are handed out once and never reused: a name
// is kept against the id, so a reused one would put an old name on a new place.
//
// **A place is made by opening one, and by nothing else.** There are no empty slots waiting to be
// filled: a screen of four boxes all asking the same question is the question asked four times, and a
// place somebody started making and walked away from is a box nobody can say anything about. So the
// folder is answered first (`../shell/FolderChoice`) and the frame is made after it — a project with
// no panes draws one way in, in the middle of the face, and nothing else.
//
// **Pages are how a project's panes go past a screenful.** They are fixed slots in the sense that
// matters: what is open does not move about on its own. The count is the most that are drawn at once
// and not a number of boxes to fill, so a page holds up to that many and the last one holds what is
// left.

/** Panes on one screen. Three steps and no more: the reason to have several is to watch them, and a
 *  count that keeps going stops being watchable long before it stops fitting. */
export type Count = 1 | 2 | 4;

/** The counts a person can pick, in the order they are offered. */
export const COUNTS: readonly Count[] = [1, 2, 4];

/** Two panes to start with: one is what a single terminal already was, and four is a screenful to
 *  arrive at rather than to be given. */
export const DEFAULT_COUNT: Count = 2;

/** How many pages a ⌘-digit reaches. A project with more than this many pages is not out of reach —
 *  every pane has a row on the rail, and picking one brings its page up — but the keyboard stops
 *  here, because there are only nine digits to press. */
export const MAX_PAGES = 9;

/** The width below which the columns beside the panes stop being columns (see `sidesAreDrawers`). */
export const NARROW_PX = 900;

/** The arrangement as the store keeps it — the wire shape of `TalkLayoutDto`. */
export type SavedLayout = {
  count: number;
  nextId: number;
  frames: { id: string; project?: number; folder?: string }[];
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
};

export const EMPTY_LAYOUT: Layout = {
  frames: [],
  nextId: 1,
  count: DEFAULT_COUNT,
  project: null,
  page: 1,
  focus: null,
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
 * the page it shows is where the way in is put.
 */
export function pageCount(layout: Layout): number {
  const panes = panesOf(layout, layout.project).length;
  return Math.max(1, Math.ceil(panes / layout.count));
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
  return { ...layout, project, page: 1, focus: first?.id ?? null };
}

/** Show a page of the project that is up, as far as there are pages to show. */
export function goPage(layout: Layout, page: number): Layout {
  if (page < 1 || page > pageCount(layout)) return layout;
  return { ...layout, page };
}

/** Work in a frame, bringing its project and page up with it — the rail's rows reach panes that are
 *  not on the screen, and reaching one has to show it. */
export function focusOn(layout: Layout, frame: string): Layout {
  const one = layout.frames.find((each) => each.id === frame);
  if (!one) return layout;
  const shown: Layout = { ...layout, project: one.project };
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
  const next: Layout = { ...layout, count };
  const page = next.focus === null ? null : pageOfFrame(next, next.focus);
  return { ...next, page: Math.min(page ?? layout.page, pageCount(next)) };
}

/**
 * The arrangement as it is kept between runs, and as it comes back.
 *
 * **What is kept is the shape**: how many panes to a page, the panes in the order they were opened,
 * and for each the project it is one of and the folder it was working in. What was running is not — a
 * session died with the last run, and a pane drawn as though it were still there would be the window
 * saying something untrue. So a restored frame comes back as a place with its last folder on it, and
 * nothing is started until somebody presses.
 */
export function laidOut(layout: Layout): SavedLayout {
  return {
    count: layout.count,
    nextId: layout.nextId,
    frames: layout.frames.map((frame) => ({
      id: frame.id,
      project: frame.project,
      ...(frame.folder === null ? {} : { folder: frame.folder }),
    })),
  };
}

/**
 * The layout a kept arrangement comes back as, or nothing where it says nothing.
 *
 * `onto` is the project the window is on, and it answers for the frames an older build wrote without
 * one: a pane whose project nothing records is put where the person is rather than dropped, and where
 * there is nowhere to put it there is nothing to draw. An arrangement with no frames left is nothing
 * to come back to — what would be restored is the empty face the window makes for itself anyway.
 */
export function restored(saved: SavedLayout, onto: number | null): Layout | null {
  const count = COUNTS.find((one) => one === saved.count) ?? DEFAULT_COUNT;
  const frames: Frame[] = [];
  for (const frame of saved.frames) {
    const project = frame.project ?? onto;
    if (project === null) continue;
    frames.push({ id: frame.id, project, session: null, folder: frame.folder ?? null });
  }
  if (frames.length === 0) return null;
  const first = frames[0]!;
  return {
    frames,
    // Ids are never reused, so the next one has to clear every frame that came back — a kept
    // arrangement written by a newer build, or an id list nobody can vouch for, must not hand a
    // fresh frame the name of an old one.
    nextId: Math.max(saved.nextId, ...frames.map((frame) => Number(frame.id) + 1 || 0)),
    count,
    project: first.project,
    page: 1,
    focus: first.id,
  };
}

/**
 * Whether the columns beside the panes are drawers rather than columns.
 *
 * One pane is a person saying they want the screen for it, and a narrow window has not got the room
 * for three things across whatever anyone asked for. Both answers are the same answer, so both are
 * given here: what changes is whether the rail and what sits on the other side are always there or
 * are opened when they are wanted.
 */
export function sidesAreDrawers(count: Count, width: number): boolean {
  return count === 1 || width < NARROW_PX;
}
