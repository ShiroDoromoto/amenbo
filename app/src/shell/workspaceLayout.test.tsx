// @vitest-environment jsdom
// What the arrangement has to do to the panes, which the layout's own tests cannot see: they know
// where a pane is, not whether turning a page killed the terminal in one.
//
// Three things are pinned here, each invisible in code that looks right either way. A pane is made by
// opening one — a face with nothing open draws one way in and no boxes, and a question walked away
// from leaves nothing behind. A pane opens in a folder of the project it belongs to, and in nothing
// else, which is what keeps one screen to one project (`../talk/layout`). And turning a page takes
// no pane down — every page is drawn side by side and scrolled to — so the terminal the reader left on
// one is the same terminal when they come back, not one started again or read back in.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PaneStart } from "../talk/terminal";

type Mounted = { start: PaneStart; said: (statement: unknown) => void; session: string };

const hoisted = vi.hoisted(() => ({
  mounts: [] as unknown[],
  detached: 0,
  /** The folders the project being shown is bound to. */
  folders: [{ path: "/repo", exists: true }] as { path: string; exists: boolean }[],
}));

// The frame a slot puts up, stood in for. It answers the way a real one does — a session id comes
// back, and what the agent in it says arrives through the same callback — so the face has something
// to arrange. What agent runs in it is the frame's own question and not this one's (`../talk/agent`).
vi.mock("../talk/agent", () => ({
  mountAgentFrame: (
    _host: HTMLElement,
    _lang: string,
    on: { opened: (s: string) => void; said: (statement: unknown) => void },
    start: PaneStart = {},
  ) => {
    const session = start.session ?? `s${hoisted.mounts.length + 1}`;
    hoisted.mounts.push({ start, said: on.said, session });
    on.opened(session);
    return Promise.resolve(() => { hoisted.detached++; });
  },
}));

// The ledger's projects and the folders each is bound to. Both are reads the face makes of the store,
// and what is under test is what it does with the answers.
vi.mock("../mock/adapter", () => ({
  dataAdapter: { listProjects: () => [{ id: 1, name: "amenbo" }] },
}));
vi.mock("../core/boundFolders", () => ({
  useBoundFolders: () => ({ all: hoisted.folders, live: hoisted.folders, answered: true }),
}));
// Taking a place away asks first (`./TerminalPane`). The asking itself is that component's own test;
// here the answer is always yes, so what this file sees is what the face does with it.
vi.mock("../core/dialog", () => ({ confirmDialog: async () => true }));

import { ACROSS, BOXES, DOWN, type Size } from "../talk/layout";
import { t } from "../core/i18n";
import { SCROLL_REST_MS, WorkspaceFace } from "./WorkspaceFace";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

const mounts = () => hoisted.mounts as Mounted[];
const q = (sel: string) => [...container.querySelectorAll<HTMLElement>(sel)];
/** The page being read. Every page of the project is drawn side by side (`./WorkspaceFace`), so what
 *  a reader sees is what is on this one. */
const shown = (host: ParentNode = container) =>
  host.querySelector<HTMLElement>(".workspace__page-grid[aria-current=\"page\"]")!;
/** What is on the page being read. */
const here = (sel: string) => [...shown().querySelectorAll<HTMLElement>(sel)];
const click = async (el: HTMLElement) => {
  await act(async () => { el.dispatchEvent(new MouseEvent("click", { bubbles: true })); });
};
/** Go to a page by pressing its number in the row beside the panes. */
const goPage = async (n: number) => { await click(q(".workspace__page")[n - 1]!); };
/** Open another pane in the project being shown. A page with room draws an empty frame and that is
 *  the one press; a full page draws the strip instead, which goes to a page with room — bringing one
 *  into being where every page is full — and the empty frame there is what opens it
 *  (`../talk/layout`). */
const openPaneIn = async (host: HTMLElement) => {
  const strip = [...shown(host).querySelectorAll<HTMLElement>(".workspace__addstrip")][0];
  if (strip) await click(strip);
  await click([...shown(host).querySelectorAll<HTMLElement>(".slot--empty .slot__open")][0]!);
};
const openPane = () => openPaneIn(container);
/** The page as 1200 across and 400 down at the origin, so one cell is 100 wide and one row 200.
 *  jsdom measures nothing, so the page answers for its own rectangle and the drag is arithmetic on
 *  what it says. */
