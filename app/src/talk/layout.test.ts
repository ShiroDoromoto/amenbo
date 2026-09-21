// What the arrangement has to keep true, none of which is visible in the arithmetic that does it.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  ACROSS, addPane, BOXES, closedFrame, closedIn, DEFAULT_SIZE, DOWN, EMPTY_LAYOUT, focusOn, goPage,
  goProject, gridAt, laidOut, landingOn, movedTo, movedWithin, openedFrame, openedIn, pageCount,
  pageOfFrame, paneIn, paneOfRun, panesOf, placing, reordered, resized, restored, roomOnPage,
  runFrameId, SIZES, slotsOf, stoodForRun, writing, folding, type Layout, type Size,
} from "./layout";

/** The ids of the panes drawn on one page, in the order they were laid down. */
const idsOn = (layout: Layout, page: number) => slotsOf(layout, page).map((one) => one.frame.id);

/** The size of one pane, by id. */
const sizeOf = (layout: Layout, id: string) => layout.frames.find((one) => one.id === id)?.size;

/**
 * A layout with `n` panes opened in one project, all of them at `size`.
 *
 * The size is pressed for rather than written in: a pane is measured against the one the page is
 * already showing, so sizing the first is what sizes every one after it — which is the same thing a
 * person does, and not a frame put into the shape at a size nobody asked for.
 */
function withPanes(n: number, size: Size = "half", project = 1): Layout {
  let layout: Layout = { ...EMPTY_LAYOUT, project };
  for (let i = 0; i < n; i++) {
    const made = openedFrame(layout, project, `/work/${project}`);
    layout = i === 0 ? resized(made.layout, made.frame.id, size) : made.layout;
  }
  return layout;
}

/**
 * The drawing, seeded back to the count it replaced.
 *
 * A pane's id is drawn rather than counted (`newFrameId`), so nothing here could name a pane by the
 * order it was opened in — which is the one thing every case below does. Seeding it is what lets
 * them go on saying "2" and meaning the second pane opened. What a real id looks like is that
 * function's business; what is pinned here is the arrangement around it.
 */
let drawn = 0;

beforeEach(() => {
  drawn = 0;
  vi.stubGlobal("crypto", { randomUUID: () => String(++drawn) });
});

afterEach(() => vi.unstubAllGlobals());

describe("a place is made by opening one", () => {
  it("has no panes at all until something is opened", () => {
    expect(EMPTY_LAYOUT.frames).toHaveLength(0);
    expect(slotsOf(EMPTY_LAYOUT, 1)).toHaveLength(0);
    // Still a page: it is where the way in is put.
    expect(pageCount(EMPTY_LAYOUT)).toBe(1);
  });

  it("draws one slot per pane and no empty ones beside them", () => {
    const two = withPanes(2, "quarter");
    expect(idsOn(two, 1)).toEqual(["1", "2"]);
  });

  it("goes to the pane it just opened, on the page it landed on", () => {
    const made = openedFrame(withPanes(2), 1, "/work/1");
    expect(made.frame.id).toBe("3");
    expect(made.layout.focus).toBe("3");
    expect(made.layout.page).toBe(2);
  });

  // A pane opened again from a record comes back under the id it had (`AMB-D-897`): that id is what
  // a provider's own home is named after, so a new one would be a different place.
  it("takes the id it is given, for a pane being opened again under its own", () => {
    const made = openedFrame(withPanes(2), 1, "/work/1", false, "a-pane-that-was");
    expect(made.frame.id).toBe("a-pane-that-was");
    expect(made.layout.focus).toBe("a-pane-that-was");
  });

  it("opens the first pane of a project at the whole page, and the rest at the page's own size", () => {
    // Nothing to be measured against means nothing to be smaller than: one pane on its own fills the
    // page (`AMB-D-939`). After that a pane is the size of the pane the page is showing.
    const one = openedFrame({ ...EMPTY_LAYOUT, project: 1 }, 1, "/work/1");
    expect(one.frame.size).toBe("whole");
    const small = resized(one.layout, "1", "sixth");
    expect(openedFrame(small, 1, "/work/1").frame.size).toBe("sixth");
  });

  it("puts it at the end of the page the reader is on, not at the end of the whole list", () => {
    // Three panes at a sixth each, with the reader back on page one: the new pane goes in behind the
    // last pane of *that* page, which is where the empty frame was drawn.
    const three = goPage(resized(withPanes(3, "sixth"), "3", "whole"), 1);
    expect(idsOn(three, 1)).toEqual(["1", "2"]);
    const made = openedFrame(three, 1, "/work/1");
    expect(panesOf(made.layout, 1).map((one) => one.id)).toEqual(["1", "2", "4", "3"]);
    expect(idsOn(made.layout, 1)).toEqual(["1", "2", "4"]);
  });
});

