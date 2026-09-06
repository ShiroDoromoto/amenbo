// The columns beside the panes of the terminal face — the project tabs at the edge, the rail, and the
// file face on the other side — and the three things a person may do to either of the two that move:
// close it, open it again, and drag the edge between it and the panes.
//
// **A column that cannot be closed goes on taking width from the middle.** The panes are what the
// face is for, and on a laptop the two columns together are most of a pane's worth of room. So each
// one has a way to close it and a way to open it again, and neither is ever left without the other:
// a column folded away with no way back is worse than one that never folded.
//
// **The widths are the person's, and they are kept per project on this device.** They are a UI
// setting rather than anything about the work, so they live in `localStorage` beside the board's own
// columns and follow the same shape they do — a floor, a ceiling read off the window, and a clamp
// applied on the way in and on the way out, so a value written by an older build or a wider screen
// cannot come back as a column with no room left beside it (`AMB-D-312`, `../core/sidebarWidth`).
// What is kept is kept **per project**: the number of panes and the amount there is to read are not
// the same from one project to the next, so one answer for the whole device fits whichever project
// it was dragged in and fights the others (`AMB-D-835`).
//
// **The file face has two widths.** A narrow one the panes are drawn beside, and a wide one that
// lies over them (`AMB-D-835`). They are dragged separately and kept separately, because they answer
// different questions: how much room reading may take while the work is still in view, and how much
// it may take while it is being read. The states themselves — closed, narrow, wide — and the moves
// between them are `AMB-T-4253`'s; what is here is where the two answers live.
//
// **A column is always a column, until the wide one is asked for.** The narrow widths never lie over
// the panes, because the room for them is always there: the window's own floor is 960px and the four
// floors together are 760px — the tab column's own among them, now that it is dragged too — and the
// ceilings below are what keep the middle from being dragged away (`AMB-D-816`). The wide width
// is the one exception, and it is a chosen one: it covers the panes and stops at the rail and the
// tabs, neither of which is ever covered (`AMB-D-835`, `AMB-D-838`).
//
// **Whether a column was asked for, which half of the file face is up, and how the project tabs are
// drawn, are kept for the device.** They are not among the answers the decision made per project
// (`AMB-D-835`), and the reason is what each of them is: a width is how much room this project's work
// wants, where a column being open at all is how the person likes to work, wherever they are. The tab
// column's own width goes with these rather than with the widths above, for the same reason: what it
// draws is one row per project, the same list whichever project is on the screen, so how wide it wants
// to be is not something any one project has an answer to (`AMB-D-848`).
//
// **The tab column is the one thing here that is never closed** (`AMB-D-838`). Its named width is
// dragged and kept the way the others are (`AMB-D-848`); its compact width is not, being the mark and
// no more. What it is standing at comes off the window before anything else is measured against it,
// because it is the one column whose room no ceiling may count on getting back.

/**
 * The least width a terminal is worth drawing in.
 *
 * It is what the middle is kept at: a column's ceiling is whatever the window has left once this
 * and the column on the other side are out of it. Rounded rather than derived — what a terminal
 * needs is however many columns of text the program in it expects, and no number this side can
 * compute answers that.
 */
export const PANE_MIN = 320;

/** The widths kept per project — the project is the last part of the key (`keyOf`). */
const RAIL_WIDTH = "railWidth";
const SIDE_NARROW = "sideNarrow";
const SIDE_WIDE = "sideWide";

/** Kept for the device, so these are whole keys rather than stems. */
const RAIL_SHOWN = "amenbo.termface.railShown";
const SIDE_SHOWN = "amenbo.termface.sideShown";
const SIDE_TAB = "amenbo.termface.sideTab";
const TABS_COMPACT = "amenbo.termface.tabsCompact";
const TABS_WIDTH = "amenbo.termface.tabsWidth";

/** The rail's floor and where it starts — the fixed width it shipped with, so nothing moves until
 *  somebody drags it. */
