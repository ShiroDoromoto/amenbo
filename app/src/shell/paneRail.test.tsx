// @vitest-environment jsdom
// The rail is the only way to a pane that is not on the screen, and the only place a frame with
// nothing running in it can still be named. Both are pinned here, because both are a row that looks
// the same whether or not it is wired to anything.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { EMPTY_LAYOUT, focusOn, frameFor, openedIn, type Layout } from "../talk/layout";
import { PaneRail } from "./PaneRail";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
const picked = vi.fn();
const renamed = vi.fn();
const opened = vi.fn();

/** Four frames at two a page: two pages, the first of them with terminals in it. */
function twoPages(): Layout {
  let layout: Layout = EMPTY_LAYOUT;
  for (const [page, slot] of [[1, 0], [1, 1], [2, 0], [2, 1]] as const) {
    layout = frameFor(layout, page, slot).layout;
  }
  layout = openedIn(layout, "1", "s1", "/repo");
  return focusOn(layout, "1");
}

async function draw(layout: Layout, names: Map<string, string> = new Map()) {
  await act(async () => {
    root.render(createElement(PaneRail, {
      layout, names, onPick: picked, onRename: renamed, onOpen: opened,
    }));
  });
}

const rows = () => [...container.querySelectorAll<HTMLElement>(".rail__row")];

beforeEach(() => {
  picked.mockReset();
  renamed.mockReset();
  opened.mockReset();
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the rail", () => {
  it("groups the rows by the page they are on, because picking one moves the screen there", async () => {
    await draw(twoPages());
    // Two pages of frames, and the one there is room to grow into.
    expect(container.querySelectorAll(".rail__pagename")).toHaveLength(3);
    expect(rows()).toHaveLength(4);
  });

  it("calls a frame by its place until someone names it", async () => {
    await draw(twoPages());
    expect(rows().map((one) => one.querySelector(".rail__name")!.textContent))
      .toEqual(["1.1", "1.2", "2.1", "2.2"]);

    await draw(twoPages(), new Map([["3", "the migration"]]));
    expect(rows()[2]!.querySelector(".rail__name")!.textContent).toBe("the migration");
  });

  it("marks the frames nothing is running in, without spelling it out over the name", async () => {
    await draw(twoPages());
    expect(rows()[0]!.querySelector(".rail__idle")).toBeNull();
    expect(rows()[1]!.querySelector(".rail__idle")).not.toBeNull();
  });

  it("offers the way in only on a page with somewhere to put one", async () => {
    await draw(twoPages());
    // Two full pages and the one there is room to grow into: the way in is on that one alone.
    const ways = [...container.querySelectorAll(".rail__page")]
      .map((page) => page.querySelector(".rail__open") !== null);
    expect(ways).toEqual([false, false, true]);
  });

  it("opens a pane on the page it was pressed on, and says which page that was", async () => {
    await draw(twoPages());
    await act(async () => {
      container.querySelector(".rail__open")!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });
    expect(opened).toHaveBeenCalledWith(3);
  });

  it("goes to a pane on a page that is not the one showing", async () => {
    await draw(twoPages());
    await act(async () => {
      rows()[2]!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });
    expect(picked).toHaveBeenCalledWith("3");
  });

  it("names a frame with nothing running in it — the one place that can be done", async () => {
    await draw(twoPages());
    await act(async () => {
      rows()[3]!.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
    });
    const field = container.querySelector<HTMLInputElement>(".rail__rename")!;
    field.value = "  the notes  ";
    await act(async () => {
      field.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    });
    expect(renamed).toHaveBeenCalledWith("4", "the notes");
    expect(container.querySelector(".rail__rename"), "the field stayed open").toBeNull();
  });

  it("leaves the name alone when the rename is dropped", async () => {
    await draw(twoPages());
    await act(async () => {
      rows()[0]!.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
    });
    const field = container.querySelector<HTMLInputElement>(".rail__rename")!;
    field.value = "no";
    await act(async () => {
      field.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    });
    expect(renamed).not.toHaveBeenCalled();
  });
});
