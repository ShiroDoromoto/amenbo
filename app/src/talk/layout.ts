// How the panes of the terminal face are arranged: pages, the slots on one, and how many there are.
//
// **A page is a fixed set of slots and nothing is held back.** Every frame this device has is in a
// slot on some page — there is no reserve behind the screen and no dragging one pane over another.
// Getting to a pane that is not on screen is paging to it (⌘1〜9) or picking it off the rail, both of
// which move the whole screen at once. A layout where panes can be shuffled has to be remembered
// before it can be trusted, and a person who cannot predict where a pane will be stops looking for it.
//
// **A frame is a place, not a process.** It exists before a terminal is started in it and stays when
// one ends, which is what lets a slot show a way to open one and what lets a name outlive the session
// that earned it (`./frames`). Ids are handed out once and never reused: a name is kept against the
// id, so a reused one would put an old name on a new place.
//
// **One page never mixes projects.** A frame opened on a page opens in that page's folder — the
// folder of the first frame started there — so panes on one screen are in one project by
// construction rather than by a check. A different project is a different page. What is *shown* about
// that, and the way to start a page somewhere chosen, arrive with the frames that are restored
// (`AMB-T-3607`) and the agent that is picked (`AMB-T-3591`).
//
// Nothing here is persisted. What a device keeps of its arrangement is `AMB-T-3607`'s to add; this is
// the shape it will keep.

/** Panes on one screen. Three steps and no more: the reason to have several is to watch them, and a
 *  count that keeps going stops being watchable long before it stops fitting. */
export type Count = 1 | 2 | 4;

/** The counts a person can pick, in the order they are offered. */
export const COUNTS: readonly Count[] = [1, 2, 4];

/** Two panes to start with: one is what a single terminal already was, and four is a screenful to
 *  arrive at rather than to be given. */
export const DEFAULT_COUNT: Count = 2;

/** How many pages there can be, which is how many ⌘-digits there are to reach them with. A page
 *  nobody can jump to is a pane nobody finds. */
export const MAX_PAGES = 9;

/** The width below which the columns beside the panes stop being columns (see `sidesAreDrawers`). */
export const NARROW_PX = 900;

/** The arrangement as the store keeps it — the wire shape of `TalkLayoutDto`. */
export type SavedLayout = {
  count: number;
  nextId: number;
  frames: { id: string; folder?: string }[];
};

/** One place a terminal is drawn, whether or not one is running in it. */
export type Frame = {
  /** Handed out once and never reused — the id `./frames` keeps this frame's name against. */
  readonly id: string;
  /** The terminal running here, or null for a frame nothing has been started in yet. */
  readonly session: string | null;
  /** The folder that terminal was started in. It is the page's folder: every frame opened on a page
   *  takes the folder of the first one started there, which is what keeps one screen to one project. */
  readonly folder: string | null;
};

/** The arrangement of the terminal face, as it stands. */
export type Layout = {
  readonly frames: readonly Frame[];
  /** The next id to hand out. Frames are never renumbered, so this only ever goes up. */
  readonly nextId: number;
  readonly count: Count;
  /** The page being shown, counted from 1. */
  readonly page: number;
  /** The frame the person is working in, or null before they have picked one. */
  readonly focus: string | null;
};

export const EMPTY_LAYOUT: Layout = {
  frames: [],
  nextId: 1,
  count: DEFAULT_COUNT,
  page: 1,
  focus: null,
};

/** Where the slot sits in the one list of frames. */
function indexOf(count: Count, page: number, slot: number): number {
  return (page - 1) * count + slot;
}

/**
 * How many pages there are.
 *
 * Always one more than the frames fill, so there is somewhere to put the next pane — a person who
 * cannot page past the last full screen has no way to start a pane at all. It stops at `MAX_PAGES`,
 * where the digits run out.
 */
export function pageCount(layout: Layout): number {
  return Math.min(Math.floor(layout.frames.length / layout.count) + 1, MAX_PAGES);
}

/** The frames on one page, one entry per slot. A slot with no frame yet reads as null and is drawn
 *  the same way a frame with no terminal is: as a place to open one. */
export function slotsOf(layout: Layout, page: number): readonly (Frame | null)[] {
  return Array.from({ length: layout.count }, (_, slot) =>
    layout.frames[indexOf(layout.count, page, slot)] ?? null);
}

