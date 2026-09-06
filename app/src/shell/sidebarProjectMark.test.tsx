// @vitest-environment jsdom
// What a project row in the rail is marked with (`AMB-D-848`): the image the project was given, or
// the colour a person gave it with the first character of its name written on it. The rail drew an
// 8px square of colour before this and no letter at all, so the same project arrived one way here
// and another on the terminal face's tabs — and an image a person had registered was on neither.
//
// The tabs' side of the same rule is `projectTabs.test`. What is asked here is what the rail alone
// answers: the archived rows, whose read path carries no image.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const hoisted = vi.hoisted(() => ({
  archived: [] as { id: number; name: string; color: string }[],
}));

vi.mock("../store/store", () => ({ useStore: () => ({ moveProject: () => {} }) }));

vi.mock("../mock/adapter", () => ({
  dataAdapter: {
    smartViews: () => [],
    listProjects: () => [
      // A dark ground, a pale one, and one with an image — the three answers the mark has.
      { id: 1, name: "amenbo", color: "#101820", icon: null, openCount: 0, proposedDecisionCount: 0 },
      { id: 2, name: "the site", color: "#ffe066", icon: null, openCount: 0, proposedDecisionCount: 0 },
      { id: 3, name: "orchard", color: "#0a0", icon: "data:image/png;base64,LOGO", openCount: 0, proposedDecisionCount: 0 },
    ],
  },
}));

vi.mock("../core/mailbox", () => ({ useInboxCount: () => 0 }));
vi.mock("../core/reads", () => ({
  useArchivedProjects: () => hoisted.archived,
  useDueCounts: () => ({ overdue: 0, today: 0, tomorrow: 0 }),
}));

import { Sidebar } from "./Sidebar";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

/** The mark on the row for a project id — every row that has one carries its project's id. */
const markFor = (id: number) =>
  container.querySelector<HTMLElement>(`[data-project-id="${id}"] .navitem__mark`)!;

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  act(() => {
    root.render(createElement(Sidebar, { nav: { type: "view", id: "inbox" }, onNav: () => {} }));
  });
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the mark on a project row", () => {
  it("writes the name's first character on the colour the project was given", () => {
    expect(markFor(1).textContent).toBe("a");
    expect(markFor(1).style.background).not.toBe("");
  });

  // The colour is picked with a colour well and is any colour at all, so the letter cannot be one
  // fixed shade — it is worked out from the ground it lands on (`./projectMark`).
  it("writes it in ink the ground can be read against", () => {
    expect(markFor(1).style.color).toBe("rgb(255, 255, 255)");
    expect(markFor(2).style.color).toBe("rgb(17, 17, 17)");
  });

  // It stands in the mark's place rather than beside it: the mark is one thing wide.
  it("draws the image a project was given in place of its colour and its letter", () => {
    const mark = markFor(3);
    expect(mark.querySelector<HTMLImageElement>(".navitem__icon")!.getAttribute("src"))
      .toBe("data:image/png;base64,LOGO");
    expect(mark.textContent).toBe("");
    // The colour would only show through the corners of a picture that fills the mark.
    expect(mark.style.background).toBe("");
  });

  // The archived list is fetched over its own read path (`ArchivedProjectDto`), which carries the
  // colour and the name and no image. Those rows get the mark the rest of them get, minus the one
  // thing they cannot be given.
  it("marks an archived project with its colour and its letter", () => {
    act(() => {
      hoisted.archived = [{ id: 9, name: "greenhouse", color: "#101820" }];
      root.render(createElement(Sidebar, { nav: { type: "view", id: "inbox" }, onNav: () => {} }));
    });
    // The archived group opens closed — the rows are only drawn once it is unfolded.
    act(() => { container.querySelector<HTMLElement>("[aria-expanded]")!.click(); });
    const marks = [...container.querySelectorAll<HTMLElement>(".navitem__mark")];
    const mark = marks[marks.length - 1];
    expect(mark.textContent).toBe("g");
    expect(mark.querySelector(".navitem__icon")).toBeNull();
  });
});
