// What the arrangement has to keep true, none of which is visible in the arithmetic that does it.
import { describe, expect, it } from "vitest";
import {
  ACROSS, acrossIn, addPane, closedFrame, closedIn, COUNTS, DEFAULT_COUNT, DEFAULT_ORIENT,
  EMPTY_LAYOUT, focusOn, goPage, goProject, laidOut, movedTo, movedWithin, openedFrame, openedIn,
  ORIENTS, orientable, pageCount, pageOfFrame, pageShape, paneIn, panesOf, reordered, restored,
  roomOnPage, setCount, setOrient, slotsOf, writing, type Layout,
} from "./layout";

/** A layout with `n` panes opened in one project, the way pressing the way in `n` times leaves one.
 *  The count is pressed for rather than written in: a split is an answer given on a project, and one
 *  put straight into the shape would be a project drawn at a count nobody answered with. */
function withPanes(n: number, count: Layout["count"] = 2, project = 1): Layout {
  let layout: Layout = setCount({ ...EMPTY_LAYOUT, project }, count);
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
      { id: "1", project: 1, session: null, folder: null, written: "" },
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

  it("offers five counts and draws a project nobody has answered for at one", () => {
    // The split is an answer given on a project, so a project that has never been answered for is
    // drawn at the one pane that is certainly wanted — the wide splits are pressed for
    // (`./layout`).
    expect(COUNTS).toEqual([1, 2, 4, 6, 8]);
    expect(DEFAULT_COUNT).toBe(1);
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
    // draw rather than on a grid with no rule for it. The row is dropped rather than rounded: what
    // that project was left at is a thing this build does not know.
    const kept = laidOut(withPanes(2));
    expect(restored({ ...kept, count: 5, splits: { 1: { count: 5 } } }, null).count).toBe(DEFAULT_COUNT);
    expect(restored({ ...kept, count: 8, splits: { 1: { count: 8 } } }, null).count).toBe(8);
    // And an arrangement written before the answers were kept by project is read off the pair
    // beside them, which is all it has.
    expect(restored({ count: 5, nextId: 1, project: 1, frames: [] }, null).count).toBe(DEFAULT_COUNT);
    expect(restored({ count: 8, nextId: 1, project: 1, frames: [] }, null).count).toBe(8);
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

describe("the split each project was left at", () => {
  it("draws a project at its own answer, and brings each back on the way between them", () => {
    // How many panes a person wants is a fact about the work, not about the face: one project has an
    // agent and its shell in it, the next is one they read in. A face with a single count made every
    // move between the two rewrite whichever they came from.
    let layout = setCount({ ...EMPTY_LAYOUT, project: 1 }, 4);
    layout = setCount(goProject(layout, 2), 2);

    expect(goProject(layout, 1).count).toBe(4);
    expect(goProject(goProject(layout, 1), 2).count).toBe(2);
  });

  it("draws a project nobody has answered for at one, whatever the last one was set to", () => {
    const wide = setCount({ ...EMPTY_LAYOUT, project: 1 }, 8);
    expect(goProject(wide, 2).count).toBe(DEFAULT_COUNT);
    // And going back is the answer again, rather than the shape the unanswered project was drawn at.
    expect(goProject(goProject(wide, 2), 1).count).toBe(8);
  });

  it("moves the split with a pane reached for in another project, as a tab does", () => {
    // The rail's rows reach panes that are not on the screen, so reaching one is as much a move
    // between projects as pressing the tab is — and a split that followed only the tab would draw
    // one project at two counts depending on how the reader got to it.
    let layout = setCount({ ...EMPTY_LAYOUT, project: 1 }, 4);
    layout = openedFrame(layout, 1, "/work/1").layout;
    layout = setCount(openedFrame(layout, 2, "/work/2").layout, 2);

    expect(focusOn(layout, "1").count).toBe(4);
  });

  it("keeps the answers between runs, and writes none for a project nobody answered for", () => {
    let layout = setCount({ ...EMPTY_LAYOUT, project: 1 }, 4);
    layout = setOrient(setCount(goProject(layout, 2), 2), "down");
    // Walked through and left alone: an answer is what a person gave, and a row here would be one
    // put in their mouth.
    layout = goProject(layout, 3);

    const kept = laidOut(layout);
    expect(kept.splits).toEqual({ 1: { count: 4 }, 2: { count: 2, orient: "down" } });

    const back = restored(kept, null);
    expect(back.count).toBe(DEFAULT_COUNT);
    expect(goProject(back, 1).count).toBe(4);
    expect(goProject(back, 2)).toMatchObject({ count: 2, orient: "down" });
  });

  it("leaves the row out of an arrangement nobody has answered anything on", () => {
    expect(laidOut(EMPTY_LAYOUT)).not.toHaveProperty("splits");
    expect(laidOut(openedFrame({ ...EMPTY_LAYOUT, project: 1 }, 1, "/work/1").layout))
      .not.toHaveProperty("splits");
  });

  it("has nothing to keep an answer against where the face is on no project", () => {
    // The count still moves — the face draws what it was asked for — but there is nothing here to
    // hold the answer, so nothing is written down.
    const wide = setCount(EMPTY_LAYOUT, 4);
    expect(wide.count).toBe(4);
    expect(wide.splits).toEqual({});
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

  it("crosses a page the same way it crosses a pane — the pages are the list cut at the count", () => {
    // Four panes at two a page: the last pane of page two onto the first of page one is one move,
    // and there is no second operation for the page it left.
    const four = withPanes(4, 2);
    const moved = reordered(four, movedWithin(panesOf(four, 1), "4", "1", "before"));
    expect(slotsOf(moved, 1).map((one) => one.id)).toEqual(["4", "1"]);
    expect(slotsOf(moved, 2).map((one) => one.id)).toEqual(["2", "3"]);
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
    // The other project's pane is still the one in the middle of the whole list.
    expect(moved.frames.map((one) => one.id)).toEqual(["3", "2", "1"]);
    expect(idsOf(moved, 2)).toEqual(["2"]);
  });

  it("leaves the page and the pane being worked in where they are", () => {
    // The pane being worked in is on page one and is carried to page two. It is still the pane the
    // person is in, and the face is still on the page they were reading (`AMB-D-853`).
    const four = goPage(focusOn(withPanes(4, 2), "1"), 1);
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

  it("stays through a change of how many panes are on the screen", () => {
    const { layout, frame } = half();
    expect(setCount(layout, 4).frames.find((one) => one.id === frame)?.written)
      .toBe("run the tests");
  });

  it("is not written down with the arrangement, which keeps where the panes are and not what is in them", () => {
    const { layout } = half();
    expect(JSON.stringify(laidOut(layout))).not.toContain("run the tests");
  });

  it("is nothing again in the places an arrangement is restored into", () => {
    const { layout } = half();
    expect(restored(laidOut(layout), 1).frames.every((one) => one.written === "")).toBe(true);
  });
});