describe("a page with room says so, and a full one says nothing", () => {
  it("has room on the page the panes stop on, and none on the ones they fill", () => {
    const three = withPanes(3, "half");
    expect(roomOnPage(three, 1), "a full page had room in it").toBe(false);
    expect(roomOnPage(three, 2)).toBe(true);
  });

  it("has room on the one page of a project with nothing open", () => {
    expect(roomOnPage({ ...EMPTY_LAYOUT, project: 1 }, 1)).toBe(true);
  });

  it("has none anywhere when every page is filled", () => {
    const four = withPanes(4, "half");
    expect([1, 2].map((page) => roomOnPage(four, page))).toEqual([false, false]);
  });

  it("has none where the hole left on the page is smaller than the pane that would go in it", () => {
    // Half the page and a quarter of it leave a quarter standing empty. Asked for another half —
    // which is what the pane being worked in is — the page has nowhere to put one, and a hole the
    // next pane does not fit is not room.
    const two = resized(withPanes(2, "half"), "2", "quarter");
    expect(roomOnPage(focusOn(two, "2"), 1), "a quarter did not fit a quarter").toBe(true);
    expect(roomOnPage(focusOn(two, "1"), 1), "a half fitted a quarter").toBe(false);
  });

  it("draws the empty frame where the pane it offers will be", () => {
    // The same search a real pane goes through, so nothing on the page moves for it and the pane
    // lands exactly where the question stood.
    const two = withPanes(2, "quarter");
    const spare = landingOn(two, 1);
    expect(spare).toEqual({ across: 0, down: 1, size: "quarter" });
    const made = openedFrame(two, 1, "/work/1");
    const laid = placing(panesOf(made.layout, 1)).find((one) => one.frame.id === made.frame.id);
    expect(laid).toMatchObject({ page: 1, across: 0, down: 1 });
  });
});

describe("asking for another pane", () => {
  it("goes to the page that has room, and makes no new one", () => {
    const three = goPage(withPanes(3, "half"), 1);
    const asked = addPane(three);
    expect(asked.page).toBe(2);
    expect(pageCount(asked)).toBe(2);
  });

  it("brings a page into being where every page is full", () => {
    const two = withPanes(2, "half");
    expect(pageCount(two)).toBe(1);
    const asked = addPane(two);
    expect(asked.page).toBe(2);
    expect(pageCount(asked)).toBe(2);
    // Nothing is on it: what makes a place is opening one, and nobody has yet.
    expect(slotsOf(asked, 2)).toHaveLength(0);
  });

  it("takes that page away again as soon as the reader is somewhere else", () => {
    const asked = addPane(withPanes(2, "half"));
    const back = goPage(asked, 1);
    expect(back.page).toBe(1);
    expect(pageCount(back), "an empty page outlived the asking").toBe(1);
  });

  it("makes it a page like any other once a pane is opened on it", () => {
    const asked = addPane(withPanes(2, "half"));
    const made = openedFrame(asked, 1, "/work/1");
    expect(made.layout.adding).toBe(false);
    expect(pageCount(made.layout)).toBe(2);
    expect(idsOn(made.layout, 2)).toEqual(["3"]);
    // The page it was brought into being from is what sized it: a page with no panes on it is not a
    // reason to go back to the whole page.
    expect(made.frame.size).toBe("half");
  });

  it("does not survive a resize, which measures the pages afresh", () => {
    const asked = addPane(withPanes(2, "half"));
    expect(pageCount(resized(asked, "1", "quarter"))).toBe(1);
  });

  it("is no part of the arrangement that is kept", () => {
    expect(JSON.stringify(laidOut(addPane(withPanes(2, "half"))))).not.toContain("adding");
  });
});

describe("a pane belongs to a project", () => {
  it("shows one project's panes and not another's", () => {
    let layout = withPanes(2, "half", 1);
    layout = openedFrame(layout, 2, "/work/2").layout;
    expect(layout.project).toBe(2);
    expect(idsOn(layout, 1)).toEqual(["3"]);
    expect(panesOf(layout, 1).map((one) => one.id)).toEqual(["1", "2"]);
  });

  it("lands on a project's first pane when it is picked", () => {
    let layout = withPanes(2, "half", 1);
    layout = openedFrame(layout, 2, "/work/2").layout;
    const back = goProject(layout, 1);
    expect(back.page).toBe(1);
    expect(back.focus).toBe("1");
  });

  it("takes the screen to another project when a pane there is reached for", () => {
    let layout = withPanes(1, "half", 1);
    layout = openedFrame(layout, 2, "/work/2").layout;
    const back = focusOn(layout, "1");
    expect(back.project).toBe(1);
    expect(back.focus).toBe("1");
  });

  it("counts a project's pages from its own panes", () => {
    let layout = withPanes(4, "half", 1);
    layout = openedFrame(layout, 2, "/work/2").layout;
    expect(pageCount(layout)).toBe(1);
    expect(pageCount(goProject(layout, 1))).toBe(2);
  });

  it("opens a pane on a project the face is not showing at that project's own last size", () => {
    // There is no page to measure it on, so it follows the project's list rather than the screen.
    let layout = withPanes(1, "eighth", 1);
    layout = openedFrame(layout, 2, "/work/2").layout;
    const back = openedFrame(goProject(layout, 3), 1, "/work/1");
    expect(back.frame.size).toBe("eighth");
    expect(panesOf(back.layout, 1).map((one) => one.id)).toEqual(["1", "3"]);
  });
});

describe("a frame is a place, not a process", () => {
  it("keeps the frame when the program in it exits", () => {
    const { layout, frame } = openedFrame(EMPTY_LAYOUT, 1, "/w");
    const ended = closedIn(openedIn(layout, frame.id, "s1", "/w", null), "s1");
    expect(ended.frames).toHaveLength(1);
    expect(ended.frames[0]!.session).toBeNull();
    expect(ended.frames[0]!.id).toBe(frame.id);
  });

  it("never hands a retired id out again — a name is kept against it", () => {
    const four = withPanes(4);
    const ended = closedIn(openedIn(four, "1", "s1", null, null), "s1");
    // Against the real drawing, not the seeded count: what is pinned here is that the id of a pane
    // whose session ended cannot come back on a fresh one, and a counter would say so of itself.
    vi.unstubAllGlobals();
    const made = openedFrame(ended, 1, "/w").frame.id;
    expect(ended.frames.map((one) => one.id)).not.toContain(made);
  });
});

