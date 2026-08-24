// @vitest-environment jsdom
// The rail is where the project is chosen, the only way to a pane that is not on the screen, and the
// only place a frame with nothing running in it can still be named. All three are pinned here,
// because each is a row that looks the same whether or not it is wired to anything.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { EMPTY_LAYOUT, focusOn, goProject, openedFrame, openedIn, type Layout } from "../talk/layout";
import type { Project } from "../mock/types";
import { PaneRail } from "./PaneRail";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
const project = vi.fn();
const picked = vi.fn();
const renamed = vi.fn();
const opened = vi.fn();

const PROJECTS = [
  { id: 1, name: "amenbo" },
  { id: 2, name: "the site" },
] as unknown as Project[];

/** Four panes in project 1 at two a page, the first of them running, and one in project 2. */
function twoProjects(): Layout {
  let layout: Layout = { ...EMPTY_LAYOUT, project: 1 };
  for (let i = 0; i < 4; i++) layout = openedFrame(layout, 1, "/repo").layout;
  layout = openedFrame(layout, 2, "/site").layout;
  layout = openedIn(layout, "1", "s1", "/repo");
  return focusOn(goProject(layout, 1), "1");
}

async function draw(layout: Layout, names: Map<string, string> = new Map()) {
  await act(async () => {
    root.render(createElement(PaneRail, {
      layout,
      names,
      projects: PROJECTS,
      needy: new Set<string>(),
      onProject: project,
      onPick: picked,
      onRename: renamed,
      onOpen: opened,
    }));
  });
}

const rows = () => [...container.querySelectorAll<HTMLElement>(".rail__row")];
const names = () => rows().map((one) => one.querySelector(".rail__name")!.textContent);

beforeEach(() => {
  project.mockReset();
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
  it("names every project and lists the panes of the one being shown", async () => {
    await draw(twoProjects());
    expect([...container.querySelectorAll(".rail__project")].map((one) => one.textContent))
      .toEqual(["amenbo", "the site"]);
    // Project 2 has a pane of its own; it is not on the rail, because it is not on the screen.
    expect(rows()).toHaveLength(4);
  });

  it("changes the whole screen when a project is picked", async () => {
    await draw(twoProjects());
    await act(async () => {
      container.querySelectorAll(".rail__project")[1]!
        .dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });
    expect(project).toHaveBeenCalledWith(2);
  });

  it("calls a pane after its folder until someone names it, and puts them in name order", async () => {
    // All four are in one folder, so the folder alone would give four identical rows: what tells them
    // apart is the place, which is the thing a rail is for.
    await draw(twoProjects());
    expect(names()).toEqual(["repo 1.1", "repo 1.2", "repo 2.1", "repo 2.2"]);

    // A named pane sorts by its name, not by when it was opened.
    await draw(twoProjects(), new Map([["3", "a migration"]]));
    expect(names()).toEqual(["a migration", "repo 1.1", "repo 1.2", "repo 2.2"]);
  });

  it("calls a pane by its folder alone where no other pane of the project is in it", async () => {
    let layout: Layout = { ...EMPTY_LAYOUT, project: 1 };
    layout = openedFrame(layout, 1, "/work/amenbo").layout;
    layout = openedFrame(layout, 1, "/work/the-site").layout;
    // A pane that took up a terminal somebody else started has no folder of its own yet, and is
    // called where it is until it learns one (`../talk/layout`).
    layout = openedFrame(layout, 1, null).layout;
    await draw(goProject(layout, 1));
    expect(names()).toEqual(["2.1", "amenbo", "the-site"]);
  });

  it("marks the panes nothing is running in, without spelling it out over the name", async () => {
    await draw(twoProjects());
    expect(rows()[0]!.querySelector(".rail__idle")).toBeNull();
    expect(rows()[1]!.querySelector(".rail__idle")).not.toBeNull();
  });

  it("offers one way in, under the heading of what it adds to", async () => {
    await draw(twoProjects());
    // One button and not one per project: what it opens a pane in is the project on the screen, and
    // a second one beside a project that is not would move the screen as a side effect.
    expect(container.querySelectorAll(".rail__open")).toHaveLength(1);
    const heads = [...container.querySelectorAll(".rail__head")];
    expect(heads[1]!.querySelector(".rail__open"), "it is not under the projects").not.toBeNull();
    expect(heads[0]!.querySelector(".rail__open")).toBeNull();
  });

  it("opens a pane in the project being shown, and says which", async () => {
    await draw(twoProjects());
    await act(async () => {
      container.querySelector(".rail__open")!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });
    expect(opened).toHaveBeenCalledWith(1);
  });

  it("goes to a pane on a page that is not the one showing", async () => {
    await draw(twoProjects());
    await act(async () => {
      rows()[2]!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });
    expect(picked).toHaveBeenCalledWith("3");
  });

  it("says a turn is standing in a project nobody is looking at", async () => {
    await act(async () => {
      root.render(createElement(PaneRail, {
        layout: twoProjects(),
        names: new Map<string, string>(),
        projects: PROJECTS,
        // The pane of project 2, which is not the project on the screen.
        needy: new Set(["5"]),
        onProject: project,
        onPick: picked,
        onRename: renamed,
        onOpen: opened,
      }));
    });
    const dots = [...container.querySelectorAll(".rail__project")]
      .map((one) => one.querySelector(".rail__needs") !== null);
    expect(dots).toEqual([false, true]);
  });

  it("names a frame with nothing running in it — the one place that can be done", async () => {
    await draw(twoProjects());
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
    await draw(twoProjects());
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
