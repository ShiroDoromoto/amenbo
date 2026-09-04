// What the arrangement has to keep true, none of which is visible in the arithmetic that does it.
import { describe, expect, it } from "vitest";
import {
  ACROSS, acrossIn, addPane, closedFrame, closedIn, COUNTS, DEFAULT_COUNT, DEFAULT_ORIENT,
  EMPTY_LAYOUT, focusOn, goPage, goProject, laidOut, movedTo, openedFrame, openedIn, ORIENTS,
  orientable, pageCount, pageOfFrame, pageShape, paneIn, panesOf, restored, roomOnPage, setCount,
  setOrient, slotsOf, type Layout,
} from "./layout";

/** A layout with `n` panes opened in one project, the way pressing the way in `n` times leaves one. */
function withPanes(n: number, count: Layout["count"] = 2, project = 1): Layout {
  let layout: Layout = { ...EMPTY_LAYOUT, count, project };
  for (let i = 0; i < n; i++) layout = openedFrame(layout, project, `/work/${project}`).layout;
  return layout;
}

describe("a place is made by opening one", () => {
  it("has no panes at all until something is opened", () => {
    expect(EMPTY_LAYOUT.frames).toHaveLength(0);
    expect(slotsOf(EMPTY_LAYOUT, 1)).toHaveLength(0);
    // Still a page: it is where the way in is put.
    expect(pageCount(EMPTY_LAYOUT)).toBe(1);
  });

  it("draws one slot per pane and no empty ones beside them", () => {
    const two = withPanes(2, 4);
    expect(slotsOf(two, 1).map((one) => one.id)).toEqual(["1", "2"]);
  });

  it("goes to the pane it just opened, on the page it landed on", () => {
    const made = openedFrame(withPanes(2), 1, "/work/1");
    expect(made.frame.id).toBe("3");
    expect(made.layout.focus).toBe("3");
    expect(made.layout.page).toBe(2);
  });
});

describe("a page with room says so, and a full one says nothing", () => {
  it("has room on the page the panes stop on, and none on the ones they fill", () => {
    const three = withPanes(3, 2);
    expect(roomOnPage(three, 1), "a full page had room in it").toBe(false);
    expect(roomOnPage(three, 2)).toBe(true);
  });

  it("has room on the one page of a project with nothing open", () => {
    expect(roomOnPage({ ...EMPTY_LAYOUT, project: 1 }, 1)).toBe(true);
  });

  it("has none anywhere when every page is filled to the count", () => {
    const four = withPanes(4, 2);
    expect([1, 2].map((page) => roomOnPage(four, page))).toEqual([false, false]);
  });
});

describe("asking for another pane", () => {
  it("goes to the page that has room, and makes no new one", () => {
    const three = goPage(withPanes(3, 2), 1);
    const asked = addPane(three);
    expect(asked.page).toBe(2);
    expect(pageCount(asked)).toBe(2);
  });

  it("brings a page into being where every page is full", () => {
    const two = withPanes(2, 2);
    expect(pageCount(two)).toBe(1);
    const asked = addPane(two);
    expect(asked.page).toBe(2);
    expect(pageCount(asked)).toBe(2);
    // Nothing is on it: what makes a place is opening one, and nobody has yet.
    expect(slotsOf(asked, 2)).toHaveLength(0);
  });

  it("takes that page away again as soon as the reader is somewhere else", () => {
    const asked = addPane(withPanes(2, 2));
    const back = goPage(asked, 1);
    expect(back.page).toBe(1);
    expect(pageCount(back), "an empty page outlived the asking").toBe(1);
  });

  it("makes it a page like any other once a pane is opened on it", () => {
    const asked = addPane(withPanes(2, 2));
    const made = openedFrame(asked, 1, "/work/1");
    expect(made.layout.adding).toBe(false);
    expect(pageCount(made.layout)).toBe(2);
    expect(slotsOf(made.layout, 2).map((one) => one.id)).toEqual(["3"]);
  });

  it("does not survive a change of split, which is measured on the other count", () => {
    const asked = addPane(withPanes(2, 2));
    expect(pageCount(setCount(asked, 4))).toBe(1);
  });

  it("is no part of the arrangement that is kept", () => {
    expect(JSON.stringify(laidOut(addPane(withPanes(2, 2))))).not.toContain("adding");
  });
});