describe("closing a pane takes the place away", () => {
  it("is gone for good, and what is left closes up", () => {
    const three = withPanes(3, "half");
    const left = closedFrame(three, "1");
    expect(left.frames.map((one) => one.id)).toEqual(["2", "3"]);
    // Two halves fill one page: the pane that was on the second page came up into the room the
    // closed one gave back, rather than the page keeping a hole.
    expect(pageCount(left)).toBe(1);
    expect(idsOn(left, 1)).toEqual(["2", "3"]);
  });

  it("does not hand the closed pane's id out again", () => {
    const two = withPanes(2);
    const left = closedFrame(two, "2");
    // The real drawing again (`never hands a retired id out again`): the pane is gone from the
    // arrangement, so nothing in the arrangement could tell a fresh id from the one it had.
    vi.unstubAllGlobals();
    const made = openedFrame(left, 1, "/w").frame.id;
    expect(two.frames.map((one) => one.id)).not.toContain(made);
  });

  it("leaves the reader on whatever moved into its place", () => {
    const three = focusOn(withPanes(3, "half"), "2");
    expect(closedFrame(three, "2").focus).toBe("3");
  });

  it("leaves them on the pane before it where nothing moved up", () => {
    const three = focusOn(withPanes(3, "half"), "3");
    const left = closedFrame(three, "3");
    expect(left.focus).toBe("2");
    // Page 2 has gone with the pane that was the only thing on it.
    expect(left.page).toBe(1);
  });

  it("leaves them on nothing when the last pane of the project goes", () => {
    const left = closedFrame(withPanes(1), "1");
    expect(left.focus).toBeNull();
    expect(left.frames).toHaveLength(0);
    expect(left.page).toBe(1);
  });

  it("does not move the reader when the pane they are on is not the one that went", () => {
    const three = focusOn(withPanes(3, "half"), "1");
    expect(closedFrame(three, "3").focus).toBe("1");
  });

  it("is nothing at all for an id no frame has", () => {
    const three = withPanes(3, "half");
    expect(closedFrame(three, "9")).toBe(three);
  });
});

describe("where a pane works", () => {
  it("is the folder it was opened in, which an agent's own cd does not redraw", () => {
    const one = openedIn(withPanes(1), "1", "s1", "/repo", null);
    expect(movedTo(one, "s1", "/elsewhere").frames[0]!.folder).toBe("/repo");
  });

  it("is learned from the session for a pane that took one up rather than starting it", () => {
    const adopted = openedIn({ ...EMPTY_LAYOUT, project: 1, frames: [
      {
        id: "1",
        project: 1,
        size: "whole",
        session: null,
        folder: null,
        agent: null,
        resumes: false,
        written: "",
        inserted: [],
        composeOpen: false,
        run: null,
      },
    ] }, "1", "s1", null, "claude");
    // What is running comes off the session as well, and by the same reasoning: a pane that adopted
    // one never asked for it.
    expect(adopted.frames[0]!.agent).toBe("claude");
    expect(movedTo(adopted, "s1", "/said").frames[0]!.folder).toBe("/said");
  });
});