const pageIs1200By400 = () => {
  shown().getBoundingClientRect = () => ({
    top: 0, left: 0, width: 1200, height: 400, right: 1200, bottom: 400,
    x: 0, y: 0, toJSON: () => ({}),
  }) as DOMRect;
};
/** Pull the corner of the pane the page is showing out to a size, which is the one way a person sets
 *  one (`AMB-D-939`, `./paneDrag`). The pane is the one being worked in where it is on this page and
 *  the last one of the page where it is not — the same pane a new one is measured against
 *  (`../talk/layout`). The first pane of a project opens at the whole page, so a road about pages,
 *  gaps and the strip beside them sizes it first. */
const atSize = async (size: Size) => {
  pageIs1200By400();
  const open = here(".slot:not(.slot--empty)");
  const slot = open.find((one) => one.classList.contains("slot--focused")) ?? open[open.length - 1]!;
  // Where the pane stands on the grid, which is what the corner is pulled from: the cell it is let
  // go over is that spot plus the box the size comes to (`./paneDrag`).
  const at = {
    across: Number(slot.style.gridColumn.split(" ")[0]) - 1,
    down: Number(slot.style.gridRow.split(" ")[0]) - 1,
  };
  const to = {
    x: (at.across + BOXES[size].across) * (1200 / ACROSS),
    y: (at.down + BOXES[size].down) * (400 / DOWN),
  };
  document.elementFromPoint = () => null;
  await act(async () => {
    slot.querySelector<HTMLElement>(".slot__corner")!.dispatchEvent(
      new MouseEvent("pointerdown", { bubbles: true, button: 0, clientX: 0, clientY: 0 }),
    );
    document.dispatchEvent(new MouseEvent("pointermove", { clientX: to.x, clientY: to.y }));
    // The one hit test a frame the move asks for.
    await new Promise((done) => setTimeout(done, 20));
  });
  await act(async () => {
    document.dispatchEvent(new MouseEvent("pointerup", { clientX: to.x, clientY: to.y }));
  });
};
/** Put the face up. It is not in `beforeEach` because what the project is bound to is set per test,
 *  and the face reads it as it comes up. */
const mount = async () => {
  await act(async () => {
    root.render(createElement(WorkspaceFace, { onWindow: () => {}, note: null }));
  });
};

beforeEach(() => {
  // The face measures the window to work out whether the columns beside the panes are columns at all
  // (`../talk/columns`). jsdom's window is 1024, which is genuinely too narrow for two panes and two
  // columns — so a test about what is drawn beside the panes says it is on a wide screen.
  window.innerWidth = 1600;
  hoisted.mounts = [];
  hoisted.detached = 0;
  hoisted.folders = [{ path: "/repo", exists: true }];
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the face comes up with nothing open", () => {
  it("draws one way in and no boxes beside it", async () => {
    await mount();
    expect(mounts()).toHaveLength(0);
    expect(q(".slot")).toHaveLength(1);
    expect(q(".slot--empty")).toHaveLength(1);
  });

  it("draws no page numbers, because there is nowhere to go from one page", async () => {
    await mount();
    expect(q(".workspace__page")).toHaveLength(0);
  });
});

