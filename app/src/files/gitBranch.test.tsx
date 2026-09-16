// @vitest-environment jsdom
// The branch line of the rail's git half: the list of branches, the move onto one, and the name
// typed into the list itself.
//
// What has to be right here is that git's own answer is what reaches the reader — the refusal about
// a working tree word for word, and the refusal about a name where they are still typing it — and
// that a move that went through sends the half around this one to look at the folder again.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { GitBranchDto } from "../bindings/bindings";
import { t, tf } from "../core/i18n";

const hoisted = vi.hoisted(() => ({
  /** Every branch of the folder's repository, as the host answers. */
  branches: [] as GitBranchDto[],
  /** The branches asked to be moved onto, in order. */
  switched: [] as string[],
  /** The names asked to be made, in order. */
  made: [] as string[],
  /** What the host refuses with, by the call it refuses — nothing where it does not. */
  refuse: {} as { switch?: string; create?: string },
  /** How many times the half around this one was told to look again. */
  moved: 0,
}));

vi.mock("./folder", () => ({
  folderGitBranches: async (): Promise<GitBranchDto[]> => hoisted.branches,
  folderGitSwitch: async (_p: number, _r: string, branch: string): Promise<string> => {
    hoisted.switched.push(branch);
    if (hoisted.refuse.switch !== undefined) throw new Error(hoisted.refuse.switch);
    return "";
  },
  folderGitBranchCreate: async (_p: number, _r: string, name: string): Promise<string> => {
    hoisted.made.push(name);
    if (hoisted.refuse.create !== undefined) throw new Error(hoisted.refuse.create);
    return "";
  },
}));

import { GitBranch } from "./GitBranch";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const ROOT = "/work/repo";

let container: HTMLDivElement;
let root: Root;

/** One branch, filled in around whatever a test cares about. */
const branch = (about: Partial<GitBranchDto> & { name: string }): GitBranchDto => ({
  upstream: null,
  ahead: 0,
  behind: 0,
  ...about,
});

/** Draw the line, standing on `on`. */
async function draw(on: GitBranchDto = branch({ name: "main" })) {
  await act(async () => {
    root.render(createElement(GitBranch, {
      projectId: 1,
      root: ROOT,
      on,
      onMoved: () => { hoisted.moved += 1; },
    }));
  });
  await settle();
}

/** Let whatever is on its way come back. */
async function settle() {
  await act(async () => { await new Promise((r) => setTimeout(r, 0)); });
}

/** Open the list. */
async function pick() {
  await act(async () => {
    container.querySelector<HTMLButtonElement>(".gitpanel__pick")?.click();
  });
  await settle();
}

const items = () => [...container.querySelectorAll<HTMLElement>(".menu__item")];
const box = () => container.querySelector<HTMLInputElement>(".files__namebox");