export const RAIL_MIN = 120;
export const RAIL_DEFAULT = 160;

/** The file face's floor and where its narrow width starts, the width it shipped with (16rem). */
export const SIDE_MIN = 200;
export const SIDE_NARROW_DEFAULT = 256;

/**
 * Where the wide width starts.
 *
 * Rounded, the way the others are, and rounded to what reading wants rather than to a share of the
 * window: 560px holds a line of prose or code at a readable measure, and on the narrowest window the
 * application opens it still leaves the rail and a strip of the panes showing behind it — enough to
 * see that the work is still there and where to press to come back to it.
 */
export const SIDE_WIDE_DEFAULT = 560;

/**
 * The width the project tabs are drawn at folded — the mark and everything drawn around it: a colour
 * and one character, the padding of the tab and of the list it is in (`styles/global.css`).
 *
 * **It is the one width on this face that is not dragged.** Folded, the column is the mark and nothing
 * else, and a mark is one size — a drag would be moving the room around a 24px square. What the
 * stylesheet still has to hold up is this leaving the mark that room.
 */
export const TABS_COMPACT_WIDTH = 46;

/** The named column's floor and where it starts — the fixed width it shipped with, so nothing moves
 *  until somebody drags it. The floor is the rail's own, because what it draws is a name of the same
 *  kind. */
export const TABS_MIN = 120;
export const TABS_DEFAULT = 160;

/**
 * The width the column is drawn at, folded or named (`AMB-D-838`, `AMB-D-848`).
 *
 * The face hands whichever it answers with to the stylesheet as `--tabs-w`, so the width the middle's
 * room is measured against and the width on the screen are one number.
 */
export function tabsWidth(compact: boolean): number {
  return compact ? TABS_COMPACT_WIDTH : getTabsWidth();
}

/**
 * Where a per-project answer is kept, or `null` for no project.
 *
 * The face has no project of its own for a moment as it comes up, and one that is on no project has
 * no answer to keep: it is drawn at the defaults and nothing is written, rather than a project's
 * widths being overwritten by a run that had not yet been told which project it was on.
 */
function keyOf(name: string, project: number | null): string | null {
  return project === null ? null : `amenbo.termface.${name}.${project}`;
}

/** What this device has kept under a key, or `null` where nothing can be read. */
function kept(key: string | null): string | null {
  if (key === null || typeof localStorage === "undefined") return null;
  return localStorage.getItem(key);
}

/** Keep an answer where there is a key to keep it under, and where the device allows it. */
function keep(key: string | null, value: string): void {
  if (key === null) return;
  try {
    localStorage.setItem(key, value);
  } catch { /* take the answer even where localStorage is unavailable */ }
}

/**
 * The most a narrow column may take: whatever the window has left once the column on the other side
 * and a pane's floor are out of it, and never less than its own floor (`AMB-D-816`).
 *
 * **Room, not a share of the window.** A share cannot answer this: 0.3 and 0.4 of a 960px window are
 * 288px and 384px, and a person who drags both out is left with 288px in the middle — under the
 * floor a pane is drawn at, on the narrowest window the application opens. Measured against the
 * room, dragging a column simply stops where the middle would start giving way.
 *
 * `other` is what the column on the other side is taking, which is zero for one that is closed:
 * closing a column is what makes room, so it counts. Where it is not known — a width read back
 * before the face has drawn — the other side's floor stands in for it, which is the least it can
 * ever be taking while it is open.
 */
function ceiling(other: number, floor: number): number {
  return Math.max(roomBeside() - other - PANE_MIN, floor);
}

/** The window's own width, or a stand-in where there is no window to ask. */
function room(): number {
  return typeof window === "undefined" ? 1280 : window.innerWidth;
}