describe("a pane works in a folder of its project", () => {
  it("asks nothing where the project is bound to one folder", async () => {
    await mount();
    await openPane();
    expect(mounts()).toHaveLength(1);
    expect(mounts()[0]!.start.cwd).toBe("/repo");
  });

  it("opens the next pane there too, and does not ask again", async () => {
    await mount();
    await openPane();
    await openPane();
    expect(mounts().map((one) => one.start.cwd)).toEqual(["/repo", "/repo"]);
  });

  it("tells each pane which place it is, so what is started there can be come back to", async () => {
    await mount();
    await openPane();
    await openPane();
    // Two panes, two places — a way back into a session is written down against the frame it was
    // started in (`AMB-D-869`), so a pane handed somebody else's would come back into their
    // conversation.
    const places = mounts().map((one) => one.start.frame);
    expect(places).toHaveLength(2);
    expect(new Set(places).size, `two panes, two places: ${places.join(", ")}`).toBe(2);
    expect(places.every((frame) => typeof frame === "string" && frame.length > 0)).toBe(true);
  });

  it("asks which folder where the project is bound to several, and makes no pane until it is answered", async () => {
    hoisted.folders = [{ path: "/repo", exists: true }, { path: "/site", exists: true }];
    await mount();
    await openPane();
    expect(mounts(), "a pane was opened before the question was answered").toHaveLength(0);
    const choices = here(".slot--asking .agent__choice");
    expect(choices.map((one) => one.textContent)).toEqual(["/repo", "/site"]);

    await click(choices[1]!);
    expect(mounts()).toHaveLength(1);
    expect(mounts()[0]!.start.cwd).toBe("/site");
    expect(here(".slot--asking"), "the question stayed up behind the pane").toHaveLength(0);
  });

  it("leaves nothing behind when the question is walked away from", async () => {
    hoisted.folders = [{ path: "/repo", exists: true }, { path: "/site", exists: true }];
    await mount();
    await openPane();
    await click(here(".slot--asking .agent__choice")[0]!);
    await atSize("half");
    // The question about the second pane, walked away from: resizing a pane, like going to a pane or
    // a project, is a person doing something else, and the question goes with it.
    await click(here(".slot--empty .slot__open")[0]!);
    expect(here(".slot--asking")).toHaveLength(1);
    await atSize("quarter");
    expect(here(".slot--asking")).toHaveLength(0);
    expect(here(".slot--empty"), "a place was left where nothing was opened").toHaveLength(1);
  });
});

describe("turning a page", () => {
  /** The row the pages are laid along, as 1200 across, scrolled to this page and then wherever it is
   *  asked to be. jsdom scrolls nothing, so the row answers for its own width and keeps its own
   *  scroll. */
  const trackIs1200 = (page: number) => {
    const track = q(".workspace__track")[0]!;
    let left = (page - 1) * 1200;
    Object.defineProperty(track, "clientWidth", { configurable: true, value: 1200 });
    Object.defineProperty(track, "scrollLeft", {
      configurable: true, get: () => left, set: (to: number) => { left = to; },
    });
    const asked: number[] = [];
    Object.defineProperty(track, "scrollTo", {
      configurable: true, value: (to: ScrollToOptions) => { asked.push(to.left!); left = to.left!; },
    });
    return { track, asked };
  };
  /** Three panes at a half each, so two pages. */
  const twoPages = async () => {
    await mount();
    await openPane();
    await atSize("half");
    await openPane();
    await openPane();
    expect(q(".workspace__page")).toHaveLength(2);
  };

  it("keeps every page's panes up, and never starts or picks up a terminal again", async () => {
    await twoPages();
    expect(q(".workspace__page-grid"), "a page that is not being read was not drawn").toHaveLength(2);

    await goPage(1);
    await goPage(2);
    await goPage(1);
    expect(hoisted.detached, "a pane was taken down on a page nobody is on").toBe(0);
    expect(mounts(), "turning a page put a terminal up again").toHaveLength(3);
  });

  it("scrolls the row to the page a digit is pressed for", async () => {
    await twoPages();                              // on page 2, where the third pane opened
    const { asked } = trackIs1200(2);

    await goPage(1);
    expect(asked).toEqual([0]);
    await goPage(2);
    expect(asked).toEqual([0, 1200]);
    expect(shown()).toBe(q(".workspace__page-grid")[1]);
    // The page scrolled away is still drawn, and is nobody's to reach until it is scrolled back.
    expect(q(".workspace__page-grid")[0]!.hasAttribute("inert")).toBe(true);
    expect(shown().hasAttribute("inert")).toBe(false);
  });

  it("scrolls the row to the page a press on a pane moved the screen to", async () => {
    await mount();
    await openPane();
    await atSize("half");
    await openPane();                              // two halves on page 1
    const { asked } = trackIs1200(1);

    // The pane being worked in, given the whole page, starts page 2 and the screen follows it.
    await atSize("whole");
    expect(q(".workspace__page--on")[0]!.textContent).toBe("2");
    expect(asked).toEqual([1200]);
  });

  it("goes to the page the row came to rest on", async () => {
    await twoPages();
    await goPage(1);
    const { track } = trackIs1200(1);

    // A swipe: the row moves, and nothing is written until it has stopped.
    await act(async () => {
      track.scrollLeft = 700;
      track.dispatchEvent(new Event("scroll"));
    });
    expect(q(".workspace__page--on")[0]!.textContent).toBe("1");
    await act(async () => {
      track.scrollLeft = 1200;
      track.dispatchEvent(new Event("scroll"));
      await new Promise((done) => setTimeout(done, SCROLL_REST_MS + 30));
    });
    expect(q(".workspace__page--on")[0]!.textContent).toBe("2");
    expect(shown()).toBe(q(".workspace__page-grid")[1]);
  });
});