describe("a pane belongs to a project", () => {
  it("shows one project's panes and not another's", () => {
    let layout = withPanes(2, 2, 1);
    layout = openedFrame(layout, 2, "/work/2").layout;
    expect(layout.project).toBe(2);
    expect(slotsOf(layout, 1).map((one) => one.id)).toEqual(["3"]);
    expect(panesOf(layout, 1).map((one) => one.id)).toEqual(["1", "2"]);
  });

  it("lands on a project's first pane when it is picked", () => {
    let layout = withPanes(2, 2, 1);
    layout = openedFrame(layout, 2, "/work/2").layout;
    const back = goProject(layout, 1);
    expect(back.page).toBe(1);
    expect(back.focus).toBe("1");
  });

  it("takes the screen to another project when a pane there is reached for", () => {
    let layout = withPanes(1, 2, 1);
    layout = openedFrame(layout, 2, "/work/2").layout;
    const back = focusOn(layout, "1");
    expect(back.project).toBe(1);
    expect(back.focus).toBe("1");
  });

  it("counts a project's pages from its own panes", () => {
    let layout = withPanes(4, 2, 1);
    layout = openedFrame(layout, 2, "/work/2").layout;
    expect(pageCount(layout)).toBe(1);
    expect(pageCount(goProject(layout, 1))).toBe(2);
  });
});

describe("a frame is a place, not a process", () => {
  it("keeps the frame when the program in it exits", () => {
    const { layout, frame } = openedFrame(EMPTY_LAYOUT, 1, "/w");
    const ended = closedIn(openedIn(layout, frame.id, "s1", "/w"), "s1");
    expect(ended.frames).toHaveLength(1);
    expect(ended.frames[0]!.session).toBeNull();
    expect(ended.frames[0]!.id).toBe(frame.id);
  });

  it("never hands a retired id out again — a name is kept against it", () => {
    const four = withPanes(4);
    const ended = closedIn(openedIn(four, "1", "s1", null), "s1");
    expect(openedFrame(ended, 1, "/w").frame.id).toBe("5");
  });
});

describe("closing a pane takes the place away", () => {
  it("is gone for good, and what is left closes up", () => {
    const three = withPanes(3, 2);
    const left = closedFrame(three, "1");
    expect(left.frames.map((one) => one.id)).toEqual(["2", "3"]);
    // Two panes at two a page is one page: the last page lost its slot rather than keeping a hole.
    expect(pageCount(left)).toBe(1);
    expect(slotsOf(left, 1).map((one) => one.id)).toEqual(["2", "3"]);
  });

  it("does not hand the closed pane's id out again", () => {
    const left = closedFrame(withPanes(2), "2");
    expect(openedFrame(left, 1, "/w").frame.id).toBe("3");
  });

  it("leaves the reader on whatever moved into its place", () => {
    const three = focusOn(withPanes(3, 2), "2");
    expect(closedFrame(three, "2").focus).toBe("3");
  });

  it("leaves them on the pane before it where nothing moved up", () => {
    const three = focusOn(withPanes(3, 2), "3");
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
    const three = focusOn(withPanes(3, 2), "1");
    expect(closedFrame(three, "3").focus).toBe("1");
  });

  it("is nothing at all for an id no frame has", () => {
    const three = withPanes(3, 2);
    expect(closedFrame(three, "9")).toBe(three);
  });
});

describe("where a pane works", () => {
  it("is the folder it was opened in, which an agent's own cd does not redraw", () => {
    const one = openedIn(withPanes(1), "1", "s1", "/repo");
    expect(movedTo(one, "s1", "/elsewhere").frames[0]!.folder).toBe("/repo");
  });

  it("is learned from the session for a pane that took one up rather than starting it", () => {
    const adopted = openedIn({ ...EMPTY_LAYOUT, project: 1, frames: [
      { id: "1", project: 1, session: null, folder: null },
    ], nextId: 2 }, "1", "s1", null);
    expect(movedTo(adopted, "s1", "/said").frames[0]!.folder).toBe("/said");
  });
});

