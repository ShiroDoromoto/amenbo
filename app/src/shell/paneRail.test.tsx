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
import { t } from "../core/i18n";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
const project = vi.fn();
const picked = vi.fn();
const renamed = vi.fn();
const took = vi.fn();

/** What the rail draws in place of its lists while the other half is up (`../files/FolderTree`). */
const TREE = "the tree";

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

async function draw(
  layout: Layout,
  names: Map<string, string> = new Map(),
  tab: "panes" | "folders" = "panes",
) {
  await act(async () => {
    root.render(createElement(PaneRail, {
      layout,
      names,
      projects: PROJECTS,
      needy: new Set<string>(),
      tab,
      onTab: took,
      folders: createElement("p", null, TREE),
      onProject: project,
      onPick: picked,
      onRename: renamed,
    }));
  });
}

const rows = () => [...container.querySelectorAll<HTMLElement>(".rail__row")];
const names = () => rows().map((one) => one.querySelector(".rail__name")!.textContent);

beforeEach(() => {
  project.mockReset();
  picked.mockReset();
  renamed.mockReset();
  took.mockReset();
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

  it("offers no way in of its own — the panes' side has a nearer one either way", async () => {
    await draw(twoProjects());
    // A page with room draws an empty frame, and a full one the strip beside the panes: both are on
    // the face the reader is looking at, so a press here was never the only road (`./TerminalFace`).
    // The two halves are not a way in either: what they choose is which of the rail's own lists is
    // drawn, and neither of them opens anything (`AMB-D-835`).
    const ways = [...container.querySelectorAll<HTMLElement>(".rail button")]
      .filter((one) => !one.classList.contains("rail__project")
        && !one.classList.contains("rail__row")
        && !one.classList.contains("rail__tab"));
    expect(ways.map((one) => one.className)).toEqual([]);
  });

  // The rail holds two lists and a tree, and a column this narrow has room for one of those at a
  // time (`AMB-D-835`). So the halves are swapped, and both are named on the control that swaps
  // them: a reader has to be able to see where the other one went.
  it("names both halves and says which is up, without drawing the one that is not", async () => {
    await draw(twoProjects());
    expect(container.textContent).not.toContain(TREE);
    const tabs = [...container.querySelectorAll<HTMLElement>(".rail__tab")];
    expect(tabs.map((one) => one.textContent))
      .toEqual([t("face.railPanes"), t("face.railFolders")]);
    expect(tabs.map((one) => one.getAttribute("aria-checked"))).toEqual(["true", "false"]);
  });

  it("asks for the other half rather than swapping itself", async () => {
    await draw(twoProjects());
    await act(async () => {
      container.querySelectorAll<HTMLElement>(".rail__tab")[1]!
        .dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });
    // Which half is up is kept per project, and what keeps it is the face around the rail
    // (`../talk/columns`).
    expect(took).toHaveBeenCalledWith("folders");
  });

  it("draws the tree in place of the lists on the half that is up", async () => {
    await draw(twoProjects(), new Map(), "folders");
    expect(container.textContent).toContain(TREE);
    expect(rows()).toHaveLength(0);
    expect(container.querySelectorAll(".rail__project")).toHaveLength(0);
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
        tab: "panes" as const,
        onTab: took,
        folders: createElement("p", null, TREE),
        onProject: project,
        onPick: picked,
        onRename: renamed,
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