describe("the empty frame", () => {
  it("is one on a page with room, and none on a full one", async () => {
    await mount();
    await openPane();
    await atSize("half");
    // One pane at half the page: the other half is a gap, and one frame says so.
    expect(here(".slot--empty")).toHaveLength(1);

    await openPane();
    expect(here(".slot--empty"), "a full page offered somewhere to open a pane").toHaveLength(0);
  });

  it("is on the page the strip goes to, which it brings into being when every page is full", async () => {
    await mount();
    await openPane();
    await atSize("half");
    await openPane();                              // page 1 full at half the page each
    await click(here(".workspace__addstrip")[0]!);

    expect(q(".workspace__page")).toHaveLength(2);
    expect(q(".workspace__page--on")[0]!.textContent).toBe("2");
    expect(here(".slot--empty")).toHaveLength(1);
    expect(mounts(), "asking for room opened a terminal by itself").toHaveLength(2);
  });
});

describe("taking a pane away", () => {
  it("takes the place off the face and closes the page up", async () => {
    await mount();
    await openPane();
    await atSize("half");
    await openPane();
    await openPane();                              // three panes at a half each, so two pages
    expect(q(".workspace__page")).toHaveLength(2);

    await act(async () => { here(".slot__end")[0]!.click(); });
    await act(async () => { await Promise.resolve(); });

    // Two panes left, both on one page — and the page nobody can go to any more is gone with them.
    expect(here(".slot")).toHaveLength(2);
    expect(q(".workspace__page")).toHaveLength(0);
  });
});

describe("how much of the page a pane takes", () => {
  it("carries the pane being resized over to the page it is on now", async () => {
    await mount();
    await openPane();
    await atSize("half");
    await openPane();                              // two panes, both on page 1 at a half each
    await atSize("whole");

    // The pane being worked in took the whole page, so it is page 2 — and that is where the screen
    // is, because the press was about that pane.
    expect(q(".workspace__page--on")[0]!.textContent).toBe("2");
    expect(here(".slot")).toHaveLength(1);
    // Drawn on another page is put up again there, and on the terminal it had.
    const [, carried, again] = mounts();
    expect(again!.start.session, "the pane carried across was given another terminal").toBe(carried!.session);
  });

  it("puts the way in beside the panes once the page is full, and nowhere else", async () => {
    await mount();
    await openPane();
    await atSize("half");
    // A page with a gap draws the empty frame, and that frame is the way in. A second one beside it
    // would be the same offer twice.
    expect(here(".workspace__addstrip")).toHaveLength(0);

    await openPane();                              // two halves, so the page is now full
    expect(here(".slot--empty")).toHaveLength(0);
    expect(here(".workspace__addstrip")).toHaveLength(1);

    // Pressing it goes to where the room is, which is the next page — the same thing the rail's own
    // way in does.
    await click(here(".workspace__addstrip")[0]!);
    expect(q(".workspace__page--on")[0]!.textContent).toBe("2");
    expect(here(".slot--empty")).toHaveLength(1);
  });

  it("draws the pane at the size that was asked for, and leaves the rest of the page blank", async () => {
    await mount();
    await openPane();
    await atSize("quarter");                       // a quarter of the page, with one pane open

    // The box is the size. A pane that grew to fill what is open would make the control look as
    // though it had done nothing.
    const slot = here(".slot:not(.slot--empty)")[0]!;
    expect(slot.style.gridColumn).toBe("1 / span 6");
    expect(slot.style.gridRow).toBe("1 / span 1");
    expect(here(".slot--empty")).toHaveLength(1);
  });

  it("takes half the page both ways round", async () => {
    await mount();
    await openPane();

    await atSize("half");
    expect(here(".slot:not(.slot--empty)")[0]!.style.gridColumn).toBe("1 / span 6");
    expect(here(".slot:not(.slot--empty)")[0]!.style.gridRow).toBe("1 / span 2");

    await atSize("half-down");
    expect(here(".slot:not(.slot--empty)")[0]!.style.gridColumn).toBe("1 / span 12");
    expect(here(".slot:not(.slot--empty)")[0]!.style.gridRow).toBe("1 / span 1");
  });

  it("is the pane being worked in that it is about, and each pane keeps its own answer", async () => {
    await mount();
    await openPane();
    await atSize("quarter");
    await openPane();                              // the second pane is a quarter too
    await atSize("sixth");                         // and only it is resized

    const boxes = here(".slot:not(.slot--empty)").map((one) => one.style.gridColumn);
    expect(boxes).toEqual(["1 / span 6", "7 / span 4"]);
  });
});

