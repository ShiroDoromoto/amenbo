// What the arrangement has to keep true, none of which is visible in the arithmetic that does it.
import { describe, expect, it } from "vitest";
import {
  closedIn, EMPTY_LAYOUT, focusOn, folderOfPage, frameFor, freeSlot, goPage, laidOut, MAX_PAGES,
  movedTo, openedIn, openedOn, pageCount, pageFor, pageOfFrame, restored, setCount, settledIn,
  sidesAreDrawers, slotsOf,
  type Layout,
} from "./layout";

/** A layout with `n` frames materialised at the given count, the way pressing through slots leaves one. */
function withFrames(n: number, count: Layout["count"] = 2): Layout {
  let layout: Layout = { ...EMPTY_LAYOUT, count };
  for (let i = 0; i < n; i++) {
    layout = frameFor(layout, Math.floor(i / count) + 1, i % count).layout;
  }
  return layout;
}

describe("pages and slots", () => {
  it("always offers one more page than the frames fill, so there is somewhere to put the next pane", () => {
    expect(pageCount(EMPTY_LAYOUT)).toBe(1);
    expect(pageCount(withFrames(1))).toBe(1);
    expect(pageCount(withFrames(2))).toBe(2);
    expect(pageCount(withFrames(4))).toBe(3);
  });

  it("stops at the digits there are to reach pages with", () => {
    expect(pageCount(withFrames(MAX_PAGES * 2))).toBe(MAX_PAGES);
    expect(goPage(withFrames(MAX_PAGES * 2), MAX_PAGES + 1).page).toBe(1);
  });

  it("fills the slots up to the one reached for, rather than leaving a hole in the page", () => {
    const { layout, frame } = frameFor({ ...EMPTY_LAYOUT, count: 4 }, 1, 3);
    expect(layout.frames.map((one) => one.id)).toEqual(["1", "2", "3", "4"]);
    expect(frame.id).toBe("4");
    expect(slotsOf(layout, 1).map((one) => one?.session)).toEqual([null, null, null, null]);
  });

  it("hands a slot the frame already there instead of a second one", () => {
    const first = frameFor(EMPTY_LAYOUT, 1, 0);
    const again = frameFor(first.layout, 1, 0);
    expect(again.frame.id).toBe(first.frame.id);
    expect(again.layout.frames).toHaveLength(1);
  });
});

describe("a frame is a place, not a process", () => {
  it("keeps the frame when the program in it exits", () => {
    const { layout, frame } = frameFor(EMPTY_LAYOUT, 1, 0);
    const running = openedIn(layout, frame.id, "s1", "/w");
    const ended = closedIn(running, "s1");
    expect(ended.frames).toHaveLength(1);
    expect(ended.frames[0]!.session).toBeNull();
    expect(ended.frames[0]!.id).toBe(frame.id);
  });

  it("never hands a retired id out again — a name is kept against it", () => {
    const four = withFrames(4);
    const ended = closedIn(openedIn(four, "1", "s1", null), "s1");
    expect(frameFor(ended, 3, 0).frame.id).toBe("5");
  });
});

describe("where another pane goes", () => {
  it("is the first slot on the page asked about, whichever page is showing", () => {
    // Two pages of two, with the first slot of page 2 taken: what page 2 has room for is its second.
    let layout = frameFor(frameFor(frameFor(EMPTY_LAYOUT, 1, 0).layout, 1, 1).layout, 2, 0).layout;
    layout = goPage(layout, 1);
    expect(freeSlot(layout, 2)).toBe(1);
    expect(freeSlot(layout, 1), "a full page has nowhere to put one").toBeNull();
  });

  it("always leaves one page with room, so there is always somewhere to start", () => {
    const layout = withFrames(5);
    expect(freeSlot(layout, pageCount(layout))).not.toBeNull();
  });
});

describe("one page, one project", () => {
  it("takes the page's folder from the first terminal started on it", () => {
    const two = openedIn(withFrames(2), "1", "s1", "/repo");
    expect(folderOfPage(two, 1)).toBe("/repo");
    // The next page is its own — a different project is a different page.
    expect(folderOfPage(two, 2)).toBeNull();
  });

  it("does not let an agent's own cd redraw where the page's panes are opened", () => {
    const two = openedIn(withFrames(2), "1", "s1", "/repo");
    expect(folderOfPage(movedTo(two, "s1", "/elsewhere"), 1)).toBe("/repo");
  });

  it("records where a pane is for a session that was adopted rather than started", () => {
    const adopted = openedIn(withFrames(2), "1", "s1", null);
    expect(folderOfPage(movedTo(adopted, "s1", "/said"), 1)).toBe("/said");
  });

  it("takes the page's folder from the choice, without waiting for a terminal to start in it", () => {
    // The state a machine with nothing startable is left in: a folder was answered and no terminal
    // followed. The slot beside it must still open there rather than ask the same question again.
    const chosen = settledIn(withFrames(2), "1", "/repo");
    expect(folderOfPage(chosen, 1)).toBe("/repo");
  });

  it("keeps the folder a frame already had — a second answer would change the page's project", () => {
    const chosen = settledIn(withFrames(2), "1", "/repo");
    expect(folderOfPage(settledIn(chosen, "1", "/elsewhere"), 1)).toBe("/repo");
  });
});