/**
 * The width there is for the columns and the panes: the window, less the tab column.
 *
 * The tabs come out of the room before anything is measured against it, because they are the one
 * thing on this face that is never closed (`AMB-D-838`). Left out of the sum, they would take their
 * width from the middle — a rail dragged to its ceiling would push the panes under the floor they are
 * drawn at by exactly the width of the tabs (`AMB-D-816`). Dragging the tabs is read the same way:
 * widening that column narrows what the two beside the panes may be dragged to, and never the panes.
 */
function roomBeside(): number {
  return room() - tabsWidth(getTabsCompact());
}

export function railMax(side: number = SIDE_MIN): number {
  return ceiling(side, RAIL_MIN);
}

export function sideNarrowMax(rail: number = RAIL_MIN): number {
  return ceiling(rail, SIDE_MIN);
}

/**
 * The most the wide width may take: the window, less the rail.
 *
 * The panes are not in the sum, because the wide width is the one that lies over them: what it must
 * leave is the rail, which is never covered (`AMB-D-835`), and the tabs, which are never covered
 * either (`AMB-D-838`). Nothing here keeps it above the narrow
 * width — the two are dragged separately, and a person who drags the wide one down to the floor has
 * said what they meant.
 */
export function sideWideMax(rail: number = RAIL_MIN): number {
  return Math.max(roomBeside() - rail, SIDE_MIN);
}

export function clampRailWidth(px: number, side: number = SIDE_MIN): number {
  return Math.min(Math.max(px, RAIL_MIN), railMax(side));
}

export function clampSideNarrow(px: number, rail: number = RAIL_MIN): number {
  return Math.min(Math.max(px, SIDE_MIN), sideNarrowMax(rail));
}

export function clampSideWide(px: number, rail: number = RAIL_MIN): number {
  return Math.min(Math.max(px, SIDE_MIN), sideWideMax(rail));
}

/**
 * The most the tab column may take: whatever the window has left once both columns beside the panes
 * and a pane's floor are out of it, and never less than its own floor.
 *
 * It is the one ceiling here measured against the window itself rather than against `roomBeside`, for
 * the plain reason that the width being bounded is the one that sum takes out.
 */
export function tabsMax(rail: number = RAIL_MIN, side: number = SIDE_MIN): number {
  return Math.max(room() - rail - side - PANE_MIN, TABS_MIN);
}

export function clampTabsWidth(
  px: number, rail: number = RAIL_MIN, side: number = SIDE_MIN,
): number {
  return Math.min(Math.max(px, TABS_MIN), tabsMax(rail, side));
}

/** A width this project has kept, clamped, or where it starts when nothing has been kept. */
function keptWidth(
  name: string, project: number | null, fallback: number, clamp: (px: number) => number,
): number {
  const raw = kept(keyOf(name, project));
  const px = raw === null ? NaN : Number(raw);
  return Number.isFinite(px) ? clamp(px) : fallback;
}

/** Clamp, keep, and answer with the width actually taken — which is a width even where nothing can
 *  be kept, so a drag still moves the column in a browser that refuses storage. */
function keepWidth(
  name: string, project: number | null, px: number, clamp: (px: number) => number,
): number {
  const taken = clamp(px);
  keep(keyOf(name, project), String(taken));
  return taken;
}

export function getRailWidth(project: number | null, side: number = SIDE_MIN): number {
  return keptWidth(RAIL_WIDTH, project, RAIL_DEFAULT, (px) => clampRailWidth(px, side));
}

export function setRailWidth(project: number | null, px: number, side: number = SIDE_MIN): number {
  return keepWidth(RAIL_WIDTH, project, px, (one) => clampRailWidth(one, side));
}

export function getSideNarrow(project: number | null, rail: number = RAIL_MIN): number {
  return keptWidth(SIDE_NARROW, project, SIDE_NARROW_DEFAULT, (px) => clampSideNarrow(px, rail));
}

export function setSideNarrow(project: number | null, px: number, rail: number = RAIL_MIN): number {
  return keepWidth(SIDE_NARROW, project, px, (one) => clampSideNarrow(one, rail));
}

