// @vitest-environment jsdom
// The band under a pane saying how much this session has filed (`AMB-D-897`).
//
// What is held here is the count and the way into each record. The count is a statement about the
// session a person is watching, so a number that double-counted one `task add` would be exactly the
// kind of thing `AMB-D-862` took off this row; and the press has to land on the record the note
// named, because a list of refs that opened the wrong one is worse than no list.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { PaneMade, type Made } from "./PaneMade";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const hoisted = vi.hoisted(() => ({
  /** Which records the band asked the host to open, in the order it asked. */
  opened: [] as { space: string; num: number }[],
}));

// The one road out of this component: a record is raised in the window the board is read in, which
// belongs to the host and not to this webview (`../talk/terminal`).
vi.mock("../talk/terminal", () => ({
  showRef: (space: string, num: number) => { hoisted.opened.push({ space, num }); },
}));

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  hoisted.opened = [];
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

/** Draw the band for a session that has filed `made`. */
async function draw(made: Made[]): Promise<void> {
  await act(async () => { root.render(createElement(PaneMade, { made })); });
}

/** The count the band is showing, or nothing where the band is not drawn at all. */
function count(): string | null {
  return container.querySelector(".maderow__count")?.textContent ?? null;
}

/** Open the list, which is the press on the count itself. */
async function open(): Promise<void> {
  await act(async () => { container.querySelector<HTMLButtonElement>(".maderow__count")?.click(); });
}

function items(): HTMLElement[] {
  return [...container.querySelectorAll<HTMLElement>(".menu__item")];
}

describe("the band says how much this session has filed", () => {
  it("draws nothing at all for a session that has filed nothing", async () => {
    await draw([]);
    expect(count()).toBe(null);
    expect(container.querySelector(".maderow")).toBe(null);
  });

  it("names each side, and names only the sides there are", async () => {
    await draw([
      { space: "task", num: 4849 },
      { space: "decision", num: 897 },
      { space: "task", num: 4850 },
    ]);
    expect(count()).toContain("2 tasks");
    expect(count()).toContain("1 decision");

    // A session that has filed no decisions says nothing about decisions: "0 decisions" is a fact
    // about nothing, and the band is one line under a terminal.
    await draw([{ space: "task", num: 4849 }]);
    expect(count()).toContain("1 task");
    expect(count()).not.toContain("decision");
  });
});

describe("the list behind the count opens the records themselves", () => {
  it("draws every record as its own ref, newest first", async () => {
    await draw([
      { space: "task", num: 4849 },
      { space: "decision", num: 897 },
    ]);
    await open();
    expect(items().map((one) => one.textContent)).toEqual(["AMB-D-897", "AMB-T-4849"]);
  });

  it("opens the record that was pressed, in the space the note named", async () => {
    await draw([
      { space: "task", num: 4849 },
      { space: "decision", num: 897 },
    ]);
    await open();
    await act(async () => { items()[1]?.click(); });
    expect(hoisted.opened).toEqual([{ space: "task", num: 4849 }]);
    expect(items(), "the list is done with once one of them has been pressed").toHaveLength(0);
  });
});