describe("a pane is laid down at its size, and the pages fall out of the order", () => {
  it("offers six sizes, none of which asks for a third row, and opens a pane at the whole page", () => {
    expect(SIZES).toEqual(["whole", "half", "half-down", "quarter", "sixth", "eighth"]);
    expect(DEFAULT_SIZE).toBe("whole");
    for (const size of SIZES) {
      const box = BOXES[size];
      // Every size divides the page exactly, which is what lets one grid draw all six.
      expect((ACROSS * DOWN) % (box.across * box.down)).toBe(0);
      expect(box.down).toBeLessThanOrEqual(DOWN);
      expect(box.across).toBeLessThanOrEqual(ACROSS);
    }
  });

  it("makes half the page two different rectangles, and they do not share a page", () => {
    // Side by side is six cells across both rows; laid down the page it is the whole width and one
    // row. Two of either fills a page — and one of each does not fit together, however the areas
    // add up (`AMB-D-939`).
    expect(BOXES.half).toEqual({ across: 6, down: 2 });
    expect(BOXES["half-down"]).toEqual({ across: 12, down: 1 });
    const mixed = resized(withPanes(2, "half"), "2", "half-down");
    expect(idsOn(mixed, 1)).toEqual(["1"]);
    expect(idsOn(mixed, 2)).toEqual(["2"]);
  });

  it("puts the boxes on the grid as the stylesheet counts its lines", () => {
    expect(gridAt("quarter", 6, 1)).toEqual({ gridColumn: "7 / span 6", gridRow: "2 / span 1" });
    expect(gridAt("whole", 0, 0)).toEqual({ gridColumn: "1 / span 12", gridRow: "1 / span 2" });
  });

  it("is one pane's answer and not the project's — the panes beside it do not move", () => {
    // How much room a piece of work wants is a fact about that piece of work, and a single answer
    // held for the project made every pane on it change together.
    const three = resized(withPanes(3, "quarter"), "2", "eighth");
    expect([1, 2, 3].map((id) => sizeOf(three, String(id))))
      .toEqual(["quarter", "eighth", "quarter"]);
  });

  it("leaves the hole where the fitting ran out, rather than filling it from further down the list", () => {
    // Half the page, a quarter, and half again: the second half does not fit the quarter that is
    // left, so it starts the next page and a quarter of the first stays empty (`AMB-D-939`). The
    // pane must not come back up to fill it — the pages are read as the order the panes are in.
    let layout = resized(withPanes(1), "1", "half");
    layout = resized(openedFrame(layout, 1, "/work/1").layout, "2", "quarter");
    layout = resized(openedFrame(layout, 1, "/work/1").layout, "3", "half");
    const laid = placing(panesOf(layout, 1));
    expect(laid.map((one) => [one.frame.id, one.page, one.across, one.down])).toEqual([
      ["1", 1, 0, 0],
      ["2", 1, 6, 0],
      // The quarter under the quarter stays empty.
      ["3", 2, 0, 0],
    ]);
  });

  it("sends the panes that no longer fit to the next page, and brings them back when it shrinks", () => {
    const four = withPanes(4, "quarter");
    expect(pageCount(four)).toBe(1);
    const wide = resized(four, "1", "half");
    expect(idsOn(wide, 1)).toEqual(["1", "2", "3"]);
    expect(idsOn(wide, 2)).toEqual(["4"]);
    expect(idsOn(resized(wide, "1", "quarter"), 1)).toEqual(["1", "2", "3", "4"]);
  });

  it("carries the reader to the page the pane they resized is on now", () => {
    const four = focusOn(withPanes(4, "quarter"), "4");
    expect(four.page).toBe(1);
    const wide = resized(four, "4", "whole");
    expect(pageOfFrame(wide, "4")).toBe(2);
    expect(wide.page).toBe(2);
  });

  it("does not renumber the panes — the list is what it was", () => {
    const four = withPanes(4);
    expect(resized(four, "1", "quarter").frames.map((one) => one.id))
      .toEqual(four.frames.map((one) => one.id));
  });

  it("lands on a page that exists when the one the reader was on has gone", () => {
    const two = withPanes(2, "half");
    const wide = resized(two, "1", "whole");
    expect(pageCount(wide)).toBe(2);
    const on = goPage({ ...wide, focus: null }, 2);
    const back = resized(on, "1", "half");
    expect(pageCount(back)).toBe(1);
    expect(back.page).toBeLessThanOrEqual(pageCount(back));
  });

  it("is nothing at all for a size the pane is already at, or an id no frame has", () => {
    const two = withPanes(2, "half");
    expect(resized(two, "1", "half")).toBe(two);
    expect(resized(two, "9", "whole")).toBe(two);
  });

  it("refuses a page this project has not got", () => {
    expect(goPage(withPanes(2), 3).page).toBe(1);
  });

  it("draws the page at the size that was pressed for, however few panes are open", () => {
    // Three eighths on a page is one page with room on it, not a page that grew to hold three: the
    // size is the press, and the cells past the empty frame stay blank.
    const wide = withPanes(3, "eighth");
    expect(pageCount(wide)).toBe(1);
    expect(slotsOf(wide, 1)).toHaveLength(3);
    expect(roomOnPage(wide, 1)).toBe(true);
  });

  it("measures a new pane against the pane being worked in, and against the last of the page where it is not", () => {
    const three = withPanes(3, "half");
    // The reader is on page two, in the pane they just opened.
    expect(openedFrame(resized(three, "3", "sixth"), 1, "/work/1").frame.size).toBe("sixth");
    // Back on page one with the focus left behind: the page's own last pane is what it is measured
    // against.
    expect(openedFrame(goPage(resized(three, "2", "quarter"), 1), 1, "/work/1").frame.size)
      .toBe("quarter");
    // And on a page `addPane` brought into being, which has no panes at all, the last pane of the
    // project is.
    expect(openedFrame(addPane(withPanes(2, "half")), 1, "/work/1").frame.size).toBe("half");
  });
});