export function getSideWide(project: number | null, rail: number = RAIL_MIN): number {
  return keptWidth(SIDE_WIDE, project, SIDE_WIDE_DEFAULT, (px) => clampSideWide(px, rail));
}

export function setSideWide(project: number | null, px: number, rail: number = RAIL_MIN): number {
  return keepWidth(SIDE_WIDE, project, px, (one) => clampSideWide(one, rail));
}

/** Whether a column has been asked for. Both start shown: the face has always drawn them, and a
 *  first run that came up with two closed columns would be hiding what it is offering. */
function keptShown(key: string): boolean {
  return kept(key) !== "0";
}

function keepShown(key: string, want: boolean): boolean {
  keep(key, want ? "1" : "0");
  return want;
}

export function getRailShown(): boolean {
  return keptShown(RAIL_SHOWN);
}

export function setRailShown(want: boolean): boolean {
  return keepShown(RAIL_SHOWN, want);
}

export function getSideShown(): boolean {
  return keptShown(SIDE_SHOWN);
}

export function setSideShown(want: boolean): boolean {
  return keepShown(SIDE_SHOWN, want);
}

/**
 * The named width this device was left at, or where it starts when nothing has been kept.
 *
 * Clamped on the way out as well as on the way in, the way the widths beside the panes are: a column
 * dragged wide on a 4K display must not come back on a laptop with no room left beside it
 * (`AMB-D-312`).
 */
export function getTabsWidth(): number {
  const raw = kept(TABS_WIDTH);
  const px = raw === null ? NaN : Number(raw);
  return Number.isFinite(px) ? clampTabsWidth(px) : TABS_DEFAULT;
}

/** Clamp, keep, and answer with the width actually taken — a width even where nothing can be kept, so
 *  a drag still moves the column in a browser that refuses storage. */
export function setTabsWidth(
  px: number, rail: number = RAIL_MIN, side: number = SIDE_MIN,
): number {
  const taken = clampTabsWidth(px, rail, side);
  keep(TABS_WIDTH, String(taken));
  return taken;
}

/**
 * Whether the project tabs are drawn compact — the colour and the first character alone, without the
 * names (`AMB-D-838`).
 *
 * **They start named.** A first run that came up with a column of coloured letters would be asking a
 * person to learn which is which before they had been told any of it once. It is kept for the device
 * rather than per project, because it is how somebody likes to work and not what one project's work
 * wants: moving between projects must not fold and unfold the very column being moved with.
 */
export function getTabsCompact(): boolean {
  return kept(TABS_COMPACT) === "1";
}

/** Keep the answer, and give it back — an answer even where nothing can be kept. */
export function setTabsCompact(want: boolean): boolean {
  keep(TABS_COMPACT, want ? "1" : "0");
  return want;
}

/** Which half of the file face is up: the memo a person writes on, or the folder's own files. */
export type SideTab = "files" | "memo";

/**
 * The half this device had up, or the one the face opens on where nothing has been kept.
 *
 * **It opens on the memo**, and the reason is not which of the two is used more. It is who starts:
 * the memo is opened by a person who wants it, and nothing is lost by its being closed — it is there
 * the moment they ask. The files half is a reading of what the folder has been doing
 * (`../files/FilesPanel`), which is there to be gone to whenever a person wants it and asks for
 * nothing while they are away. The memo also has something in it the moment it is opened — their own
 * words — where the files half opens on whatever happened to move last.
 *
 * Anything else kept reads as the memo: the value is one of two words, and a word from an older
 * build or a hand-edited store is not an answer.
 */
export function getSideTab(): SideTab {
  return kept(SIDE_TAB) === "files" ? "files" : "memo";
}

/** Keep the half that was asked for, and answer with it — a half even where nothing can be kept. */
export function setSideTab(which: SideTab): SideTab {
  keep(SIDE_TAB, which);
  return which;
}