describe("the count is the most a page draws", () => {
  it("carries the pane being worked in across a change of count", () => {
    const four = focusOn(withPanes(4), "4");
    expect(four.page).toBe(2);
    const one = setCount(four, 1);
    expect(pageOfFrame(one, "4")).toBe(4);
    expect(one.page).toBe(4);
  });

  it("does not renumber the panes — the list is what it was", () => {
    const four = withPanes(4);
    expect(setCount(four, 4).frames.map((one) => one.id)).toEqual(four.frames.map((one) => one.id));
  });

  it("lands on a page that exists when nothing is focused", () => {
    const wide = goPage({ ...withPanes(8, 2), focus: null }, 4);
    expect(wide.page).toBe(4);
    const wider = setCount(wide, 4);
    expect(wider.page).toBeLessThanOrEqual(pageCount(wider));
  });

  it("refuses a page this project has not got", () => {
    expect(goPage(withPanes(2), 3).page).toBe(1);
  });

  it("offers five counts and comes up on two", () => {
    // Two is where a first run lands: one terminal is what there was before, and the wide splits are
    // arrived at rather than handed out (`./layout`).
    expect(COUNTS).toEqual([1, 2, 4, 6, 8]);
    expect(DEFAULT_COUNT).toBe(2);
    // Every count says how many go across, and no count ever asks for a third row — whichever way
    // the one count that can be asked is laid.
    for (const one of COUNTS) {
      expect(ACROSS[one]).toBeGreaterThan(0);
      for (const orient of ORIENTS) {
        expect(acrossIn(one, orient)).toBeGreaterThan(0);
        expect(one / acrossIn(one, orient)).toBeLessThanOrEqual(2);
      }
    }
  });

  it("asks about two panes and about no other count", () => {
    // Four and above have spent their rows already, and one has nothing to arrange: two is the count
    // where spending width first stops paying (`./layout`).
    expect(COUNTS.filter(orientable)).toEqual([2]);
    expect(DEFAULT_ORIENT).toBe("across");
    // Down is the one that turns the count around; across is what every count does.
    expect(acrossIn(2, "across")).toBe(2);
    expect(acrossIn(2, "down")).toBe(1);
    expect(acrossIn(4, "down")).toBe(ACROSS[4]);
  });

  it("names a grid by the answer only where there is one to give", () => {
    // The class is what a page is laid out by, so a count that cannot be asked is named by its number
    // alone — a second name for the same grid would be a second grid to keep in step.
    expect(pageShape(2, "across")).toBe("2");
    expect(pageShape(2, "down")).toBe("2-down");
    expect(pageShape(4, "down")).toBe("4");
  });

  it("lays the two panes the other way without moving any of them", () => {
    // The count is how many a page holds, so the pages are the same pages and the reader is in the
    // pane they were in: what changed is where the two are drawn.
    const two = focusOn(withPanes(3, 2), "3");
    const down = setOrient(two, "down");
    expect(down.orient).toBe("down");
    expect(down.page).toBe(two.page);
    expect(down.focus).toBe("3");
    expect(slotsOf(down, 2).map((one) => one.id)).toEqual(slotsOf(two, 2).map((one) => one.id));
  });

  it("keeps the orientation across a count that cannot be asked about it", () => {
    // A person who went to four and asked for two again means the two they set up, not the default
    // back — so the answer stands at every count and is drawn on at one.
    const down = setOrient(withPanes(2), "down");
    expect(setCount(setCount(down, 4), 2).orient).toBe("down");
  });

  it("draws the count that was pressed for, however few panes are open", () => {
    // Three panes on a count of eight is one page with room on it, not a page that shrank to three:
    // the shape is the press, and the gaps past the empty frame stay blank.
    const wide = withPanes(3, 8);
    expect(pageCount(wide)).toBe(1);
    expect(slotsOf(wide, 1)).toHaveLength(3);
    expect(roomOnPage(wide, 1)).toBe(true);
  });

  it("keeps an orientation it has never heard of out of a kept arrangement", () => {
    // The same as an unknown count: what comes back has to be something the stylesheet has a grid
    // for, and there are two.
    const kept = { ...laidOut(withPanes(2)), orient: "sideways" as Layout["orient"] };
    expect(restored(kept, null).orient).toBe(DEFAULT_ORIENT);
    // What was asked for comes back, and what was never asked stays out of the row.
    expect(laidOut(setOrient(withPanes(2), "down")).orient).toBe("down");
    expect(laidOut(withPanes(2))).not.toHaveProperty("orient");
    expect(restored(laidOut(setOrient(withPanes(2), "down")), null).orient).toBe("down");
  });

  it("keeps a count it has never heard of out of a kept arrangement", () => {
    // A build that offered some other count wrote one, and this one has to land on something it can
    // draw rather than on a grid with no rule for it.
    const kept = { ...laidOut(withPanes(2)), count: 5 as Layout["count"] };
    expect(restored(kept, null)!.count).toBe(DEFAULT_COUNT);
    expect(restored({ ...laidOut(withPanes(2)), count: 8 }, null)!.count).toBe(8);
  });
});