/**
 * The first slot on `page` with no frame in it, or null where the page is full.
 *
 * It is what the rail's way in asks twice: once to know whether to offer itself at all — a control
 * that cannot do anything is one a reader learns to ignore — and once to know where the pane goes.
 * Both have to be the same answer, which is why it is one function.
 *
 * There is always at least one page with a free slot, because the arrangement offers one more page
 * than the frames fill (`pageCount`).
 */
export function freeSlot(layout: Layout, page: number): number | null {
  const at = slotsOf(layout, page).findIndex((frame) => frame === null);
  return at < 0 ? null : at;
}

/** The page a frame is on, or null for an id no frame has. */
export function pageOfFrame(layout: Layout, frame: string): number | null {
  const at = layout.frames.findIndex((one) => one.id === frame);
  return at < 0 ? null : Math.floor(at / layout.count) + 1;
}

/** The frame a session is running in, or null where none is. */
export function frameOfSession(layout: Layout, session: string): Frame | null {
  return layout.frames.find((one) => one.session === session) ?? null;
}

/**
 * The folder a page's panes are in: the first one started there.
 *
 * It is what a pane opened on this page is opened in, and it is why one screen is one project. A
 * page nothing has been started on has no folder, and the first terminal opened there settles it.
 */
export function folderOfPage(layout: Layout, page: number): string | null {
  for (let slot = 0; slot < layout.count; slot++) {
    const frame = layout.frames[indexOf(layout.count, page, slot)];
    if (frame?.folder != null) return frame.folder;
  }
  return null;
}

/**
 * The page a folder handed in from the ledger is worked in — the one already in it, or the first one
 * nothing has been started on. Null only where every page is settled in some other folder.
 *
 * **A folder already open is not opened beside itself.** One screen is one project ({@link
 * folderOfPage}), so a second page in the same folder would be that project twice with its panes
 * split between them — the reader would go looking for the pane they left in the one they are not on.
 *
 * Where it is not open yet, the page that takes it is one with nothing settled, because settling is
 * what a page's folder is: putting it on a page that has one would make that page two projects. On a
 * face nobody has started anything on that is the first page, which is why the ordinary first run
 * lands on the screen the reader is already looking at.
 */
export function pageFor(layout: Layout, folder: string): number | null {
  for (let page = 1; page <= MAX_PAGES; page++) {
    if (folderOfPage(layout, page) === folder) return page;
  }
  for (let page = 1; page <= MAX_PAGES; page++) {
    if (folderOfPage(layout, page) === null) return page;
  }
  return null;
}

/**
 * A frame on `page` with `folder` already settled on it, for a folder handed in from the ledger.
 *
 * **The frame is a new one where the slot's is empty**, and that is the point rather than an
 * accident: a frame is drawn from the moment it is put up, so a folder settled underneath one that
 * is already on the screen would leave its invitation standing over a page that now has an answer.
 * A fresh id is what brings the pane up again. Nothing is lost by it — a frame with no session is a
 * place and nothing more, and the one thing a place carries of its own, its name, is given to frames
 * a person has worked in (`./frames`).
 *
 * A slot with a terminal running in it is left exactly as it is: what is running there was started
 * somewhere, and replacing it would be this taking a session away from whoever is in it.
 */
export function openedOn(layout: Layout, page: number, folder: string): { layout: Layout; frame: Frame } {
  const made = frameFor(layout, page, 0);
  const at = indexOf(made.layout.count, page, 0);
  const standing = made.layout.frames[at]!;
  if (standing.session !== null) return { layout: made.layout, frame: standing };
  const frame: Frame = { id: String(made.layout.nextId), session: null, folder };
  const frames = [...made.layout.frames];
  frames[at] = frame;
  return { layout: { ...made.layout, frames, nextId: made.layout.nextId + 1 }, frame };
}

/**
 * Make sure a slot has a frame, and answer with the layout and the frame at it.
 *
 * Slots are filled up to the one asked for. Reaching for the fourth slot of an empty page is a person
 * saying they want a pane there, and the three before it are places on the same screen — drawn as
 * ways to open a terminal, which is what a frame with nothing running in it is.
 */
