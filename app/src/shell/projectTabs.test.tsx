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
import { t } from "../core/i18n";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
const project = vi.fn();
const folded = vi.fn();

const PROJECTS = [
  { id: 1, name: "amenbo", color: "#101820" },
  { id: 2, name: "the site", color: "#ffe066" },
] as unknown as Project[];

/** A pane in each project, the one in the second running — so a turn can be left standing in it. */
function twoProjects(): Layout {
  let layout: Layout = { ...EMPTY_LAYOUT, project: 1 };
  layout = openedFrame(layout, 1, "/repo").layout;
  layout = openedFrame(layout, 2, "/site").layout;
  layout = openedIn(layout, "2", "s2", "/site");
  return goProject(layout, 1);
}

async function draw(compact = false, needy: string[] = []) {
  await act(async () => {
    root.render(createElement(ProjectTabs, {
      layout: twoProjects(),
      projects: PROJECTS,
      needy: new Set(needy),
      compact,
      onCompact: folded,
      onProject: project,
    }));
  });
}

const tabs = () => [...container.querySelectorAll<HTMLElement>(".ptabs__tab")];
const marks = () => tabs().map((one) => one.querySelector(".ptabs__mark")!.textContent);
const names = () => tabs().map((one) => one.querySelector(".ptabs__name")?.textContent ?? null);

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
    expect(tabs().map((one) => one.getAttribute("aria-label"))).toEqual(["amenbo", "the site"]);
  });

  // The whole of what the column being uncloseable buys: the dot is on a project the reader is not in.
  it("wears a dot for a turn standing in a project that is not the one being shown", async () => {
    await draw(false, ["2"]);
    expect(tabs()[0].querySelector(".ptabs__needs")).toBeNull();
    expect(tabs()[1].querySelector(".ptabs__needs")).not.toBeNull();
  });

  // The panes of the project being shown say whose turn it is for themselves.
  it("wears none for a turn standing in the project being shown", async () => {
    await draw(false, ["1"]);
    expect(container.querySelector(".ptabs__needs")).toBeNull();
  });

  it("still wears it once the names are folded away", async () => {
    await draw(true, ["2"]);
    expect(tabs()[1].querySelector(".ptabs__needs")).not.toBeNull();
  });

  it("asks for the other width, and says which one it is offering", async () => {
    await draw();
    const fold = container.querySelector<HTMLElement>(".ptabs__fold")!;
    expect(fold.getAttribute("aria-label")).toBe(t("face.tabsCompact"));
    await act(async () => { fold.click(); });
    expect(folded).toHaveBeenCalledWith(true);

    await draw(true);
    const back = container.querySelector<HTMLElement>(".ptabs__fold")!;
    expect(back.getAttribute("aria-label")).toBe(t("face.tabsNamed"));
    await act(async () => { back.click(); });
    expect(folded).toHaveBeenLastCalledWith(false);
  });
});