describe("an arrangement kept between runs", () => {
  it("keeps the shape and lets the sessions go", () => {
    let layout = withPanes(2);
    layout = openedIn(layout, "1", "session-a", "/work/1");
    layout = openedIn(layout, "2", "session-b", "/work/1");

    const kept = laidOut(layout);
    expect(kept.count).toBe(layout.count);
    expect(kept.frames).toEqual([
      { id: "1", project: 1, folder: "/work/1" },
      { id: "2", project: 1, folder: "/work/1" },
    ]);
    // What was running is not in it at all: a session died with the last run, and a pane drawn as
    // though it were still there would be the window saying something untrue.
    expect(JSON.stringify(kept)).not.toContain("session-a");
  });

  it("carries the project the face is on, for the window that has no ledger to be asked on", () => {
    // The window the terminal is split out into has no ledger to have taken a project from, so an
    // arrangement with no panes to name one opens as the project the board was showing
    // (`../shell/TerminalFace`).
    expect(laidOut(withPanes(1)).project).toBe(1);
    // A face that has not been told of a project says so by leaving it out, rather than naming one
    // no pane is in.
    expect(laidOut(EMPTY_LAYOUT)).not.toHaveProperty("project");
  });

  it("names the pane being worked in, for the window that comes up on it", () => {
    // The press that splits hands nothing over, so where the reader was is theirs to read back out
    // of the shape (`../shell/TerminalFace`).
    const layout = focusOn(withPanes(2), "2");
    expect(laidOut(layout).splitOut).toBe("2");
    // A face with no pane to be working in leaves it out rather than naming a place that is not one.
    expect(laidOut(EMPTY_LAYOUT)).not.toHaveProperty("splitOut");
  });

  it("comes back as places to open a terminal in, each in its own project", () => {
    const back = restored({
      count: 4,
      nextId: 3,
      frames: [{ id: "1", project: 7, folder: "/work/repo" }, { id: "2", project: 8 }],
    }, null);
    expect(back.count).toBe(4);
    expect(back.frames.map((one) => one.session)).toEqual([null, null]);
    expect(back.frames.map((one) => one.folder)).toEqual(["/work/repo", null]);
    // The face has to be showing something, and the first pane is where a fresh one starts too.
    expect(back.project).toBe(7);
    expect(back.focus).toBe("1");
    expect(back.page).toBe(1);
  });

  it("puts a pane whose project nothing recorded where the person is looking", () => {
    const back = restored({ count: 2, nextId: 2, frames: [{ id: "1", folder: "/work/repo" }] }, 5);
    expect(back.frames[0]!.project).toBe(5);
  });

  it("has nowhere to put one when the window is on no project either", () => {
    expect(restored({ count: 2, nextId: 2, frames: [{ id: "1" }] }, null).frames).toHaveLength(0);
  });

  it("hands the next frame an id no name is already on", () => {
    // An arrangement whose `nextId` is behind its own frames — an older build, or a hand-over nobody
    // can vouch for — must not let a fresh frame take the name of one that came with it.
    const back = restored({ count: 2, nextId: 1, frames: [{ id: "1", project: 1 }, { id: "7", project: 1 }] }, null);
    expect(openedFrame(back, 1, "/w").frame.id).toBe("8");
  });

  it("brings the split back with no frames to draw it with", () => {
    // What every window that comes up after a run reads: the frames are not kept, and the split the
    // person chose is (`AMB-T-3687`). It is the empty face, laid out the way they laid it out.
    const back = restored({ count: 4, nextId: 1, project: 3, frames: [] }, 3);
    expect(back.count).toBe(4);
    expect(back.frames).toHaveLength(0);
    expect(back.project).toBe(3);
    expect(back.focus).toBeNull();
    expect(pageCount(back)).toBe(1);
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