// The two gestures the page itself carries (`AMB-D-939`): a pane taken by its row and carried past
// another, and a pane's corner pulled to a size. The arithmetic behind both is `./paneDrag`'s and is
// tested there; what is pinned here is that the face is wired to it — that a drag on the row writes
// an order and a drag on the corner writes a size, neither of which the arithmetic can say.
//
// jsdom has no layout, so the page and the panes answer for their own rectangles and the document
// for what is under a point. It is the same trade `./paneOrder.test` makes.
describe("moving and sizing a pane where it is drawn", () => {
  /** Press on this, move to that point, and let go — with the document answering for what is under
   *  it. `holding` is run with the hand still down, which is the only moment what a drag draws is on
   *  the screen. The wait before it is for the one hit test a frame the move asks for. */
  async function carry(
    on: HTMLElement,
    to: { x: number; y: number },
    over: HTMLElement | null = null,
    holding: () => void = () => {},
  ) {
    document.elementFromPoint = () => over;
    await act(async () => {
      on.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true, button: 0, clientX: 0, clientY: 0 }));
      document.dispatchEvent(new MouseEvent("pointermove", { clientX: to.x, clientY: to.y }));
      await new Promise((done) => setTimeout(done, 20));
    });
    holding();
    await act(async () => {
      document.dispatchEvent(new MouseEvent("pointerup", { clientX: to.x, clientY: to.y }));
    });
  }

  const panes = () => here(".slot:not(.slot--empty)");

  it("leaves a pane at the size its corner was let go over", async () => {
    await mount();
    await openPane();                              // one pane, at the whole page
    pageIs1200By400();

    // Pulled in to six cells across and one row down, which is a quarter of the page.
    await carry(here(".slot__corner")[0]!, { x: 600, y: 200 }, null, () => {
      // While the hand is down the pane is untouched and the size is an outline over the page: it
      // holds a terminal, and one resized on every report of the pointer would be told a new width
      // dozens of times for one gesture.
      expect(panes()[0]!.style.gridColumn).toBe("1 / span 12");
      const outline = here(".workspace__stretch")[0]!;
      expect(outline.style.gridColumn).toBe("1 / span 6");
      expect(outline.style.gridRow).toBe("1 / span 1");
    });
    expect(here(".workspace__stretch")).toHaveLength(0);
    expect(panes()[0]!.style.gridColumn).toBe("1 / span 6");
    expect(panes()[0]!.style.gridRow).toBe("1 / span 1");
  });

  it("leaves the size alone where the press never became a drag", async () => {
    await mount();
    await openPane();
    pageIs1200By400();

    // Down and up in the same place: a press on the corner is not a gesture that settled anywhere.
    await carry(here(".slot__corner")[0]!, { x: 0, y: 0 });
    expect(panes()[0]!.style.gridColumn).toBe("1 / span 12");
  });

  it("measures a corner against the grid of the page it is on, wherever that page is drawn", async () => {
    await mount();
    await openPane();
    await atSize("half");
    await openPane();                              // two halves, so page 1 is full
    await openPane();                              // the strip goes to page 2, and the pane opens there
    expect(q(".workspace__page--on")[0]!.textContent).toBe("2");
    // Page 2 as drawn to the right of page 1, so a corner measured against any box but its own page's
    // would come out a page too wide.
    const grid = here(".slot__corner")[0]!.closest<HTMLElement>(".workspace__page-grid")!;
    grid.getBoundingClientRect = () => ({
      top: 0, left: 1200, width: 1200, height: 400, right: 2400, bottom: 400,
      x: 1200, y: 0, toJSON: () => ({}),
    }) as DOMRect;

    // Six cells across and one row down from the page's own left edge, which is a quarter.
    await carry(here(".slot__corner")[0]!, { x: 1800, y: 200 }, null, () => {
      const outline = here(".workspace__stretch")[0]!;
      expect(outline.style.gridColumn).toBe("1 / span 6");
      expect(outline.style.gridRow).toBe("1 / span 1");
    });
    expect(panes()[0]!.style.gridColumn).toBe("1 / span 6");
    expect(panes()[0]!.style.gridRow).toBe("1 / span 1");
  });

  it("puts a pane carried onto another where the half it was let go over says", async () => {
    await mount();
    await openPane();
    await atSize("quarter");
    await openPane();                              // two quarters, side by side
    const [first, second] = panes();
    const was = panes().map((one) => one.dataset.hand);

    // The far half of the second pane, which is the right half at two panes drawn across.
    second!.getBoundingClientRect = () => ({
      top: 0, left: 600, width: 600, height: 200, right: 1200, bottom: 200,
      x: 600, y: 0, toJSON: () => ({}),
    }) as DOMRect;
    await carry(first!.querySelector<HTMLElement>(".slot__bar")!, { x: 900, y: 100 }, second!, () => {
      // Nothing has moved yet: what says where the drop would land is a mark on the pane it would go
      // in beside, on the far side of it and along the axis these two are neighbours on.
      expect(panes().map((one) => one.dataset.hand)).toEqual(was);
      const mark = second!.querySelector<HTMLElement>(".slot__goes")!;
      expect(mark.dataset.side).toBe("after");
      expect(mark.dataset.axis).toBe("across");
      expect(first!.className).toContain("slot--held");
    });

    expect(here(".slot__goes")).toHaveLength(0);
    expect(panes().map((one) => one.dataset.hand)).toEqual([was[1], was[0]]);
  });

  it("gives no handle to a project with one pane, which is already in order", async () => {
    await mount();
    await openPane();
    expect(here(".slot__bar--grab")).toHaveLength(0);

    await atSize("quarter");
    await openPane();
    expect(here(".slot__bar--grab")).toHaveLength(2);
  });
});

