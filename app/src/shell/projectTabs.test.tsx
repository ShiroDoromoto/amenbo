// @vitest-environment jsdom
// The tabs are the only way to another project once the rail's list of them is gone, and the only
// place a turn standing in a project nobody is looking at is knocked about. Both survive folding the
// names away, which is the one thing about this column a person can change.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { EMPTY_LAYOUT, goProject, openedFrame, openedIn, type Layout } from "../talk/layout";
import type { Project } from "../mock/types";
import { ProjectTabs } from "./ProjectTabs";
import { t, tf, tn } from "../core/i18n";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
const project = vi.fn();
const folded = vi.fn();

const PROJECTS = [
  { id: 1, name: "amenbo", color: "#101820", icon: null },
  { id: 2, name: "the site", color: "#ffe066", icon: null },
] as unknown as Project[];

/** The same two projects, with an image registered for the second (`AMB-D-839`). */
const MARKED = [
  PROJECTS[0],
  { ...PROJECTS[1], icon: "data:image/png;base64,LOGO" },
] as unknown as Project[];

/** A pane in each project, the one in the second running — so a turn can be left standing in it. */
function twoProjects(): Layout {
  let layout: Layout = { ...EMPTY_LAYOUT, project: 1 };
  layout = openedFrame(layout, 1, "/repo").layout;
  layout = openedFrame(layout, 2, "/site").layout;
  layout = openedIn(layout, "2", "s2", "/site", null);
  return goProject(layout, 1);
}

async function draw(compact = false, projects: Project[] = PROJECTS) {
  await act(async () => {
    root.render(createElement(ProjectTabs, {
      layout: twoProjects(),
      projects,
      compact,
      onCompact: folded,
      onProject: project,
    }));
  });
}

const tabs = () => [...container.querySelectorAll<HTMLElement>(".ptabs__tab")];
const marks = () => tabs().map((one) => one.querySelector(".ptabs__mark")!.textContent);
const names = () => tabs().map((one) => one.querySelector(".ptabs__name")?.textContent ?? null);
const counts = () => tabs().map((one) => one.querySelector(".ptabs__count")?.textContent ?? null);
/** What a tab with panes open is read out as, written from the dictionary rather than in English. */
const spoken = (name: string, panes: number) =>
  tf("face.tabPanes", { name, panes: tn("face.panes", panes) });

beforeEach(() => {
  project.mockReset();
  folded.mockReset();
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the project tabs", () => {
  it("draws one for every project, and marks the one being shown", async () => {
    await draw();
    expect(names()).toEqual(["amenbo", "the site"]);
    expect(tabs()[0].getAttribute("aria-current")).toBe("page");
    expect(tabs()[1].getAttribute("aria-current")).toBeNull();
  });

  it("goes to the project pressed", async () => {
    await draw();
    await act(async () => { tabs()[1].click(); });
    expect(project).toHaveBeenCalledWith(2);
  });

  // Folding takes the width the names take, and nothing else: which project a tab is stays readable
  // by its colour and its first character, and by the name every one of them is still called.
  it("keeps the mark and the name when the names are folded away", async () => {
    await draw(true);
    expect(names()).toEqual([null, null]);
    expect(marks()).toEqual(["a", "t"]);
    expect(tabs().map((one) => one.getAttribute("aria-label")))
      .toEqual([spoken("amenbo", 1), spoken("the site", 1)]);
  });

  // What a project shows for itself, where somebody gave it one (`AMB-D-838`). It stands in the mark's
  // place rather than beside it — that place is the whole of what a compact tab says it is.
  it("draws the image a project was given in place of its colour and its letter", async () => {
    await draw(false, MARKED);
    expect(marks()).toEqual(["a", ""]);
    const image = tabs()[1].querySelector<HTMLImageElement>(".ptabs__icon");
    expect(image?.getAttribute("src")).toBe("data:image/png;base64,LOGO");
    // The colour would only show through the corners of a picture that fills the mark.
    expect(tabs()[1].querySelector<HTMLElement>(".ptabs__mark")!.style.background).toBe("");
  });

  // Registering one is a thing a person does, and most never will.
  it("keeps the colour and the letter for a project with no image", async () => {
    await draw(false, MARKED);
    expect(tabs()[0].querySelector(".ptabs__icon")).toBeNull();
    expect(tabs()[0].querySelector<HTMLElement>(".ptabs__mark")!.style.background).not.toBe("");
  });

  // Folding takes the names, and the mark is what is left — an image the same as a letter.
  it("still draws it once the names are folded away", async () => {
    await draw(true, MARKED);
    expect(tabs()[1].querySelector(".ptabs__icon")).not.toBeNull();
    expect(tabs()[1].getAttribute("aria-label")).toBe(spoken("the site", 1));
  });

  it("asks for the other width, and says which one it is offering", async () => {
    await draw();
    const fold = container.querySelector<HTMLElement>(".ptabs__fold")!;
    expect(fold.getAttribute("aria-label")).toBe(t("face.tabsCompact"));
    // The arrow points the way the column is about to go, which is the whole of what the mark says.
    expect(fold.querySelector("svg")!.getAttribute("data-icon")).toBe("foldLeft");
    await act(async () => { fold.click(); });
    expect(folded).toHaveBeenCalledWith(true);

    await draw(true);
    const back = container.querySelector<HTMLElement>(".ptabs__fold")!;
    expect(back.getAttribute("aria-label")).toBe(t("face.tabsNamed"));
    expect(back.querySelector("svg")!.getAttribute("data-icon")).toBe("foldRight");
    await act(async () => { back.click(); });
    expect(folded).toHaveBeenLastCalledWith(false);
  });

  // How many panes the project has open, so a reader can tell where the work is without going into
  // each project to look. It is the frames, whatever is running in them.
  it("says how many panes a project has open", async () => {
    await draw();
    expect(counts()).toEqual(["1", "1"]);
    expect(tabs()[0].getAttribute("aria-label")).toBe(spoken("amenbo", 1));
  });

  // A project nobody has opened a terminal in would otherwise carry a nought to read for nothing,
  // and its name would be read out with a count that counts nothing.
  it("leaves the count off a project with nothing open", async () => {
    const archive = { id: 3, name: "the archive", color: "#888888", icon: null };
    const withEmpty = [...PROJECTS, archive] as unknown as Project[];
    await draw(false, withEmpty);
    expect(counts()).toEqual(["1", "1", null]);
    expect(tabs()[2].getAttribute("aria-label")).toBe("the archive");
  });

  // Folding takes the names, not the count: the number is why a folded column can still be read.
  it("keeps the count once the names are folded away", async () => {
    await draw(true);
    expect(counts()).toEqual(["1", "1"]);
  });
});