describe("an arrangement kept between runs", () => {
  it("keeps the shape and lets the sessions go", () => {
    let layout = withPanes(2);
    layout = openedIn(layout, "1", "session-a", "/work/1", "claude");
    layout = openedIn(layout, "2", "session-b", "/work/1", null);

    const kept = laidOut(layout);
    expect(kept.frames).toEqual([
      // What was started in each, which is the half of a row a folder cannot carry: the second is at
      // a plain prompt, and a prompt has nothing to name. And which way the box under each was left,
      // written both ways round because a row without it is a row from before it was kept
      // (`AMB-D-890`).
      { id: "1", project: 1, size: "half", folder: "/work/1", agent: "claude", composeOpen: false },
      { id: "2", project: 1, size: "half", folder: "/work/1", composeOpen: false },
    ]);
    // What was running is not in it at all: a session died with the last run, and a pane drawn as
    // though it were still there would be the window saying something untrue.
    expect(JSON.stringify(kept)).not.toContain("session-a");
  });

  it("carries the project the face is on, for the window that has no ledger to be asked on", () => {
    // The window the terminal is split out into has no ledger to have taken a project from, so an
    // arrangement with no panes to name one opens as the project the board was showing
    // (`../shell/WorkspaceFace`).
    expect(laidOut(withPanes(1)).project).toBe(1);
    // A face that has not been told of a project says so by leaving it out, rather than naming one
    // no pane is in.
    expect(laidOut(EMPTY_LAYOUT)).not.toHaveProperty("project");
  });

  it("names the pane being worked in, for the window that comes up on it", () => {
    // The press that splits hands nothing over, so where the reader was is theirs to read back out
    // of the shape (`../shell/WorkspaceFace`).
    const layout = focusOn(withPanes(2), "2");
    expect(laidOut(layout).splitOut).toBe("2");
    // A face with no pane to be working in leaves it out rather than naming a place that is not one.
    expect(laidOut(EMPTY_LAYOUT)).not.toHaveProperty("splitOut");
  });

  it("comes back as places to open a terminal in, each in its own project", () => {
    const back = restored({
      frames: [
        { id: "1", project: 7, size: "quarter", folder: "/work/repo" },
        { id: "2", project: 8, size: "quarter" },
      ],
    }, null);
    expect(back.frames.map((one) => one.size)).toEqual(["quarter", "quarter"]);
    expect(back.frames.map((one) => one.session)).toEqual([null, null]);
    expect(back.frames.map((one) => one.folder)).toEqual(["/work/repo", null]);
    // The face has to be showing something, and the first pane is where a fresh one starts too.
    expect(back.project).toBe(7);
    expect(back.focus).toBe("1");
    expect(back.page).toBe(1);
  });

  it("comes back with what was started in each place, and with none of it running", () => {
    // The row is the way back into the session rather than a picture of one (`AMB-D-869`): the pane
    // says what was in it, and the window is what decides to start one again (`AMB-T-4641`).
    const back = restored({
      project: 1,
      frames: [{ id: "1", project: 1, folder: "/work/repo", agent: "claude" }],
    }, 1);
    expect(back.frames[0]!.agent).toBe("claude");
    expect(back.frames[0]!.session).toBeNull();
    // And a pane that was at a plain prompt has nothing to name, which is not the same as a pane
    // nobody can account for.
    const bare = restored({ frames: [{ id: "1", project: 1 }] }, 1);
    expect(bare.frames[0]!.agent).toBeNull();
  });

  it("marks the place that came back holding a way into what was running in it", () => {
    // What the mark is for is the press it spares: the face opens such a place without being asked
    // (`AMB-D-869`), and every other place is drawn with the way in on it.
    const back = restored({
      project: 1,
      frames: [
        { id: "1", project: 1, folder: "/work/repo", agent: "claude", resumes: true },
        { id: "2", project: 1, folder: "/work/repo", agent: "claude" },
        // A plain shell: the row says nothing was left to come back to, however it is marked.
        { id: "3", project: 1, folder: "/work/repo", resumes: true },
      ],
    }, 1);
    expect(back.frames.map((one) => one.resumes)).toEqual([true, false, false]);
  });

  it("stops calling a place one that came back, once a terminal has started in it", () => {
    // A page turned away from and back again mounts the pane afresh, and a mark left standing would
    // read as a second reason to start something there.
    const back = restored({
      project: 1,
      frames: [{ id: "1", project: 1, folder: "/work/repo", agent: "claude", resumes: true }],
    }, 1);
    const open = openedIn(back, "1", "s1", "/work/repo", "claude");
    expect(open.frames[0]!.resumes).toBe(false);
    // And it is not written down: what the mark stands for is a handle the window never holds.
    expect(laidOut(open).frames[0]).not.toHaveProperty("resumes");
  });

  it("puts a pane whose project nothing recorded where the person is looking", () => {
    const back = restored({ frames: [{ id: "1", folder: "/work/repo" }] }, 5);
    expect(back.frames[0]!.project).toBe(5);
  });

  it("has nowhere to put one when the window is on no project either", () => {
    expect(restored({ frames: [{ id: "1" }] }, null).frames).toHaveLength(0);
  });

  it("hands the next frame an id no name is already on", () => {
    // The ids an arrangement comes back with are whatever was written down — an older build's
    // counted ones, or a hand-over nobody can vouch for — and a fresh frame must not take one of
    // them. Drawing the id is what settles that without reading them at all (`AMB-D-897`), so this
    // is the real drawing rather than the seeded count, whose next value is one of the two below.
    const back = restored({ frames: [{ id: "1", project: 1 }, { id: "7", project: 1 }] }, null);
    vi.unstubAllGlobals();
    const made = openedFrame(back, 1, "/w").frame.id;
    expect(back.frames.map((one) => one.id)).not.toContain(made);
    expect(openedFrame(back, 1, "/w").frame.id).not.toBe(made);
  });

  it("comes back as the empty face where nothing was ever opened", () => {
    // A size is a fact about a pane, so a device with no panes has nothing to bring back but the
    // project it was left on.
    const back = restored({ project: 3, frames: [] }, 3);
    expect(back.frames).toHaveLength(0);
    expect(back.project).toBe(3);
    expect(back.focus).toBeNull();
    expect(pageCount(back)).toBe(1);
  });
});

