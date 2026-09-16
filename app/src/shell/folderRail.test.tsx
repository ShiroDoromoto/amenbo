// @vitest-environment jsdom
// The column beside the panes holds one of two lists and the name of whose folders they are. The
// name is what neither list can say for itself: a folder name says which folder, never which
// project it was bound to (`AMB-D-838`). The pair of tabs is the other half — the column has room
// for one list at a time, so the folder's own names and what git says about it take turns
// (`AMB-D-905`).
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { Project } from "../mock/types";
import { t } from "../core/i18n";
import type { RailTab } from "../talk/columns";
import { FolderRail } from "./FolderRail";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

/** What the rail draws in place of a tree, which is the file side's to draw (`../files/FolderTree`). */
const TREE = "the tree";

/** And in place of what git says, which is the file side's too (`../files/GitPanel`). */
const GIT = "what git says";

const PROJECT = { id: 1, name: "amenbo" } as unknown as Project;

/** What the face draws under the name where the project has more than one folder to choose from
 *  (`../files/RootPick`). */
const PICKER = "which folder";

/** Which half the rail was last asked for, so a press can be read back. */
let asked: RailTab | null = null;

async function draw(
  project: Project | null,
  { picker = false, tab = "files" as RailTab }: { picker?: boolean; tab?: RailTab } = {},
) {
  await act(async () => {
    root.render(createElement(FolderRail, {
      project,
      picker: picker ? createElement("button", null, PICKER) : undefined,
      tab,
      onTab: (which: RailTab) => { asked = which; },
      folders: createElement("p", null, TREE),
      git: createElement("p", null, GIT),
    }));
  });
}

/** One of the two tabs, by the words on it. */
const tabFor = (says: string) =>
  [...container.querySelectorAll<HTMLElement>(".rail__tab")].find((one) => one.textContent === says);

const title = () => container.querySelector(".rail__title")?.textContent ?? null;

beforeEach(() => {
  asked = null;
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

  // The projects are the tabs at the edge of the face (`./ProjectTabs`) and the panes are the middle
  // of it, so neither is chosen here. What the head does carry is handed in, for the reason the two
  // lists are — the choice has readers on both sides of the panes.
  it("stands the control it is handed under the name", async () => {
    await draw(PROJECT, { picker: true });
    // In the head with the name, not above the list: it says which folder the name's project is
    // being read in, which is the same sentence read on (`AMB-D-905`).
    expect(container.querySelector(".rail__head button")?.textContent).toBe(PICKER);
  });

  // One list at a time, because the column has room for one (`AMB-D-835`). The half that is down is
  // not drawn at all: each of them reads the disk while it is on the screen.
  it("draws the half that is up and not the other, and says which one that is", async () => {
    await draw(PROJECT, { tab: "files" });
    expect(container.textContent).toContain(TREE);
    expect(container.textContent).not.toContain(GIT);
    expect(tabFor(t("git.tabFiles"))?.getAttribute("aria-selected")).toBe("true");
    expect(tabFor(t("git.tabGit"))?.getAttribute("aria-selected")).toBe("false");

    await draw(PROJECT, { tab: "git" });
    expect(container.textContent).toContain(GIT);
    expect(container.textContent).not.toContain(TREE);
  });

  // The press says which half, and the face answers with it: the width the column is drawn at is
  // held there and is the half's own, so the two moves are one answer (`../talk/columns`).
  it("hands the press up rather than swapping the list itself", async () => {
    await draw(PROJECT, { tab: "files" });
    await act(async () => { tabFor(t("git.tabGit"))?.click(); });
    expect(asked).toBe("git");
    // Still drawing the folder's names: what is up is the face's answer, not this press.
    expect(container.textContent).toContain(TREE);
  });
});