// A move within a page is made on the page itself now (`./paneDrag`, `AMB-D-939`), so the modal is
// left with the one move the page cannot show — a pane carried onto a page that is not on the
// screen. What it is offered from has to follow that.
describe("the way to carrying a pane onto another page", () => {
  const wayIn = () => q(".workspace__action").find((one) => one.getAttribute("title") === t("face.order"));

  it("is not drawn while every pane of the project is on one page", async () => {
    await mount();
    await openPane();
    await atSize("half");
    await openPane();                              // two panes, both on page 1
    expect(q(".workspace__page")).toHaveLength(0);
    expect(wayIn(), "a modal was offered with nowhere to carry a pane to").toBeUndefined();
  });

  it("is not drawn on the page the strip brought into being, which holds nothing yet", async () => {
    // That page is the asking, and the modal draws only the pages the panes fill — so a way in there
    // would open on one page and say nothing (`../talk/layout`).
    await mount();
    await openPane();
    await atSize("half");
    await openPane();                              // page 1 is full
    await click(here(".workspace__addstrip")[0]!);
    expect(q(".workspace__page")).toHaveLength(2);
    expect(wayIn()).toBeUndefined();
  });

  it("is drawn once there is a second page to reach, and opens on every page at once", async () => {
    await mount();
    await openPane();
    await atSize("half");
    await openPane();
    await openPane();                              // a third pane, which is page 2
    expect(q(".workspace__page")).toHaveLength(2);

    await click(wayIn()!);
    expect(q(".paneorder__page")).toHaveLength(2);
    // The cards of both pages are in it, which is the one thing the face cannot draw.
    expect(q(".paneorder__card")).toHaveLength(3);
  });
});