// The crossing to the host — and to the window the workspace is split out into — carries a size on
// each pane and no place at all (`AMB-D-939`). A store written before sizes were kept is converted
// once on the way in, by the migration, so nothing here reads two shapes.
describe("a pane's size, crossing to the store and back", () => {
  it("writes the size of every pane, and reads it back onto the same pane", () => {
    const mixed = resized(withPanes(3, "quarter"), "2", "eighth");
    const kept = laidOut(mixed);
    expect(kept.frames.map((one) => one.size)).toEqual(["quarter", "eighth", "quarter"]);
    expect(restored(kept, null).frames.map((one) => one.size))
      .toEqual(["quarter", "eighth", "quarter"]);
  });

  it("tells the two halves apart, which is what a count on the project could not", () => {
    // And it tells them apart pane by pane: the first is laid down the page while the one beside it
    // is still across, which one answer held for the project could not say at all.
    const down = laidOut(resized(withPanes(2, "half"), "1", "half-down"));
    expect(down.frames.map((one) => one.size)).toEqual(["half-down", "half"]);
    expect(restored(down, null).frames.map((one) => one.size)).toEqual(["half-down", "half"]);
  });

  it("keeps one project's sizes off another's, which one answer per project could not", () => {
    let layout = resized(withPanes(1, "quarter", 1), "1", "quarter");
    layout = resized(openedFrame(goProject(layout, 2), 2, "/work/2").layout, "2", "sixth");
    const kept = laidOut(layout);
    expect(kept.frames.map((one) => [one.project, one.size]))
      .toEqual([[1, "quarter"], [2, "sixth"]]);
  });

  it("reads a size it has never heard of as no answer at all", () => {
    // A row from a build that offered some other set has to land on something this one can draw,
    // rather than on a rectangle it has no name for. What is dropped comes back as the whole page,
    // which is the honest reading — this build does not know what that pane was left at.
    const odd = restored({ frames: [{ id: "1", project: 1, size: "third" as Size }] }, 1);
    expect(odd.frames[0]!.size).toBe(DEFAULT_SIZE);
    // And so does a row from a build that kept none. An older store's splits are read once, by the
    // migration, so nothing here has to know what a count was.
    expect(restored({ frames: [{ id: "1", project: 1 }] }, 1).frames[0]!.size).toBe(DEFAULT_SIZE);
  });

  it("says nothing about a face with no panes on it", () => {
    expect(laidOut(EMPTY_LAYOUT).frames).toHaveLength(0);
    expect(laidOut({ ...EMPTY_LAYOUT, project: 1 }).frames).toHaveLength(0);
  });
});

describe("a folder handed in from the ledger", () => {
  it("finds the pane of that project already working in it, rather than one beside it", () => {
    const open = openedFrame({ ...EMPTY_LAYOUT, project: 1 }, 1, "/work/repo").layout;
    expect(paneIn(open, 1, "/work/repo")?.id).toBe("1");
  });

  it("is nothing to go to where the folder is open in another project", () => {
    const open = openedFrame({ ...EMPTY_LAYOUT, project: 2 }, 2, "/work/repo").layout;
    expect(paneIn(open, 1, "/work/repo")).toBeNull();
  });

  it("is nothing to go to where nothing of this project is in it", () => {
    const open = openedFrame({ ...EMPTY_LAYOUT, project: 1 }, 1, "/work/other").layout;
    expect(paneIn(open, 1, "/work/repo")).toBeNull();
  });
});

describe("putting the panes in order", () => {
  /** The ids of one project's panes, in the order they stand in. */
  const idsOf = (layout: Layout, project = 1) => panesOf(layout, project).map((one) => one.id);

  it("puts a pane before the one it was dropped on the near half of", () => {
    const panes = panesOf(withPanes(4), 1);
    expect(movedWithin(panes, "4", "2", "before").map((one) => one.id)).toEqual(["1", "4", "2", "3"]);
  });

  it("puts it after the one it was dropped on the far half of", () => {
    const panes = panesOf(withPanes(4), 1);
    expect(movedWithin(panes, "1", "3", "after").map((one) => one.id)).toEqual(["2", "3", "1", "4"]);
  });

  it("crosses a page the same way it crosses a pane — the pages are the order laid down", () => {
    // Four halves, two to a page: the last pane of page two onto the first of page one is one move,
    // and there is no second operation for the page it left.
    const four = withPanes(4, "half");
    const moved = reordered(four, movedWithin(panesOf(four, 1), "4", "1", "before"));
    expect(idsOn(moved, 1)).toEqual(["4", "1"]);
    expect(idsOn(moved, 2)).toEqual(["2", "3"]);
  });

  it("is the list unchanged where a pane was dropped on itself", () => {
    const panes = panesOf(withPanes(3), 1);
    expect(movedWithin(panes, "2", "2", "before")).toBe(panes);
  });

  it("is the list unchanged for an id no pane in it has", () => {
    const panes = panesOf(withPanes(3), 1);
    expect(movedWithin(panes, "9", "1", "before")).toBe(panes);
    expect(movedWithin(panes, "1", "9", "before")).toBe(panes);
  });

  it("moves the panes of the project it is on, and leaves every other project where it was", () => {
    let mixed: Layout = { ...EMPTY_LAYOUT, project: 1 };
    mixed = openedFrame(mixed, 1, "/a").layout;
    mixed = openedFrame(mixed, 2, "/b").layout;
    mixed = openedFrame(mixed, 1, "/c").layout;
    mixed = goProject(mixed, 1);
    const moved = reordered(mixed, movedWithin(panesOf(mixed, 1), "3", "1", "before"));
    expect(idsOf(moved)).toEqual(["3", "1"]);
    // The other project's pane is still where it was in the one list every project shares.
    expect(moved.frames.map((one) => one.id)).toEqual(["3", "1", "2"]);
    expect(idsOf(moved, 2)).toEqual(["2"]);
  });

  it("carries each pane's size with it, so the pages are laid out afresh", () => {
    // A pane's size is the pane's, so moving one moves its rectangle — and the page it lands on is
    // laid out from the order it is now in.
    const three = resized(withPanes(3, "quarter"), "1", "whole");
    expect(idsOn(three, 1)).toEqual(["1"]);
    const moved = reordered(three, movedWithin(panesOf(three, 1), "1", "3", "after"));
    expect(idsOn(moved, 1)).toEqual(["2", "3"]);
    expect(idsOn(moved, 2)).toEqual(["1"]);
  });

  it("leaves the page and the pane being worked in where they are", () => {
    // The pane being worked in is on page one and is carried to page two. It is still the pane the
    // person is in, and the face is still on the page they were reading (`AMB-D-853`).
    const four = goPage(focusOn(withPanes(4, "half"), "1"), 1);
    const moved = reordered(four, movedWithin(panesOf(four, 1), "1", "4", "after"));
    expect(moved.focus).toBe("1");
    expect(moved.page).toBe(1);
    expect(pageOfFrame(moved, "1")).toBe(2);
  });

  it("refuses an order that is not this project's panes, rather than writing part of one", () => {
    const three = withPanes(3);
    const panes = panesOf(three, 1);
    expect(reordered(three, panes.slice(1))).toBe(three);
    expect(reordered(three, [...panes, panes[0]!])).toBe(three);
    // The right number of panes, one of which belongs to another project.
    const beside = openedFrame(three, 2, "/b");
    const mixed = goProject(beside.layout, 1);
    expect(reordered(mixed, [panes[0]!, panes[1]!, beside.frame])).toBe(mixed);
  });
});

