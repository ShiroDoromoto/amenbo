// @vitest-environment jsdom
// Both ways in are moves the GUI makes, so what is tested is where a click lands — not what it put
// on the clipboard for a terminal. The open card is the one with a walk of its own: pick the project,
// pick the folder, and end on that project's board. Only the boundaries are stubbed (the folder
// picker and the bind call, neither of which exists outside Tauri); the screen itself runs for real.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Project } from "../mock/types";
import type { Nav } from "../shell/AppShell";

const hoisted = vi.hoisted(() => ({
  /** Is this the desktop app? Off is the browser, where there is no folder to pick. */
  tauri: true,
  projects: [] as { id: number; name: string }[],
  /** What the folder picker answers; null is the reader dismissing it. */
  picked: "/w/one" as string | null,
  bound: [] as Array<[number, string]>,
  /** The CLI name this build installs — what the asking step's request has to name. */
  cli: "amenbo" as string,
}));

vi.mock("../core/mutations", () => ({
  pickFolder: () => Promise.resolve(hoisted.picked),
  bindFolder: (projectId: number, dir: string) => {
    hoisted.bound.push([projectId, dir]);
    return Promise.resolve();
  },
  fetchCliCommandName: () => Promise.resolve(hoisted.cli),
}));

// Only the desktop/browser answer is varied; the rest of the module stays itself, since the
// dictionary reads the current language off the same snapshot.
vi.mock("../core/snapshot", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../core/snapshot")>()),
  inTauri: () => hoisted.tauri,
}));

vi.mock("../mock/adapter", () => ({
  dataAdapter: { listProjects: () => hoisted.projects as unknown as Project[] },
}));

import { OnboardingScreen } from "./OnboardingScreen";
import { t, tf } from "../core/i18n";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
let navs: Nav[];

const buttons = () => Array.from(container.querySelectorAll("button"));
const button = (label: string) => buttons().find((b) => (b.textContent ?? "").includes(label));

async function render() {
  await act(async () => {
    root.render(createElement(OnboardingScreen, { onNav: (n: Nav) => navs.push(n) }));
  });
}

async function click(el: Element | undefined) {
  expect(el, "the button under test is on screen").toBeTruthy();
  await act(async () => {
    (el as HTMLElement).click();
  });
}

beforeEach(() => {
  hoisted.tauri = true;
  hoisted.projects = [{ id: 7, name: "one" }, { id: 9, name: "two" }];
  hoisted.picked = "/w/one";
  hoisted.cli = "amenbo";
  hoisted.bound = [];
  navs = [];
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the two ways in", () => {
  it("creating navigates to the create screen", async () => {
    await render();
    await click(button(t("onboard.createLabel")));
    expect(navs).toEqual([{ type: "view", id: "newProject" }]);
  });

  it("opening links the chosen project's folder and lands on its board", async () => {
    await render();
    await click(button(t("onboard.openLabel")));
    const select = container.querySelector("select") as HTMLSelectElement;
    await act(async () => {
      select.value = "9";
      select.dispatchEvent(new Event("change", { bubbles: true }));
    });
    await click(button(t("newproj.chooseFolder")));
    expect(hoisted.bound).toEqual([[9, "/w/one"]]);
    expect(navs).toEqual([{ type: "project", id: "9" }]);
  });

  // Dismissing the picker is an answer, not a failure: nothing is bound and the reader stays put.
  it("binds nothing when the folder picker is dismissed", async () => {
    hoisted.picked = null;
    await render();
    await click(button(t("onboard.openLabel")));
    await click(button(t("newproj.chooseFolder")));
    expect(hoisted.bound).toEqual([]);
    expect(navs).toEqual([]);
  });
});

describe("where linking is not a move that can be made", () => {
  // A card that cannot do what it says only asks to be tried. With nothing to open, creating is the
  // only real way in; in the browser there is no picker to hand a folder over with.
  it("offers no open card with no project to open", async () => {
    hoisted.projects = [];
    await render();
    expect(button(t("onboard.openLabel"))).toBeUndefined();
    expect(button(t("onboard.createLabel"))).toBeTruthy();
  });

  it("offers no open card in the browser", async () => {
    hoisted.tauri = false;
    await render();
    expect(button(t("onboard.openLabel"))).toBeUndefined();
  });
});

describe("what the asking step hands over", () => {
  // Two wordings for the same move leave the reader checking which one is right, so the step shows
  // the request the first loop copies rather than an example of its own — down to the command name,
  // which the request tells the reader's AI to run and so has to be the one this build installs.
  it("is the request the first loop copies, word for word", async () => {
    hoisted.cli = "amenbo-dev";
    await render();
    const shown = Array.from(container.querySelectorAll("code")).map((c) => c.textContent ?? "");
    expect(shown).toContain(tf("firstloop.prompt", { cmd: "amenbo-dev" }));
  });
});
