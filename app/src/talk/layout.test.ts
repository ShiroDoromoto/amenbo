// What the arrangement has to keep true, none of which is visible in the arithmetic that does it.
import { describe, expect, it } from "vitest";
import {
  closedIn, EMPTY_LAYOUT, focusOn, goPage, goProject, laidOut, movedTo, openedFrame, openedIn,
  pageCount, pageOfFrame, paneIn, panesOf, restored, setCount, sidesAreDrawers, slotsOf,
  type Layout,
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
});

describe("the sides", () => {
  it("are drawers when one pane was asked for, however wide the window", () => {
    expect(sidesAreDrawers(1, 2560)).toBe(true);
  });

  it("are drawers on a narrow window, whatever count was asked for", () => {
    expect(sidesAreDrawers(4, 700)).toBe(true);
    expect(sidesAreDrawers(2, 700)).toBe(true);
  });

  it("are columns otherwise", () => {
    expect(sidesAreDrawers(2, 1400)).toBe(false);
    expect(sidesAreDrawers(4, 1400)).toBe(false);
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

  it("carries the project the face is on, for the window that has no rail to be asked on", () => {
    // The split-out window draws one pane and nobody chose it on its way in, so what it opens as is
    // the project the board was showing (`../talk.ts`).
    expect(laidOut(withPanes(1)).project).toBe(1);
    // A face that has not been told of a project says so by leaving it out, rather than naming one
    // no pane is in.
    expect(laidOut(EMPTY_LAYOUT)).not.toHaveProperty("project");
  });

  it("comes back as places to open a terminal in, each in its own project", () => {
    const back = restored({
      count: 4,
      nextId: 3,
      frames: [{ id: "1", project: 7, folder: "/work/repo" }, { id: "2", project: 8 }],
    }, null)!;
    expect(back.count).toBe(4);
    expect(back.frames.map((one) => one.session)).toEqual([null, null]);
    expect(back.frames.map((one) => one.folder)).toEqual(["/work/repo", null]);
    // The face has to be showing something, and the first pane is where a fresh one starts too.
    expect(back.project).toBe(7);
    expect(back.focus).toBe("1");
    expect(back.page).toBe(1);
  });

  it("puts a pane whose project nothing recorded where the person is looking", () => {
    const back = restored({ count: 2, nextId: 2, frames: [{ id: "1", folder: "/work/repo" }] }, 5)!;
    expect(back.frames[0]!.project).toBe(5);
  });

  it("has nowhere to put one when the window is on no project either", () => {
    expect(restored({ count: 2, nextId: 2, frames: [{ id: "1" }] }, null)).toBeNull();
  });

  it("hands the next frame an id no name is already on", () => {
    // A kept arrangement whose `nextId` is behind its own frames — an older build, or a file nobody
    // can vouch for — must not let a fresh frame take the name of one that came back.
    const back = restored({ count: 2, nextId: 1, frames: [{ id: "1", project: 1 }, { id: "7", project: 1 }] }, null)!;
    expect(openedFrame(back, 1, "/w").frame.id).toBe("8");
  });

  it("is nothing to come back to when it holds no frames", () => {
    expect(restored({ count: 2, nextId: 1, frames: [] }, 1)).toBeNull();
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