describe("what is written in the box under a pane", () => {
  /** One project with two pages of panes, and a sentence half written in the first one. */
  function half() {
    const one = openedFrame({ ...EMPTY_LAYOUT, project: 1 }, 1, "/repo");
    return { layout: writing(one.layout, one.frame.id, "run the tests"), frame: one.frame.id };
  }

  it("is nothing in a place that has just been opened", () => {
    const one = openedFrame({ ...EMPTY_LAYOUT, project: 1 }, 1, "/repo");
    expect(one.frame.written).toBe("");
  });

  it("is kept against the pane it was written in", () => {
    const { layout, frame } = half();
    expect(layout.frames.find((one) => one.id === frame)?.written).toBe("run the tests");
  });

  it("stays where it is when the page turns, which is the pane being put away and not written in", () => {
    const { layout, frame } = half();
    const away = goPage(goPage(layout, 2), 1);
    expect(away.frames.find((one) => one.id === frame)?.written).toBe("run the tests");
  });

  it("stays through a change of how much of the page the pane takes", () => {
    const { layout, frame } = half();
    expect(resized(layout, frame, "quarter").frames.find((one) => one.id === frame)?.written)
      .toBe("run the tests");
  });

  it("crosses with the arrangement, which is how the window split out of the face gets it", () => {
    const { layout, frame } = half();
    const over = restored(laidOut(layout), 1);
    expect(over.frames.find((one) => one.id === frame)?.written).toBe("run the tests");
  });

  it("is left out of the arrangement where the box is empty, the way the folder is", () => {
    const one = openedFrame({ ...EMPTY_LAYOUT, project: 1 }, 1, "/repo");
    expect(laidOut(one.layout).frames[0]).not.toHaveProperty("written");
  });

  it("is nothing in a place an arrangement carrying no draft is restored into", () => {
    const bare = restored({ project: 1, frames: [{ id: "1", project: 1 }] }, 1);
    expect(bare.frames[0]!.written).toBe("");
  });

  it("moves nothing the store keeps, so a keystroke is not a write to the disk", () => {
    const { layout } = half();
    const { frames: _typed, ...rest } = laidOut(layout);
    const { frames: _empty, ...was } = laidOut(writing(layout, layout.frames[0]!.id, ""));
    expect(rest).toEqual(was);
  });
});

// The box is opened by a press on one pane's band, and what that press answers is that pane
// (`AMB-D-890`). A pane is taken down and drawn again all through a run — a page turned, a pane
// resized, the terminal put in a window of its own — and a reader who opened the box did not ask for
// it to shut at any of those; nor did the readers of every other pane on the screen.
describe("whether the box under a pane is open", () => {
  /** Two panes in one project, with the box opened under the first. */
  function opened() {
    const layout = withPanes(2);
    const frame = layout.frames[0]!.id;
    return { layout: folding(layout, frame, true), frame };
  }

  it("is folded in a place that has just been opened, where nothing says otherwise", () => {
    expect(openedFrame({ ...EMPTY_LAYOUT, project: 1 }, 1, "/repo").frame.composeOpen).toBe(false);
  });

  it("is what the caller says a place is being opened with", () => {
    expect(openedFrame({ ...EMPTY_LAYOUT, project: 1 }, 1, "/repo", true).frame.composeOpen).toBe(true);
  });

  it("is kept against the pane the press was made in, and no other", () => {
    const { layout, frame } = opened();
    expect(layout.frames.find((one) => one.id === frame)?.composeOpen).toBe(true);
    expect(layout.frames.find((one) => one.id !== frame)?.composeOpen).toBe(false);
  });

  it("stays through the moves that take a pane down and draw it again", () => {
    const { layout, frame } = opened();
    const away = resized(goPage(goPage(layout, 2), 1), frame, "quarter");
    expect(away.frames.find((one) => one.id === frame)?.composeOpen).toBe(true);
  });

  // The arrangement is how the board and the window a terminal is split out into hand the face over,
  // and it is what goes on to the store as well — so this is read back in both of those.
  it("crosses in the arrangement, both ways round", () => {
    const { layout, frame } = opened();
    const back = restored(laidOut(layout), null);
    expect(back.frames.find((one) => one.id === frame)?.composeOpen).toBe(true);
    expect(back.frames.find((one) => one.id !== frame)?.composeOpen).toBe(false);
  });

  // A row written before this was kept has no answer of its own. What it opens on is this machine's
  // habit, which is what every pane opened on until then (`../core/composeStartsOpen`).
  it("opens on the habit where a row from an older build has no answer", () => {
    const older = { count: 2 as const, project: 1, frames: [{ id: "1", project: 1 }] };
    expect(restored(older, null).frames[0]?.composeOpen).toBe(false);
    expect(restored(older, null, true).frames[0]?.composeOpen).toBe(true);
  });
});