export function frameFor(layout: Layout, page: number, slot: number): { layout: Layout; frame: Frame } {
  const at = indexOf(layout.count, page, slot);
  const existing = layout.frames[at];
  if (existing) return { layout, frame: existing };
  const frames = [...layout.frames];
  let nextId = layout.nextId;
  while (frames.length <= at) {
    frames.push({ id: String(nextId), session: null, folder: null });
    nextId++;
  }
  return { layout: { ...layout, frames, nextId }, frame: frames[at]! };
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
 * The folder a frame works in, settled before anything has been started in it.
 *
 * Choosing a folder is what a frame with none offers, and it settles the page there and then — not
 * when a terminal finally starts in it (`./agent`). The two are usually a moment apart and sometimes
 * never join: on a machine with no agent to start, the pane says so instead of opening, and a page
 * that took its folder from a started terminal would go on asking every other slot where to work.
 *
 * A frame that already has one keeps it. The page's folder is where its first pane was settled, and a
 * second answer would be a screen that quietly changed project.
 */
export function settledIn(layout: Layout, frame: string, folder: string): Layout {
  return withFrame(layout, frame, (was) => (was.folder === null ? { ...was, folder } : was));
}

/** A terminal has started in a frame. The folder is the one it was started in, which is the page's
 *  from the second pane onwards. */
export function openedIn(layout: Layout, frame: string, session: string, folder: string | null): Layout {
  return withFrame(layout, frame, (was) => ({ ...was, session, folder: folder ?? was.folder }));
}

/** The folder an agent says it is in now. It moves with its own `cd`, and the page's is the folder a
 *  pane was *started* in, so this is recorded on the frame without being allowed to redraw the page's
 *  ancestry: only a frame with nothing settled yet takes one. */
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

/** Show a page, as far as there are pages to show. */
export function goPage(layout: Layout, page: number): Layout {
  const last = pageCount(layout);
  if (page < 1 || page > last) return layout;
  return { ...layout, page };
}

/** Work in a frame, bringing its page up with it — the rail's rows reach panes that are not on
 *  screen, and reaching one has to show it. */
export function focusOn(layout: Layout, frame: string): Layout {
  const page = pageOfFrame(layout, frame);
  return page === null ? layout : { ...layout, page, focus: frame };
}

/**
 * Show a different number of panes.
 *
 * The frames do not move: they are one list, and the count is how much of it a page shows. What has
 * to be carried across is the pane being worked in — a person who asks for one pane means the one
 * they were looking at — so the page follows the focus rather than the number.
 */
export function setCount(layout: Layout, count: Count): Layout {
  const next: Layout = { ...layout, count };
  const page = next.focus === null ? null : pageOfFrame(next, next.focus);
  return { ...next, page: Math.min(page ?? layout.page, pageCount(next)) };
}

/**
 * The arrangement as it is kept between runs, and as it comes back (`AMB-T-3607`).
 *
 * **What is kept is the shape**: how many panes to a page, the frames in slot order, and the folder
 * each was working in. What was running is not — a session died with the last run, and a pane drawn
 * as though it were still there would be the window saying something untrue. So a restored frame
 * comes back as a place to open a terminal in, and nothing is started until somebody presses.
 */
export function laidOut(layout: Layout): SavedLayout {
  return {
    count: layout.count,
    nextId: layout.nextId,
    frames: layout.frames.map((frame) => ({
      id: frame.id,
      ...(frame.folder === null ? {} : { folder: frame.folder }),
    })),
  };
}

/**
 * The layout a kept arrangement comes back as, or nothing where it says nothing.
 *
 * An arrangement with no frames in it is nothing to come back to: what would be restored is the
 * empty face the window makes for itself anyway. The focus lands on the first frame, because a face
 * has to be working somewhere and the first place is where a fresh one starts too.
 */
export function restored(saved: SavedLayout): Layout | null {
  const count = COUNTS.find((one) => one === saved.count) ?? DEFAULT_COUNT;
  const frames = saved.frames.map((frame) => ({
    id: frame.id,
    session: null,
    folder: frame.folder ?? null,
  }));
  if (frames.length === 0) return null;
  return {
    frames,
    // Ids are never reused, so the next one has to clear every frame that came back — a kept
    // arrangement written by a newer build, or an id list nobody can vouch for, must not hand a
    // fresh frame the name of an old one.
    nextId: Math.max(saved.nextId, ...frames.map((frame) => Number(frame.id) + 1 || 0)),
    count,
    page: 1,
    focus: frames[0]!.id,
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