/** Type into the name box, the way a reader does. */
async function type(text: string) {
  const input = box();
  if (input === null) throw new Error("there is no box to type in");
  await act(async () => {
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set;
    setter?.call(input, text);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

/** Press one key in the name box. */
async function press(key: string) {
  await act(async () => {
    box()?.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
  });
  await settle();
}

beforeEach(() => {
  hoisted.branches = [];
  hoisted.switched = [];
  hoisted.made = [];
  hoisted.refuse = {};
  hoisted.moved = 0;
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the branch line", () => {
  /// The name and the counts are the same line the half drew before there was anything to press on
  /// it, and the press is added at the end of it.
  it("names the branch, and draws only the count there is one of", async () => {
    await draw(branch({ name: "task/4932", upstream: "origin/task/4932", ahead: 2 }));
    expect(container.querySelector(".gitpanel__branchname")?.textContent).toBe("task/4932");
    expect([...container.querySelectorAll(".gitpanel__count")].map((one) => one.textContent))
      .toEqual(["↑2"]);
  });

  /// Which branch is checked out is the one call that already answered it, not a second answer read
  /// off the list — so the mark goes on the row whose name is the one being stood on.
  it("marks the branch the folder is on, and says how far each stands", async () => {
    hoisted.branches = [
      branch({ name: "main", ahead: 1 }),
      branch({ name: "task/4932", behind: 3 }),
    ];
    await draw(branch({ name: "task/4932", behind: 3 }));
    await pick();
    expect(items().map((one) => one.querySelector(".gitpanel__pickname")?.textContent))
      .toEqual(["main", "task/4932", t("git.newBranch")]);
    const marked = items().filter((one) => one.querySelector("[data-icon='check']") !== null);
    expect(marked.map((one) => one.querySelector(".gitpanel__pickname")?.textContent))
      .toEqual(["task/4932"]);
    expect(items()[0]?.querySelector(".gitpanel__pickcount")?.textContent).toBe("↑1");
  });

  /// A move that went through changes every line the half around this one drew, so it is told to
  /// look again rather than left waiting for the folder to be noticed moving.
  it("moves onto the branch pressed, and says so", async () => {
    hoisted.branches = [branch({ name: "main" }), branch({ name: "other" })];
    await draw();
    await pick();
    await act(async () => { items()[1]?.click(); });
    await settle();
    expect(hoisted.switched).toEqual(["other"]);
    expect(hoisted.moved).toBe(1);
    expect(container.querySelector(".menu")).toBeNull();
  });

  /// Pressing the branch already checked out is not a move. git would be asked to go where it is
  /// and answer nothing.
  it("asks nothing where the branch pressed is the one being stood on", async () => {
    hoisted.branches = [branch({ name: "main" })];
    await draw();
    await pick();
    await act(async () => { items()[0]?.click(); });
    await settle();
    expect(hoisted.switched).toEqual([]);
    expect(hoisted.moved).toBe(0);
  });

  /// git's refusal is the ordinary answer to a checkout, and it is the reader's to read: which
  /// files stand in the way is in it, one per line (`AMB-D-906`, 3-4).
  it("draws what git refused with, word for word", async () => {
    const said = "error: Your local changes to the following files would be overwritten"
      + " by checkout:\n        worktree_cut.rs\nPlease commit your changes or stash them"
      + " before you switch branches.";
    hoisted.branches = [branch({ name: "main" }), branch({ name: "other" })];
    hoisted.refuse.switch = said;
    await draw();
    await pick();
    await act(async () => { items()[1]?.click(); });
    await settle();
    expect(container.querySelector(".gitpanel__said")?.textContent).toBe(said);
  });

  /// The name is made in the row itself, never in a window of its own — the way a name is made in
  /// the tree (`./FolderTree`).
  it("turns the last row into a box, and makes what is typed into it", async () => {
    hoisted.branches = [branch({ name: "main" })];
    await draw();
    await pick();
    expect(box()).toBeNull();
    await act(async () => { items()[1]?.click(); });
    expect(box()).not.toBeNull();
    expect(document.activeElement).toBe(box());

    await type("task/4940");
    await press("Enter");
    expect(hoisted.made).toEqual(["task/4940"]);
    expect(hoisted.moved).toBe(1);
    expect(container.querySelector(".menu")).toBeNull();
  });

  /// Which names git will have is the one thing a reader cannot work out for themselves, so the
  /// refusal is drawn where they are still typing, with what they wrote still in front of them.
  it("keeps the box open on a name git refused, with what was typed still in it", async () => {
    hoisted.branches = [branch({ name: "main" })];
    hoisted.refuse.create = "fatal: a branch named 'main' already exists";
    await draw();
    await pick();
    await act(async () => { items()[1]?.click(); });
    await type("main");
    await press("Enter");
    expect(hoisted.made).toEqual(["main"]);
    expect(box()?.value).toBe("main");
    expect(container.querySelector(".gitpanel__said")?.textContent)
      .toBe("fatal: a branch named 'main' already exists");
    expect(hoisted.moved).toBe(0);

    // A name git has already said no to is not one to ask about twice: typing is what clears it.
    await press("Enter");
    expect(hoisted.made).toEqual(["main"]);
  });

  /// The branch's name is what the row is about, so the list is read again every time it opens
  /// rather than kept from whenever it was last looked at.
  it("says nothing about a count of nothing", async () => {
    hoisted.branches = [branch({ name: "main" })];
    await draw();
    await pick();
    expect(items()[0]?.querySelector(".gitpanel__pickcount")?.textContent).toBe("");
    expect(tf("git.fromBranch", { name: "main" })).toContain("main");
  });
});