describe("what Amenbo put into the box under a pane", () => {
  const PICTURE = "/tmp/amenbo-pasted-7a/pasted-0a0b0c0d.png";

  /** A pane with a path Amenbo put in the box, and a word the person wrote after it. */
  function put() {
    const one = openedFrame({ ...EMPTY_LAYOUT, project: 1 }, 1, "/repo");
    const written = `'${PICTURE}' look at this`;
    return { layout: writing(one.layout, one.frame.id, written, [PICTURE]), frame: one.frame.id };
  }

  const standing = (layout: Layout, frame: string) =>
    layout.frames.find((one) => one.id === frame)?.inserted;

  it("is nothing in a place nobody has put anything into", () => {
    const one = openedFrame({ ...EMPTY_LAYOUT, project: 1 }, 1, "/repo");
    expect(one.frame.inserted).toEqual([]);
  });

  it("is remembered against the pane it was put into", () => {
    const { layout, frame } = put();
    expect(standing(layout, frame)).toEqual([PICTURE]);
  });

  it("stays while the person goes on writing round it", () => {
    const { layout, frame } = put();
    const on = writing(layout, frame, `'${PICTURE}' look at this, it is the third one`);
    expect(standing(on, frame)).toEqual([PICTURE]);
  });

  // The send empties the box, and what was put into a body is nothing without the body.
  it("is forgotten when the box is emptied", () => {
    const { layout, frame } = put();
    expect(standing(writing(layout, frame, ""), frame)).toEqual([]);
  });

  // Whether the body still holds one is asked at the send, where the quoting the path went in under
  // is understood (`./terminal`). This keeps the list that is asked about.
  it("is kept while the box is not empty, even where the person deleted the path", () => {
    const { layout, frame } = put();
    expect(standing(writing(layout, frame, "look at this"), frame)).toEqual([PICTURE]);
  });

  it("is remembered once, however often the same path is put in", () => {
    const { layout, frame } = put();
    const again = writing(layout, frame, `'${PICTURE}' '${PICTURE}'`, [PICTURE]);
    expect(standing(again, frame)).toEqual([PICTURE]);
  });

  it("crosses with the draft, to the window the terminal is split out into", () => {
    const { layout, frame } = put();
    expect(standing(restored(laidOut(layout), 1), frame)).toEqual([PICTURE]);
  });

  it("is left out of the arrangement where nothing was put in", () => {
    const one = openedFrame({ ...EMPTY_LAYOUT, project: 1 }, 1, "/repo");
    const typed = writing(one.layout, one.frame.id, "run the tests");
    expect(laidOut(typed).frames[0]).not.toHaveProperty("inserted");
    expect(standing(restored(laidOut(typed), 1), one.frame.id)).toEqual([]);
  });

  // An arrangement written by a build that did not carry the draft, or one whose body was dropped
  // on the way: a remembered path with nothing standing over it would have the next send wait out
  // an agent that has no file to read.
  it("comes back only where the body it was put into came too", () => {
    const back = restored({
      project: 1,
      frames: [{ id: "1", project: 1, inserted: [PICTURE] }],
    }, 1);
    expect(back.frames[0]!.inserted).toEqual([]);
  });

  it("moves nothing the store keeps, so a paste is not a write to the disk", () => {
    const { layout } = put();
    const { frames: _put, ...rest } = laidOut(layout);
    const { frames: _empty, ...was } = laidOut(writing(layout, layout.frames[0]!.id, ""));
    expect(rest).toEqual(was);
  });
});

// The pane an automation run is drawn in (`AMB-T-5251`). Two things are pinned, and neither shows in
// code that looks right either way: one run is one pane however many steps it takes, and the pane
// arrives without moving the reader.
describe("the pane a run stands in", () => {
  it("is made once and answered again after that", () => {
    const first = stoodForRun({ ...EMPTY_LAYOUT, project: 1 }, 1, 7);
    expect(first.frame.run).toBe(7);
    expect(first.frame.id).toBe(runFrameId(7));

    const again = stoodForRun(first.layout, 1, 7);
    expect(again.layout.frames).toHaveLength(1);
    expect(again.frame.id).toBe(first.frame.id);
    expect(paneOfRun(again.layout, 7)!.id).toBe(first.frame.id);
  });

  it("moves neither the pane being worked in nor the page", () => {
    const { layout: one } = openedFrame({ ...EMPTY_LAYOUT, project: 1 }, 1, "/work/a");
    const { layout: after } = stoodForRun(one, 1, 7);
    expect(after.focus).toBe(one.focus);
    expect(after.page).toBe(one.page);
  });

  it("crosses to the other window and is not kept by the store", () => {
    const { layout } = stoodForRun({ ...EMPTY_LAYOUT, project: 1 }, 1, 7);
    // The shape the two windows hand the face over in carries it…
    const written = laidOut(layout);
    expect(written.frames[0]!.run).toBe(7);
    expect(restored(written, 1).frames[0]!.run).toBe(7);
    // …and a row that came back from the store carries none, because a run that was under way when
    // the app ended is stopped on the way up (`AMB-T-5247`).
    expect(restored({ project: 1, frames: [{ id: "1", project: 1 }] }, 1).frames[0]!.run).toBe(null);
  });
});
