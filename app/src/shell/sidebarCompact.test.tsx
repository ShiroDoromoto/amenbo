// @vitest-environment jsdom
// The rail's two widths (`AMB-D-848`): named, and compact at 46px. There is no third state that
// takes the column away, which is what lets the way between them live in the column itself — a
// control that could put the rail out of reach would have to be reachable from somewhere else, and
// somewhere else is the bar over both faces, where it cannot say which of the two it moves.
//
// What is asked here is what folding may and may not take away: the names and the headings go, and
// everything that tells one row from another stays. The terminal face's side of the same rule is
// `projectTabs.test`.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const hoisted = vi.hoisted(() => ({ due: { stop: 0, heed: 0 } }));

vi.mock("../store/store", () => ({ useStore: () => ({ moveProject: () => {} }) }));

vi.mock("../mock/adapter", () => ({
  dataAdapter: {
    smartViews: () => [{ id: "inbox" }, { id: "activity" }, { id: "due" }],
    listProjects: () => [
      { id: 1, name: "amenbo", color: "#101820", icon: null, openCount: 3, proposedDecisionCount: 1 },
    ],
  },
}));

vi.mock("../core/mailbox", () => ({ useInboxCount: () => 0 }));
vi.mock("../core/reads", () => ({
  useArchivedProjects: () => [],
  useDueCounts: () => hoisted.due,
}));

import { t } from "../core/i18n";
import { Sidebar } from "./Sidebar";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
const folded = vi.fn();

function draw(compact: boolean) {
  act(() => {
    root.render(createElement(Sidebar, {
      nav: { type: "view", id: "inbox" },
      onNav: () => {},
      compact,
      onCompact: folded,
    }));
  });
}

const fold = () => container.querySelector<HTMLElement>(".sidebar__fold")!;
const projectRow = () => container.querySelector<HTMLElement>('[data-project-id="1"]')!;

beforeEach(() => {
  hoisted.due = { stop: 0, heed: 0 };
  folded.mockClear();
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the rail folded to its marks", () => {
  it("takes the names and the headings away and leaves the marks", () => {
    draw(false);
    expect(container.querySelectorAll(".sidebar__label").length).toBeGreaterThan(0);
    expect(container.querySelectorAll(".navitem__name").length).toBeGreaterThan(0);

    draw(true);
    expect(container.querySelectorAll(".sidebar__label")).toHaveLength(0);
    expect(container.querySelectorAll(".navitem__name")).toHaveLength(0);
    // The mark is what tells one project from another once its name is gone, so folding must not
    // reach it.
    expect(projectRow().querySelector(".navitem__mark")).not.toBeNull();
  });

  it("says every row's name whether or not it is drawn", () => {
    for (const compact of [false, true]) {
      draw(compact);
      expect(projectRow().getAttribute("aria-label")).toBe("amenbo");
      expect(projectRow().getAttribute("title")).toBe("amenbo");
    }
  });

  // A count is what a row is worth looking at for, and it survives the fold — moved onto the row's
  // corner, which is the only place left on it.
  it("keeps a project's count", () => {
    draw(true);
    expect(projectRow().querySelector(".navitem__count")!.textContent).toBe("4");
  });

  // The due row is the one that warns on two steps at once. Compact there is no room beside the mark
  // for the second badge, so the two are added up and drawn in the more urgent of the two colours.
  it("adds the two due badges into one, on the more urgent step", () => {
    hoisted.due = { stop: 2, heed: 5 };
    draw(false);
    const both = container.querySelectorAll<HTMLElement>(".navitem__counts .navitem__count");
    expect([...both].map((b) => b.textContent)).toEqual(["2", "5"]);

    draw(true);
    const one = container.querySelectorAll<HTMLElement>(".navitem__count--stop, .navitem__count--heed");
    expect(one).toHaveLength(1);
    expect(one[0].textContent).toBe("7");
    expect(one[0].className).toContain("navitem__count--stop");
    // Which step each part of the sum stands on is still said, since the colour alone cannot say it.
    expect(one[0].title).toContain(t("smartview.dueStop"));
    expect(one[0].title).toContain(t("smartview.dueHeed"));
  });

  // Only the step that has something on it: a sum drawn in a colour nothing stands on would be
  // pointing the reader at an empty list.
  it("draws the sum on the step that is there when only one is", () => {
    hoisted.due = { stop: 0, heed: 4 };
    draw(true);
    const one = container.querySelector<HTMLElement>(".navitem__count--stop, .navitem__count--heed")!;
    expect(one.textContent).toBe("4");
    expect(one.className).toContain("navitem__count--heed");
  });

  // The control is in the column at either width — that is the whole of why it moved out of the bar
  // over the window — and it says what pressing it does rather than which state the column is in.
  it("carries the way back at both widths, worded for the press", () => {
    draw(false);
    expect(fold().getAttribute("aria-label")).toBe(t("face.tabsCompact"));
    act(() => fold().click());
    expect(folded).toHaveBeenLastCalledWith(true);

    draw(true);
    expect(fold().getAttribute("aria-label")).toBe(t("face.tabsNamed"));
    act(() => fold().click());
    expect(folded).toHaveBeenLastCalledWith(false);
  });

  // What scrolls is the list and not the column: a machine with a project for every folder it has
  // ever opened must not push the way back off the bottom of the screen.
  it("keeps the way back outside what scrolls", () => {
    draw(false);
    expect(container.querySelector(".sidebar__list .sidebar__fold")).toBeNull();
    expect(fold().parentElement!.classList.contains("sidebar")).toBe(true);
  });
});