describe("the count is how much of the list a page shows", () => {
  it("carries the pane being worked in across a change of count", () => {
    // Four frames at two a page: frame 4 is on page 2. At one a page it is on page 4.
    const four = focusOn(withFrames(4), "4");
    expect(four.page).toBe(2);
    const one = setCount(four, 1);
    expect(pageOfFrame(one, "4")).toBe(4);
    expect(one.page).toBe(4);
  });

  it("does not renumber the frames — the list is what it was", () => {
    const four = withFrames(4);
    expect(setCount(four, 4).frames.map((one) => one.id)).toEqual(four.frames.map((one) => one.id));
  });

  it("lands on a page that exists when nothing is focused", () => {
    const wide = goPage(withFrames(8, 2), 5);
    expect(wide.page).toBe(5);
    expect(setCount(wide, 4).page).toBeLessThanOrEqual(pageCount(setCount(wide, 4)));
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
    let layout = frameFor(EMPTY_LAYOUT, 1, 1).layout;
    layout = openedIn(layout, "1", "session-a", "/work/repo");
    layout = openedIn(layout, "2", "session-b", "/work/repo");

    const kept = laidOut(layout);
    expect(kept.count).toBe(layout.count);
    expect(kept.frames).toEqual([
      { id: "1", folder: "/work/repo" },
      { id: "2", folder: "/work/repo" },
    ]);
    // What was running is not in it at all: a session died with the last run, and a pane drawn as
    // though it were still there would be the window saying something untrue.
    expect(JSON.stringify(kept)).not.toContain("session-a");
  });

  it("comes back as places to open a terminal in, in the same slots", () => {
    const back = restored({
      count: 4,
      nextId: 3,
      frames: [{ id: "1", folder: "/work/repo" }, { id: "2" }],
    })!;
    expect(back.count).toBe(4);
    expect(back.frames.map((one) => one.session)).toEqual([null, null]);
    expect(back.frames.map((one) => one.folder)).toEqual(["/work/repo", null]);
    // The face has to be working somewhere, and the first place is where a fresh one starts too.
    expect(back.focus).toBe("1");
    expect(back.page).toBe(1);
  });

  it("hands the next frame an id no name is already on", () => {
    // A kept arrangement whose `nextId` is behind its own frames — an older build, or a file nobody
    // can vouch for — must not let a fresh frame take the name of one that came back.
    const back = restored({ count: 2, nextId: 1, frames: [{ id: "1" }, { id: "7" }] })!;
    expect(frameFor(back, 2, 0).frame.id).toBe("8");
  });

  it("is nothing to come back to when it holds no frames", () => {
    expect(restored({ count: 2, nextId: 1, frames: [] })).toBeNull();
  });
});

describe("a folder handed in from the ledger", () => {
  it("lands on the page it is already open on, rather than beside itself", () => {
    const made = frameFor(EMPTY_LAYOUT, 2, 0);
    const open = settledIn(made.layout, made.frame.id, "/work/repo");
    expect(pageFor(open, "/work/repo")).toBe(2);
  });

  it("takes a page nothing has been started on, which on a fresh face is the one in front of you", () => {
    expect(pageFor(EMPTY_LAYOUT, "/work/repo")).toBe(1);
    const first = frameFor(EMPTY_LAYOUT, 1, 0);
    const busy = settledIn(first.layout, first.frame.id, "/work/other");
    expect(pageFor(busy, "/work/repo")).toBe(2);
  });

  it("is nowhere to go when every page is settled somewhere else", () => {
    let layout: Layout = { ...EMPTY_LAYOUT, count: 1 };
    for (let page = 1; page <= MAX_PAGES; page++) {
      const made = frameFor(layout, page, 0);
      layout = settledIn(made.layout, made.frame.id, `/work/${page}`);
    }
    expect(pageFor(layout, "/work/repo")).toBeNull();
  });

  // The frame is replaced rather than settled into: a pane already on the screen is drawn, and its
  // invitation would go on standing over a page that now has an answer.
  it("comes up as a frame of its own, so the pane is put up again", () => {
    const standing = frameFor(EMPTY_LAYOUT, 1, 0);
    const opened = openedOn(standing.layout, 1, "/work/repo");
    expect(opened.frame.id).not.toBe(standing.frame.id);
    expect(opened.frame.folder).toBe("/work/repo");
    expect(folderOfPage(opened.layout, 1)).toBe("/work/repo");
    expect(slotsOf(opened.layout, 1)[0]!.id).toBe(opened.frame.id);
  });

  it("leaves a slot with a terminal running in it exactly as it is", () => {
    const standing = frameFor(EMPTY_LAYOUT, 1, 0);
    const running = openedIn(standing.layout, standing.frame.id, "session-a", null);
    const opened = openedOn(running, 1, "/work/repo");
    expect(opened.frame.id).toBe(standing.frame.id);
    expect(opened.frame.session).toBe("session-a");
    expect(folderOfPage(opened.layout, 1)).toBeNull();
  });
});
