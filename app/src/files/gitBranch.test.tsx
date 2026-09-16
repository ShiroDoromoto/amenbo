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
  /** The branches asked to be brought in, in order. */
  brought: [] as string[],
  /** How many times the merge underway was asked to be put back. */
  aborted: 0,
  /** What the host refuses with, by the call it refuses — nothing where it does not. */
  refuse: {} as { switch?: string; create?: string; merge?: string },
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
  folderGitMerge: async (_p: number, _r: string, branch: string): Promise<string> => {
    hoisted.brought.push(branch);
    if (hoisted.refuse.merge !== undefined) throw new Error(hoisted.refuse.merge);
    return "Fast-forward";
  },
  folderGitMergeAbort: async (): Promise<string> => {
    hoisted.aborted += 1;
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

/** Draw the line, standing on `on` — with or without a merge underway. */
async function draw(on: GitBranchDto = branch({ name: "main" }), merging = false) {
  await act(async () => {
    root.render(createElement(GitBranch, {
      projectId: 1,
      root: ROOT,
      on,
      merging,
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
  hoisted.brought = [];
  hoisted.aborted = 0;
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
      .toEqual(["main", "task/4932", t("git.newBranch"), t("git.mergeFrom")]);
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

  /// Which branch to bring in is the same question as which to move onto, asked of the same names —
  /// so it is a second face of the one list, and every row on it says which of the two it does.
  it("brings a branch in from the other face of the list", async () => {
    hoisted.branches = [branch({ name: "main" }), branch({ name: "other" })];
    await draw();
    await pick();
    await act(async () => { items()[3]?.click(); });
    await settle();
    // Not the row that was pressed: the face changed under it, and every branch but the one being
    // stood on is on the new one.
    expect(items().map((one) => one.querySelector(".gitpanel__pickname")?.textContent))
      .toEqual([tf("git.mergeOne", { name: "other" })]);

    await act(async () => { items()[0]?.click(); });
    await settle();
    expect(hoisted.brought).toEqual(["other"]);
    expect(hoisted.switched).toEqual([]);
    expect(hoisted.moved).toBe(1);
    expect(container.querySelector(".menu")).toBeNull();
  });

  /// A merge that stopped on conflicts exits non-zero and has still written the whole of them into
  /// the working tree, so git's words go up and the half around this one looks again either way.
  it("draws what a merge that stopped on conflicts said, and looks again anyway", async () => {
    const said = "Auto-merging folder.ts\nCONFLICT (content): Merge conflict in folder.ts\n"
      + "Automatic merge failed; fix conflicts and then commit the result.";
    hoisted.branches = [branch({ name: "main" }), branch({ name: "other" })];
    hoisted.refuse.merge = said;
    await draw();
    await pick();
    await act(async () => { items()[3]?.click(); });
    await act(async () => { items()[0]?.click(); });
    await settle();
    expect(hoisted.brought).toEqual(["other"]);
    expect(container.querySelector(".gitpanel__said")?.textContent).toBe(said);
    expect(container.querySelector(".gitpanel__said--refused")).not.toBeNull();
    expect(hoisted.moved).toBe(1);
  });

  /// git refuses a second merge in its own words, so the way to that face is out of the list while
  /// one is underway.
  it("offers no way in while a merge is already underway", async () => {
    hoisted.branches = [branch({ name: "main" }), branch({ name: "other" })];
    await draw(branch({ name: "main" }), true);
    await pick();
    expect(items().map((one) => one.querySelector(".gitpanel__pickname")?.textContent))
      .toEqual(["main", "other", t("git.newBranch")]);
  });

  /// A repository with one branch has nothing to bring into it, and git would answer a merge of a
  /// branch into itself with `Already up to date.`
  it("offers no way in where there is no other branch to bring", async () => {
    hoisted.branches = [branch({ name: "main" })];
    await draw();
    await pick();
    expect(items().map((one) => one.querySelector(".gitpanel__pickname")?.textContent))
      .toEqual(["main", t("git.newBranch")]);
  });

  /// A checkout made at a commit rather than at a branch: what a merge made there would belong to
  /// is a commit no branch names, so the way in is out of the list.
  it("offers no way in where the folder is on no branch", async () => {
    hoisted.branches = [branch({ name: "main" }), branch({ name: "other" })];
    await draw({ name: null, upstream: null, ahead: 0, behind: 0 });
    await pick();
    expect(items().map((one) => one.querySelector(".gitpanel__pickname")?.textContent))
      .toEqual(["main", "other", t("git.newBranch")]);
  });

  /// The band is what says a merge is underway, and it is drawn off `MERGE_HEAD` rather than off the
  /// conflicts — a merge with every conflict settled and staged has no conflict left to say so.
  it("draws the band while a merge is underway, and asks before putting the tree back", async () => {
    await draw(branch({ name: "main" }), true);
    expect(container.querySelector(".gitpanel__merging")?.textContent).toContain(t("git.merging"));
    expect(container.textContent).not.toContain(t("git.mergeAbortGone"));

    const press = (what: string) =>
      [...container.querySelectorAll<HTMLButtonElement>(".gitpanel__mergingdo .btn")]
        .find((one) => one.textContent === what);
    await act(async () => { press(t("git.mergeAbort"))?.click(); });
    // Asked, and not yet done: what a reader has settled by hand is in no commit and no reflog.
    expect(hoisted.aborted).toBe(0);
    expect(container.textContent).toContain(t("git.mergeAbortGone"));

    await act(async () => { press(t("git.mergeAbortKeep"))?.click(); });
    expect(hoisted.aborted).toBe(0);
    expect(container.textContent).not.toContain(t("git.mergeAbortGone"));

    await act(async () => { press(t("git.mergeAbort"))?.click(); });
    await act(async () => { press(t("git.mergeAbortGo"))?.click(); });
    await settle();
    expect(hoisted.aborted).toBe(1);
    expect(hoisted.moved).toBe(1);
  });

  /// Nothing of the merge is drawn where there is no merge, which is the ordinary state of a folder.
  it("draws no band where no merge is underway", async () => {
    await draw();
    expect(container.querySelector(".gitpanel__merging")).toBeNull();
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
