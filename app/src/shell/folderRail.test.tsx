// @vitest-environment jsdom
// The column beside the panes holds the tree and the name of whose folders it is. The name is the
// whole of what this adds over drawing the tree on its own, and it is the thing a tree cannot say
// for itself: its sections are folder names, and a reader looking at them has no way to tell which
// project they were bound to (`AMB-D-838`).
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { Project } from "../mock/types";
import { FolderRail } from "./FolderRail";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

/** What the rail draws in place of a tree, which is the file side's to draw (`../files/FolderTree`). */
const TREE = "the tree";

const PROJECT = { id: 1, name: "amenbo" } as unknown as Project;

async function draw(project: Project | null) {
  await act(async () => {
    root.render(createElement(FolderRail, {
      project,
      folders: createElement("p", null, TREE),
    }));
  });
}

const title = () => container.querySelector(".rail__title")?.textContent ?? null;

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the folder rail", () => {
  it("names the project the tree is rooted in, and draws the tree", async () => {
    await draw(PROJECT);
    expect(title()).toBe("amenbo");
    expect(container.textContent).toContain(TREE);
  });

  // The row is kept so the tree does not walk up the column for the moment before the face has been
  // told which project it is on.
  it("keeps the row where there is no project to name yet", async () => {
    await draw(null);
    expect(container.querySelector(".rail__head")).not.toBeNull();
    expect(title()).toBe("");
  });

  // Nothing is opened from here and nothing is chosen here: the projects are the tabs at the edge of
  // the face (`./ProjectTabs`) and the panes are the middle of it.
  it("offers no control of its own", async () => {
    await draw(PROJECT);
    expect(container.querySelectorAll(".rail button")).toHaveLength(0);
  });
});
