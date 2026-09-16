// @vitest-environment jsdom
// The rail's git half: where the branch stands, what git has to say about the folder under it, and
// what a reader does about it.
//
// What has to be right here is that the half says git's own answer and no more — the two letters
// decide which of the two lists a path is in, a folder that is no repository is a sentence rather
// than an empty list, and a count that is nothing is not drawn at all.
//
// And that every door it presses names the paths it is about (`AMB-D-906`): a commit that named
// none would take in the agent in the pane's half-staged work, and a stash that named none would
// take in its working tree. What git says in refusing is printed as git wrote it.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { FolderChangesDto, FolderGitDto, GitEntryDto, GitStashDto } from "../bindings/bindings";
import { t, tf } from "../core/i18n";

const hoisted = vi.hoisted(() => ({
  /** What git answers about each folder, by the folder it is about. */
  git: {} as Record<string, FolderGitDto>,
  /** Every folder the panel asked about, in order. */
  asked: [] as string[],
  /** Everyone listening for the host's word that a folder moved. */
  takers: [] as ((changes: FolderChangesDto) => void)[],
  /** What is put aside, as the list door reads it. */
  stashes: [] as GitStashDto[],
  /** How many conflicts are left in each path, by the whole path — the count read off the file. */
  marks: {} as Record<string, number>,
  /** What each door was handed, in order. */
  staged: [] as string[][][],
  unstaged: [] as string[][][],
  commits: [] as { message: string; paths: string[][] }[],
  stashed: [] as { message: string; paths: string[][] }[],
  popped: [] as string[],
  taken: [] as { paths: string[][]; mine: boolean }[],
  continued: [] as boolean[],
  /** What git says in refusing the next write, or nothing where it refuses none. */
  refuse: null as string | null,
  /** The watches laid and taken down, as the folder, the part that laid it and which mount. */
  watched: [] as string[],
  unwatched: [] as string[],
  tags: 0,
  /** Every call out to the remote, in order — the name of the one that was run. */
  ran: [] as string[],
  /** What the next call out to the remote answers with, and whether it answers by refusing. */
  answer: { text: "", refuse: null as unknown },
  /** Held back while a test wants a call to still be out. Let go with `hoisted.let()`. */
  held: null as null | (() => void),
  let: () => {},
}));

// Everything the factory reaches for is `hoisted`'s: the factory runs when the module under test is
// first imported, which is before any `const` in this file has been given its value.
vi.mock("./folder", () => {
  /** What a write door answers: it keeps what it was handed, and refuses where git would. */
  const kept = <T,>(into: T[], one: T): Promise<string> => {
    into.push(one);
    return hoisted.refuse !== null
      ? Promise.reject(new Error(hoisted.refuse))
      : Promise.resolve("");
  };
  /** One of the three, answering the way the test set it up to. */
  const reach = (name: string) => async (): Promise<string> => {
    hoisted.ran.push(name);
    if (hoisted.held !== null) await new Promise<void>((go) => { hoisted.let = () => { go(); }; });
    if (hoisted.answer.refuse !== null) throw hoisted.answer.refuse;
    return hoisted.answer.text;
  };
  return {
    folderGitStatus: async (_projectId: number, root: string): Promise<FolderGitDto> => {
      hoisted.asked.push(root);
      return hoisted.git[root] ?? { prefix: "", branch: null, rows: [], merging: false };
    },
    onFolderChanged: async (take: (changes: FolderChangesDto) => void) => {
      hoisted.takers.push(take);
      return () => { hoisted.takers = hoisted.takers.filter((one) => one !== take); };
    },
    nextWatchTag: () => {
      hoisted.tags += 1;
      return hoisted.tags;
    },
    folderWatch: async (_p: number, root: string, watcher: string, tag: number) => {
      hoisted.watched.push(`${root} ${watcher} ${tag}`);
      return { root, capped: false, unwatched: false, gone: false };
    },
    folderUnwatch: async (root: string, watcher: string, tag: number) => {
      hoisted.unwatched.push(`${root} ${watcher} ${tag}`);
    },
    folderGitStashes: async (): Promise<GitStashDto[]> => hoisted.stashes,
    folderGitMarks: async (_p: number, _r: string, paths: string[][]): Promise<number[]> =>
      paths.map((path) => hoisted.marks[path.join("/")] ?? 0),
    folderGitMergeContinue: () => kept(hoisted.continued, true),
    folderGitTake: (_p: number, _r: string, paths: string[][], mine: boolean) =>
      kept(hoisted.taken, { paths, mine }),
    folderGitStage: (_p: number, _r: string, paths: string[][]) => kept(hoisted.staged, paths),
    folderGitUnstage: (_p: number, _r: string, paths: string[][]) => kept(hoisted.unstaged, paths),
    folderGitCommit: (_p: number, _r: string, message: string, paths: string[][]) =>
      kept(hoisted.commits, { message, paths }),
    folderGitStash: (_p: number, _r: string, message: string, paths: string[][]) =>
      kept(hoisted.stashed, { message, paths }),
    folderGitStashPop: (_p: number, _r: string, name: string) => kept(hoisted.popped, name),
    // The branch line asks for these, and only once its list is opened — which no test here does.
    folderGitBranches: async () => [],
    folderGitSwitch: async () => "",
    folderGitBranchCreate: async () => "",
    folderGitFetch: reach("fetch"),
    folderGitPull: reach("pull"),
    folderGitPush: reach("push"),
  };
});

import { GitPanel } from "./GitPanel";
import { carriedIntoStage, carriedOverStage, type Held, STAGE_ATTR } from "./handDrag";
import type { DiffPick } from "./GitDiff";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const ROOT = "/work/repo";

let container: HTMLDivElement;
let root: Root;

/** One of git's rows, filled in around whatever a test cares about. */
const row = (about: Partial<GitEntryDto> & { path: string[] }): GitEntryDto => ({
  index: " ",
  worktree: " ",
  isDir: false,
  ...about,
});

/** What git answers, filled in around whatever a test cares about. */
const says = (about: Partial<FolderGitDto>): FolderGitDto => ({
  prefix: "",
  branch: { name: "main", upstream: "origin/main", ahead: 0, behind: 0 },
  rows: [],
  merging: false,
  said: null,
  ...about,
});

/** Every press of the way across to the history, so a test can read it back. */
let opened = 0;

async function draw(
  at: string | null = ROOT,
  onHistory?: () => void,
  onRead?: (path: string[]) => void,
  /** What the faces around this half hand down: the two the reading column has, and the gesture a
   *  row is taken up by. */
  around: {
    onPicked?: (one: DiffPick | null) => void;
    onDiff?: () => void;
    onCarry?: (held: Held) => void;
  } = {},
) {
  await act(async () => {
    root.render(
      createElement(GitPanel, { projectId: 1, root: at, onHistory, onRead, ...around }),
    );
  });
  await act(async () => { await new Promise((r) => setTimeout(r, 0)); });
}

/** One of the two lists, by the name over it. */
const sectionOf = (under: string): Element | undefined =>
  [...container.querySelectorAll(".gitpanel__section")]
    .find((one) => one.querySelector(".gitpanel__head")?.textContent?.startsWith(under));

/** The rows of one of the two lists, by the name over it. */
const listed = (under: string): string[] =>
  [...(sectionOf(under)?.querySelectorAll(".gitpanel__name") ?? [])].map((one) => one.textContent ?? "");

/** One list's row for one name, as the row it is drawn as. */
const rowOf = (under: string, name: string): Element =>
  [...(sectionOf(under)?.querySelectorAll(".gitpanel__row") ?? [])]
    .find((one) => one.querySelector(".gitpanel__name")?.textContent === name)!;

/** The box on that row. */
const box = (under: string, name: string): HTMLInputElement =>
  rowOf(under, name).querySelector<HTMLInputElement>(".gitpanel__check")!;

/** Whether that row's box is ticked. */
const ticked = (under: string, name: string): boolean => box(under, name).checked;

const messageBox = (): HTMLTextAreaElement =>
  container.querySelector<HTMLTextAreaElement>(".gitpanel__message")!;

const commitButton = (): HTMLButtonElement =>
  container.querySelector<HTMLButtonElement>(".gitpanel__do")!;

/** The last of that row: the one that opens what is put aside. */
const stashButton = (): HTMLButtonElement =>
  net().find((one) => one.textContent === t("git.stash"))!;

/** What the open menu offers, as the words on each item. */
const itemNames = (): string[] =>
  [...document.querySelectorAll(".menu__item")].map((one) => one.textContent ?? "");

/** One item of the open menu, by the words on it. */
const menuItem = (words: string): HTMLElement =>
  [...document.querySelectorAll<HTMLElement>(".menu__item")]
    .find((one) => one.textContent === words)!;

/** Open the menu a row carries, the way a reader does. */
async function rightClickOn(what: Element) {
  await act(async () => {
    what.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true }));
    await new Promise((r) => setTimeout(r, 0));
  });
}

/** Press one thing, and let whatever it asked for come back. */
async function press(what: HTMLElement) {
  await act(async () => {
    what.click();
    await new Promise((r) => setTimeout(r, 0));
  });
}

/** Write `words` into the commit box, in place of whatever is in it. */
async function type(words: string) {
  const at = messageBox();
  await act(async () => {
    const set = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")!.set!;
    set.call(at, words);
    at.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

/** The host's word that this folder moved, which is the half's cue to read it again. */
async function moveFolder() {
  await act(async () => {
    for (const take of [...hoisted.takers]) {
      take({ root: ROOT, capped: false, unwatched: false, gone: false });
    }
    await new Promise((r) => setTimeout(r, 0));
  });
}

/** The buttons of that row, in the order they are drawn: the three of the remote, then the stash. */
const net = (): HTMLButtonElement[] =>
  [...container.querySelectorAll<HTMLButtonElement>(".gitpanel__net .btn")];

beforeEach(() => {
  opened = 0;
  hoisted.git = {};
  hoisted.asked = [];
  hoisted.takers = [];
  hoisted.stashes = [];
  hoisted.staged = [];
  hoisted.unstaged = [];
  hoisted.commits = [];
  hoisted.stashed = [];
  hoisted.popped = [];
  hoisted.marks = {};
  hoisted.taken = [];
  hoisted.continued = [];
  hoisted.refuse = null;
  hoisted.watched = [];
  hoisted.unwatched = [];
  hoisted.tags = 0;
  hoisted.ran = [];
  hoisted.answer = { text: "", refuse: null };
  hoisted.held = null;
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the rail's git half", () => {
  it("names the branch, and draws only the count there is one of", async () => {
    hoisted.git[ROOT] = says({
      branch: { name: "task/4928", upstream: "origin/task/4928", ahead: 2, behind: 0 },
    });
    await draw();
    expect(container.querySelector(".gitpanel__branchname")?.textContent).toBe("task/4928");
    const counts = [...container.querySelectorAll(".gitpanel__count")].map((one) => one.textContent);
    // A pair of zeroes is a branch level with the one it is measured by, and two numbers saying
    // "nothing to do" are worse than none.
    expect(counts).toEqual(["↑2"]);
    expect(container.querySelector(".gitpanel__count")?.getAttribute("title"))
      .toBe(tf("git.ahead", { n: 2 }));
  });

  /// A checkout made at a commit rather than at a branch. git writes no name there.
  it("says so where the checkout is on no branch", async () => {
    hoisted.git[ROOT] = says({ branch: { name: null, upstream: null, ahead: 0, behind: 0 } });
    await draw();
    expect(container.querySelector(".gitpanel__branchname")?.textContent).toBe(t("git.detached"));
  });

  /// One project in twenty-one on this machine. An empty list there would read as a repository
  /// where nothing has happened.
  it("draws the sentence and nothing else where the folder is no repository", async () => {
    hoisted.git[ROOT] = { prefix: "", branch: null, rows: [], merging: false, said: null };
    await draw();
    expect(container.textContent).toContain(t("git.noRepo"));
    expect(container.querySelector(".gitpanel__section")).toBeNull();
  });

  /// A repository whose `git status` came back non-zero — the LFS filter a `.gitattributes` names
  /// is not on the window's `PATH` (`AMB-T-4979` measured it). The branch is empty either way, and
  /// telling the reader their repository is not one is a falsehood they cannot act on.
  it("draws what git said where it refused to answer, and not the no-repository sentence", async () => {
    const refusal = "git-lfs filter-process: git-lfs: command not found\n"
      + "fatal: the remote end hung up unexpectedly";
    hoisted.git[ROOT] = { prefix: "", branch: null, rows: [], merging: false, said: refusal };
    await draw();
    expect(container.textContent).toContain(refusal);
    expect(container.textContent).toContain(t("git.noAnswer"));
    expect(container.textContent).not.toContain(t("git.noRepo"));
    expect(container.querySelector(".gitpanel__said--refused")).not.toBeNull();
  });

  /// Which list a path is in is git's answer and not a reader's sorting: `X` is what the index says
  /// and `Y` what the working tree says, and a path can have something in each.
  it("puts a path in the list each of git's two letters says it is in", async () => {
    hoisted.git[ROOT] = says({
      rows: [
        row({ path: ["staged.rs"], index: "M" }),
        row({ path: ["working.rs"], worktree: "M" }),
        row({ path: ["both.rs"], index: "M", worktree: "M" }),
        // Never seen by git at all. There is nothing staged about it, whatever the first letter is.
        row({ path: ["new.md"], index: "?", worktree: "?" }),
      ],
    });
    await draw();
    expect(listed(t("git.staged"))).toEqual(["staged.rs", "both.rs"]);
    expect(listed(t("git.changes"))).toEqual(["working.rs", "both.rs", "new.md"]);
  });

  /// A path the merge could not settle is in neither of the two lists. What a box does there is
  /// stage, and staging a conflict is the one press that says the conflict has been settled.
  it("keeps what the merge could not settle out of the two lists", async () => {
    hoisted.git[ROOT] = says({
      rows: [
        row({ path: ["both.rs"], index: "U", worktree: "U" }),
        // Each of the other six ways git writes a conflict, and an ordinary change beside them.
        row({ path: ["added.rs"], index: "A", worktree: "A" }),
        row({ path: ["dropped.rs"], index: "D", worktree: "D" }),
        row({ path: ["mine.rs"], index: "U", worktree: "D" }),
        row({ path: ["theirs.rs"], index: "D", worktree: "U" }),
        row({ path: ["ours.rs"], index: "U", worktree: "A" }),
        row({ path: ["incoming.rs"], index: "A", worktree: "U" }),
        row({ path: ["plain.rs"], index: "M", worktree: "M" }),
      ],
    });
    await draw();
    expect(listed(t("git.conflicts"))).toEqual([
      "both.rs", "added.rs", "dropped.rs", "mine.rs", "theirs.rs", "ours.rs", "incoming.rs",
    ]);
    expect(listed(t("git.staged"))).toEqual(["plain.rs"]);
    expect(listed(t("git.changes"))).toEqual(["plain.rs"]);
  });

  /// The number is counted off the file and not off git's index, so it is the answer to "is this
  /// one done" before anybody has said so — and the press that says so is offered only at none.
  it("draws how much of each conflict is left, and offers the press only at none", async () => {
    hoisted.git[ROOT] = says({
      rows: [
        row({ path: ["left.rs"], index: "U", worktree: "U" }),
        row({ path: ["done.rs"], index: "U", worktree: "U" }),
      ],
    });
    hoisted.marks = { "left.rs": 3 };
    await draw();
    const left = rowOf(t("git.conflicts"), "left.rs");
    expect(left.querySelector(".gitpanel__marks")?.textContent).toBe(tf("git.conflictMarks", { n: 3 }));
    expect(left.querySelector(".gitpanel__settle")).toBeNull();
    const done = rowOf(t("git.conflicts"), "done.rs");
    expect(done.querySelector(".gitpanel__marks")).toBeNull();
    expect(done.querySelector(".gitpanel__settle")?.textContent).toBe(t("git.settleOne"));
  });

  /// Nothing stages a conflict by itself, however little is left in the file: the press is the
  /// reader saying it is settled, and it names the one path it was made on.
  it("stages the path the press to settle was made on, and only that one", async () => {
    hoisted.git[ROOT] = says({
      rows: [
        row({ path: ["src", "one.rs"], index: "U", worktree: "U" }),
        row({ path: ["src", "two.rs"], index: "U", worktree: "U" }),
      ],
    });
    await draw();
    expect(hoisted.staged).toEqual([]);
    await press(rowOf(t("git.conflicts"), "one.rs").querySelector<HTMLElement>(".gitpanel__settle")!);
    expect(hoisted.staged).toEqual([[["src", "one.rs"]]]);
  });

  /// git wrote the message when it began the merge and takes that one, so a box to write in would
  /// be one a reader types into and is never asked for. And git refuses to end a merge while a path
  /// is still unmerged, which is why the press is down until the list is empty.
  it("swaps the commit box for the press that ends the merge, and holds it down until none is left", async () => {
    hoisted.git[ROOT] = says({
      merging: true,
      rows: [row({ path: ["both.rs"], index: "U", worktree: "U" })],
    });
    await draw();
    expect(container.querySelector(".gitpanel__message")).toBeNull();
    expect(commitButton().textContent).toBe(t("git.mergeContinue"));
    expect(commitButton().disabled).toBe(true);
    expect(container.textContent).toContain(tf("git.conflictsLeft", { n: 1 }));

    hoisted.git[ROOT] = says({ merging: true, rows: [row({ path: ["both.rs"], index: "M" })] });
    await moveFolder();
    expect(commitButton().disabled).toBe(false);
    expect(container.textContent).toContain(t("git.conflictsSettled"));
    await press(commitButton());
    expect(hoisted.continued).toEqual([true]);
  });

  /// A conflicted file is an ordinary file, and this half is the tab the tree is not: a reader
  /// standing on the list has no other way to the file the conflict is settled in.
  it("opens the file a conflicted row names, by the path this half spells", async () => {
    hoisted.git[ROOT] = says({
      rows: [row({ path: ["src", "both.rs"], index: "U", worktree: "U" })],
    });
    const read: string[][] = [];
    await draw(ROOT, undefined, (path) => read.push(path));
    await press(
      rowOf(t("git.conflicts"), "both.rs").querySelector<HTMLElement>(".gitpanel__conflictname")!,
    );
    expect(read).toEqual([["src", "both.rs"]]);
  });

  /// Taking one side whole is the short way out of a conflict and is refused over anything else,
  /// so it is drawn on a conflicted row and nowhere else.
  it("offers taking one side whole on a conflicted row and on no other", async () => {
    hoisted.git[ROOT] = says({
      rows: [
        row({ path: ["both.rs"], index: "U", worktree: "U" }),
        row({ path: ["plain.rs"], worktree: "M" }),
      ],
    });
    await draw();
    await rightClickOn(rowOf(t("git.changes"), "plain.rs"));
    expect(itemNames()).not.toContain(t("git.takeOurs"));

    await rightClickOn(rowOf(t("git.conflicts"), "both.rs"));
    expect(itemNames()).toContain(t("git.takeOurs"));
    expect(itemNames()).toContain(t("git.takeTheirs"));
    await press(menuItem(t("git.takeTheirs")));
    expect(hoisted.taken).toEqual([{ paths: [["both.rs"]], mine: false }]);
  });

  /// A folder git named as a whole rather than naming what is inside it, which is what it does with
  /// an untracked one. The slash is how git writes that.
  it("draws a folder git answered for as a whole as the folder it is", async () => {
    hoisted.git[ROOT] = says({
      rows: [row({ path: ["newdir"], index: "?", worktree: "?", isDir: true })],
    });
    await draw();
    expect(listed(t("git.changes"))).toEqual(["newdir/"]);
  });

  /// Both lists are drawn either way: which of the two a path is in is the answer, and a list that
  /// disappeared would leave the other one unnamed.
  it("names both lists where neither has anything in it", async () => {
    hoisted.git[ROOT] = says({});
    await draw();
    expect(container.textContent).toContain(t("git.nothingStaged"));
    expect(container.textContent).toContain(t("git.nothingChanged"));
  });

  /// The name and the folder holding it are told apart, so a list of twenty changes can be read
  /// down the names rather than down the paths.
  it("draws the name apart from the folder holding it", async () => {
    hoisted.git[ROOT] = says({ rows: [row({ path: ["src", "deep", "lib.rs"], worktree: "M" })] });
    await draw();
    expect(container.querySelector(".gitpanel__name")?.textContent).toBe("lib.rs");
    expect(container.querySelector(".gitpanel__where")?.textContent).toBe("src/deep");
    // And the whole of it is still reachable, for a name two folders of one repository share.
    expect(container.querySelector(".gitpanel__row")?.getAttribute("title")).toBe("src/deep/lib.rs");
  });

  /// Staging moves not one byte of the working tree and every line of this list, so the word the
  /// host says when the folder moves is the moment to look again.
  it("asks again when the host says the folder moved, and not for another folder's word", async () => {
    hoisted.git[ROOT] = says({});
    await draw();
    expect(hoisted.asked).toEqual([ROOT]);

    await act(async () => {
      for (const take of [...hoisted.takers]) {
        take({ root: "/work/elsewhere", capped: false, unwatched: false, gone: false });
      }
      await new Promise((r) => setTimeout(r, 0));
    });
    expect(hoisted.asked).toEqual([ROOT]);

    await act(async () => {
      for (const take of [...hoisted.takers]) {
        take({ root: ROOT, capped: false, unwatched: false, gone: false });
      }
      await new Promise((r) => setTimeout(r, 0));
    });
    expect(hoisted.asked).toEqual([ROOT, ROOT]);
  });

  /// The tree lays the watch while the tree is drawn, and this half is drawn in its place — so a
  /// half leaning on the tree's watch would hear nothing for as long as anybody looked at it.
  it("watches the folder itself while it is drawn, and lets go as it goes", async () => {
    hoisted.git[ROOT] = says({});
    await draw();
    expect(hoisted.watched).toEqual([`${ROOT} git 1`]);
    expect(hoisted.unwatched).toEqual([]);

    await act(async () => { root.render(createElement(GitPanel, { projectId: 1, root: null })); });
    expect(hoisted.unwatched).toEqual([`${ROOT} git 1`]);
  });
  /// The history is read in the column across the panes, so what stands here is the way to it —
  /// and nothing of it is asked for until that press is made (`AMB-T-4899`).
  it("offers the way across to the history, and asks nothing of it here", async () => {
    hoisted.git[ROOT] = says({});
    await draw(ROOT, () => { opened += 1; });
    const across = container.querySelector<HTMLElement>(".gitpanel__open");
    expect(across?.textContent).toContain(t("git.history"));
    await act(async () => { across?.click(); });
    expect(opened).toBe(1);
  });

  /// Nothing is handed down where there is nowhere for it to open, and a press that reaches nothing
  /// is not offered.
  it("offers no way across where there is nowhere to open it", async () => {
    hoisted.git[ROOT] = says({});
    await draw();
    expect(container.querySelector(".gitpanel__open")).toBeNull();
  });

  /// The face has not been told which folder it is on yet. Nothing is asked and nothing is said:
  /// there is no folder here for a sentence to be about.
  it("asks nothing where there is no folder to ask about", async () => {
    await draw(null);
    expect(hoisted.asked).toEqual([]);
    expect(container.textContent).toBe("");
  });

  /// The window runs the three itself (`AMB-T-4900`), and the one that sends carries the count of
  /// what it would send. What is put aside stands at the end of the same row.
  it("draws the three that go out to the remote, with what push would send on it", async () => {
    hoisted.git[ROOT] = says({
      branch: { name: "main", upstream: "origin/main", ahead: 2, behind: 0 },
    });
    await draw();
    expect(net().map((one) => one.textContent))
      .toEqual([t("git.fetch"), t("git.pull"), `${t("git.push")} ↑2`, t("git.stash")]);
  });

  /// The fill belongs to the count, not to the button: it is on where the count is on.
  it("fills push where there is something to send", async () => {
    hoisted.git[ROOT] = says({
      branch: { name: "main", upstream: "origin/main", ahead: 1, behind: 0 },
    });
    await draw();
    expect(net()[2]!.className).toContain("btn--primary");
  });

  /// With nothing to send, push stands with the other three. A filled button there recommends a
  /// press that does nothing, beside a label that already says so by carrying no count.
  it("leaves push unfilled where there is nothing to send", async () => {
    hoisted.git[ROOT] = says({
      branch: { name: "main", upstream: "origin/main", ahead: 0, behind: 0 },
    });
    await draw();
    expect(net()[2]!.className).not.toContain("btn--primary");
  });

  it("runs the one that was pressed, and no other", async () => {
    hoisted.git[ROOT] = says({});
    await draw();
    await press(net()[1]!);
    expect(hoisted.ran).toEqual(["pull"]);
  });

  /// git's own sentence, in git's own words (`AMB-D-906`, 3-4). An upstream that was never set is
  /// the case a reader meets first, and git's answer is the one that says what to do about it.
  it("draws what git said when it refused, as git wrote it", async () => {
    hoisted.git[ROOT] = says({});
    hoisted.answer = {
      text: "",
      refuse: { code: "error", message_en: "fatal: The current branch main has no upstream branch." },
    };
    await draw();
    await press(net()[2]!);
    const said = container.querySelector(".gitpanel__said");
    expect(said?.textContent).toBe("fatal: The current branch main has no upstream branch.");
    expect(said?.className).toContain("gitpanel__said--refused");
  });

  /// A fetch that found nothing writes nothing, and a button that answers with silence reads as one
  /// that did not work.
  it("says so where git worked and wrote nothing", async () => {
    hoisted.git[ROOT] = says({});
    await draw();
    await press(net()[0]!);
    const said = container.querySelector(".gitpanel__said");
    expect(said?.textContent).toBe(t("git.quiet"));
    expect(said?.className).not.toContain("gitpanel__said--refused");
  });

  /// Where the branch stands is what these three move, so the answer above them is asked for again
  /// rather than waited on.
  it("asks git again once the call comes back", async () => {
    hoisted.git[ROOT] = says({});
    await draw();
    expect(hoisted.asked).toEqual([ROOT]);
    await press(net()[0]!);
    expect(hoisted.asked).toEqual([ROOT, ROOT]);
  });

  /// One call at a time: a second press while one is out would be a second process against the same
  /// repository, and the reader has no way of telling which of the two the answer came from.
  it("puts the three down while one of them is out, and back up after it", async () => {
    hoisted.git[ROOT] = says({});
    hoisted.held = () => {};
    await draw();
    await press(net()[0]!);
    expect(net().every((one) => one.disabled)).toBe(true);
    expect(container.querySelector(".gitpanel__said")?.textContent).toBe(t("git.running"));

    hoisted.held = null;
    await act(async () => {
      hoisted.let();
      await new Promise((r) => setTimeout(r, 0));
    });
    expect(net().every((one) => one.disabled)).toBe(false);
    expect(hoisted.ran).toEqual(["fetch"]);
  });
});

describe("the rail's git half, pressed", () => {
  /// The box on a row of the changes list is the row not being staged, and pressing it is what
  /// stages that one path — named to git and not left to whatever the index happens to hold.
  it("stages the one path a box of the changes list was pressed on", async () => {
    hoisted.git[ROOT] = says({ rows: [row({ path: ["src", "lib.rs"], worktree: "M" })] });
    await draw();
    await press(box(t("git.changes"), "lib.rs"));
    expect(hoisted.staged).toEqual([[["src", "lib.rs"]]]);
    expect(hoisted.unstaged).toEqual([]);
  });

  /// And the box on a row of the staged list is the row being staged, so pressing it is the other
  /// way — which is how a path in both lists is a ticked row and an empty one at the same time.
  it("unstages the one path a box of the staged list was pressed on", async () => {
    hoisted.git[ROOT] = says({ rows: [row({ path: ["lib.rs"], index: "M", worktree: "M" })] });
    await draw();
    expect(ticked(t("git.staged"), "lib.rs")).toBe(true);
    expect(ticked(t("git.changes"), "lib.rs")).toBe(false);
    await press(box(t("git.staged"), "lib.rs"));
    expect(hoisted.unstaged).toEqual([[["lib.rs"]]]);
    expect(hoisted.staged).toEqual([]);
  });

  /// The pathspec is the point of the whole screen (`AMB-D-906`, 3-2): the commit is about the paths
  /// the list was read with, so the agent in the pane's half-staged work is not written down too.
  it("names every staged path to the commit, and empties the box once it is written down", async () => {
    hoisted.git[ROOT] = says({
      rows: [row({ path: ["a.rs"], index: "M" }), row({ path: ["b", "c.rs"], index: "A" })],
    });
    await draw();
    await type("fix: the one thing");
    expect(container.querySelector(".gitpanel__hint")?.textContent)
      .toBe(tf("git.commitNaming", { n: 2 }));
    await press(commitButton());
    expect(hoisted.commits).toEqual([{ message: "fix: the one thing", paths: [["a.rs"], ["b", "c.rs"]] }]);
    expect(messageBox().value).toBe("");
  });

  /// Nothing staged is nothing to name, and no words are no commit. Both are the shape of the press
  /// rather than a rewriting of git — what git would have said is never reached.
  it("will not commit with nothing staged, nor with nothing written", async () => {
    hoisted.git[ROOT] = says({ rows: [row({ path: ["a.rs"], worktree: "M" })] });
    await draw();
    await type("fix: the one thing");
    expect(commitButton().disabled).toBe(true);

    hoisted.git[ROOT] = says({ rows: [row({ path: ["a.rs"], index: "M" })] });
    await moveFolder();
    expect(commitButton().disabled).toBe(false);
    await type("   ");
    expect(commitButton().disabled).toBe(true);
  });

  /// A box that moved its row from one list to the other has already said that it worked. The line
  /// that stands in for git's silence is for the doors that move nothing on this screen.
  it("says nothing of git's silence where the lists have already shown the work", async () => {
    hoisted.git[ROOT] = says({ rows: [row({ path: ["a.rs"], worktree: "M" })] });
    await draw();
    await press(box(t("git.changes"), "a.rs"));
    expect(container.querySelector(".gitpanel__said")).toBeNull();
    expect(container.textContent).not.toContain(t("git.quiet"));
  });

  /// git's own sentence, word for word (`AMB-D-906`, 3-4) — and the words the reader typed still in
  /// the box, because a commit git would not make is one they are about to ask for again.
  it("prints what git said in refusing, and keeps what was typed", async () => {
    hoisted.git[ROOT] = says({ rows: [row({ path: ["a.rs"], index: "M" })] });
    hoisted.refuse = "error: cannot commit\nPlease sort it out first.";
    await draw();
    await type("fix: the one thing");
    await press(commitButton());
    expect(container.querySelector(".gitpanel__said--refused")?.textContent)
      .toBe("error: cannot commit\nPlease sort it out first.");
    expect(messageBox().value).toBe("fix: the one thing");
  });

  /// The word from the host is gathered over 400ms, which is right for a folder somebody else is
  /// writing to and too slow for a box the reader has just ticked.
  it("reads the folder again itself once a write comes back", async () => {
    hoisted.git[ROOT] = says({ rows: [row({ path: ["a.rs"], worktree: "M" })] });
    await draw();
    expect(hoisted.asked).toEqual([ROOT]);
    await press(box(t("git.changes"), "a.rs"));
    expect(hoisted.asked).toEqual([ROOT, ROOT]);
  });

  /// Only the paths git follows. An untracked one named in a stash's pathspec is refused outright —
  /// `did not match any file(s) known to git`, with nothing put aside.
  it("puts aside the followed paths and leaves the untracked ones out", async () => {
    hoisted.git[ROOT] = says({
      rows: [
        row({ path: ["a.rs"], worktree: "M" }),
        row({ path: ["new.md"], index: "?", worktree: "?" }),
      ],
    });
    await draw();
    await press(stashButton());
    await press(menuItem(t("git.stashPush")));
    expect(hoisted.stashed).toEqual([{ message: "", paths: [["a.rs"]] }]);
  });

  /// Nothing git follows is nothing to put aside, and a door that could only ever come back with
  /// git's refusal is one to leave out rather than to offer.
  it("offers no way to put aside where git follows none of it", async () => {
    hoisted.git[ROOT] = says({ rows: [row({ path: ["new.md"], index: "?", worktree: "?" })] });
    await draw();
    await press(stashButton());
    expect(itemNames()).not.toContain(t("git.stashPush"));
    expect(document.body.textContent).toContain(t("git.stashEmpty"));
  });

  /// `stash@{0}` is where a stash sits and not what it is, so it is restored by the name the list
  /// was just read with — and the list is read when it opens, not kept from an earlier look.
  it("restores a stash by the name the list it was drawn from was read with", async () => {
    hoisted.git[ROOT] = says({ rows: [row({ path: ["a.rs"], worktree: "M" })] });
    hoisted.stashes = [
      { name: "stash@{0}", message: "On main: the newer one", at: "2026-09-16T00:00:00+09:00" },
      { name: "stash@{1}", message: "On main: the older one", at: "2026-09-15T00:00:00+09:00" },
    ];
    await draw();
    await press(stashButton());
    expect(itemNames()).toContain(`On main: the newer one${t("git.stashRestore")}`);
    await press(menuItem(`On main: the older one${t("git.stashRestore")}`));
    expect(hoisted.popped).toEqual(["stash@{1}"]);
  });
});

/// Rows are gathered here the way they are in the tree, because the two halves are one rail: ⌘
/// takes a row in, Shift reaches from where the reader was, and the arrows walk the list. What the
/// set is for is the doors that act on several rows at once, so what has to be right is which rows
/// are in it — and that a press meant for one row does not quietly put the set down.
describe("the rail's git half, with rows picked out", () => {
  /** The rows of one list that say they are picked out, by their names. */
  const pickedIn = (under: string): string[] =>
    [...(sectionOf(under)?.querySelectorAll("[aria-selected=\"true\"]") ?? [])]
      .map((one) => one.querySelector(".gitpanel__name")?.textContent ?? "");

  /** Press a row with keys held down, the way a reader gathering rows does. */
  async function clickWith(what: Element, keys: MouseEventInit) {
    await act(async () => {
      what.dispatchEvent(new MouseEvent("click", { bubbles: true, ...keys }));
      await new Promise((r) => setTimeout(r, 0));
    });
  }

  /** A key pressed on the row the keyboard is standing on. */
  async function keyOn(what: Element, key: string, keys: KeyboardEventInit = {}) {
    await act(async () => {
      what.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, key, ...keys }));
      await new Promise((r) => setTimeout(r, 0));
    });
  }

  /** The box on the line a list is named on, or nothing where the list has none. */
  const allBox = (under: string): HTMLInputElement | undefined =>
    sectionOf(under)?.querySelector<HTMLInputElement>(".gitpanel__headrow .gitpanel__check")
      ?? undefined;

  /** Four changed paths, which is enough for a range to have rows inside it. */
  const four = (): FolderGitDto => says({
    rows: [
      row({ path: ["a.rs"], worktree: "M" }),
      row({ path: ["b.rs"], worktree: "M" }),
      row({ path: ["c.rs"], worktree: "M" }),
      row({ path: ["d.rs"], worktree: "M" }),
    ],
  });

  it("takes a row into the set with the machine's key, and back out of it", async () => {
    hoisted.git[ROOT] = four();
    await draw();
    await clickWith(rowOf(t("git.changes"), "a.rs"), {});
    expect(pickedIn(t("git.changes"))).toEqual(["a.rs"]);

    await clickWith(rowOf(t("git.changes"), "c.rs"), { metaKey: true, ctrlKey: true });
    expect(pickedIn(t("git.changes"))).toEqual(["a.rs", "c.rs"]);

    await clickWith(rowOf(t("git.changes"), "a.rs"), { metaKey: true, ctrlKey: true });
    expect(pickedIn(t("git.changes"))).toEqual(["c.rs"]);
  });

  it("reaches from the end the range is measured from to the row Shift was pressed on", async () => {
    hoisted.git[ROOT] = four();
    await draw();
    await clickWith(rowOf(t("git.changes"), "b.rs"), {});
    await clickWith(rowOf(t("git.changes"), "d.rs"), { shiftKey: true });
    expect(pickedIn(t("git.changes"))).toEqual(["b.rs", "c.rs", "d.rs"]);

    // Both ways: a range pulled back past its own start grows the other way from the same end.
    await clickWith(rowOf(t("git.changes"), "a.rs"), { shiftKey: true });
    expect(pickedIn(t("git.changes"))).toEqual(["a.rs", "b.rs"]);
  });

  it("walks the list with the arrows, and reaches with them while Shift is held", async () => {
    hoisted.git[ROOT] = four();
    await draw();
    await clickWith(rowOf(t("git.changes"), "a.rs"), {});
    await keyOn(rowOf(t("git.changes"), "a.rs"), "ArrowDown");
    expect(pickedIn(t("git.changes"))).toEqual(["b.rs"]);

    await keyOn(rowOf(t("git.changes"), "b.rs"), "ArrowDown", { shiftKey: true });
    expect(pickedIn(t("git.changes"))).toEqual(["b.rs", "c.rs"]);

    await keyOn(rowOf(t("git.changes"), "c.rs"), "End", { shiftKey: true });
    expect(pickedIn(t("git.changes"))).toEqual(["b.rs", "c.rs", "d.rs"]);
  });

  /// Staging is what a changed row takes and unstaging what a staged one takes, so a set spanning
  /// both lists would be a press with two meanings.
  it("puts down what was picked in one list when a row of another is pressed", async () => {
    hoisted.git[ROOT] = says({
      rows: [
        row({ path: ["a.rs"], worktree: "M" }),
        row({ path: ["b.rs"], worktree: "M" }),
        row({ path: ["kept.rs"], index: "M" }),
      ],
    });
    await draw();
    await clickWith(rowOf(t("git.changes"), "a.rs"), {});
    await clickWith(rowOf(t("git.changes"), "b.rs"), { metaKey: true, ctrlKey: true });
    expect(pickedIn(t("git.changes"))).toEqual(["a.rs", "b.rs"]);

    await clickWith(rowOf(t("git.staged"), "kept.rs"), { metaKey: true, ctrlKey: true });
    expect(pickedIn(t("git.staged"))).toEqual(["kept.rs"]);
    expect(pickedIn(t("git.changes"))).toEqual([]);
  });

  /// The box is what the list does to one path. A reader who gathered five rows to act on has not
  /// begun again by ticking one of them.
  it("leaves the set where it is when the box on a row is pressed", async () => {
    hoisted.git[ROOT] = four();
    await draw();
    await clickWith(rowOf(t("git.changes"), "a.rs"), {});
    await clickWith(rowOf(t("git.changes"), "b.rs"), { metaKey: true, ctrlKey: true });

    await press(box(t("git.changes"), "c.rs"));
    expect(hoisted.staged).toEqual([[["c.rs"]]]);
    expect(pickedIn(t("git.changes"))).toEqual(["a.rs", "b.rs"]);
  });

  /// A path that has left the list it was picked in is a row nobody can see, and a set holding one
  /// is a set the next press acts on silently. Staging one of three is how it happens.
  it("keeps the rows still on the list when one of the set leaves it", async () => {
    hoisted.git[ROOT] = four();
    await draw();
    await clickWith(rowOf(t("git.changes"), "a.rs"), {});
    await clickWith(rowOf(t("git.changes"), "c.rs"), { shiftKey: true });
    expect(pickedIn(t("git.changes"))).toEqual(["a.rs", "b.rs", "c.rs"]);

    hoisted.git[ROOT] = says({
      rows: [
        row({ path: ["b.rs"], worktree: "M" }),
        row({ path: ["c.rs"], worktree: "M" }),
        row({ path: ["d.rs"], worktree: "M" }),
        row({ path: ["a.rs"], index: "M" }),
      ],
    });
    await moveFolder();
    expect(pickedIn(t("git.changes"))).toEqual(["b.rs", "c.rs"]);
  });

  /// A menu opened away from what is picked is a menu about the row under the pointer: the
  /// alternative is a box standing over one row and acting on others.
  it("acts on the whole set from a row in it, and on one row from a row outside it", async () => {
    hoisted.git[ROOT] = says({
      rows: [
        row({ path: ["a.rs"], index: "U", worktree: "U" }),
        row({ path: ["b.rs"], index: "U", worktree: "U" }),
        row({ path: ["c.rs"], index: "U", worktree: "U" }),
      ],
    });
    await draw();
    await clickWith(rowOf(t("git.conflicts"), "a.rs"), {});
    await clickWith(rowOf(t("git.conflicts"), "b.rs"), { metaKey: true, ctrlKey: true });
    await rightClickOn(rowOf(t("git.conflicts"), "b.rs"));
    await press(menuItem(t("git.takeOurs")));
    expect(hoisted.taken).toEqual([{ paths: [["a.rs"], ["b.rs"]], mine: true }]);

    await rightClickOn(rowOf(t("git.conflicts"), "c.rs"));
    expect(pickedIn(t("git.conflicts"))).toEqual(["c.rs"]);
    await press(menuItem(t("git.takeTheirs")));
    expect(hoisted.taken[1]).toEqual({ paths: [["c.rs"]], mine: false });
  });

  /// `git rm --cached` refuses a pathspec naming a path git has never seen, and refuses the whole
  /// of it — so over a set with an untracked row among them the press would fail for all of them.
  /// It is drawn and greyed rather than taken away: the menu keeps one shape whatever is picked.
  it("greys the door that stops git following rows it never followed", async () => {
    hoisted.git[ROOT] = says({
      rows: [
        row({ path: ["tracked.rs"], worktree: "M" }),
        row({ path: ["new.rs"], index: "?", worktree: "?" }),
      ],
    });
    await draw();
    await rightClickOn(rowOf(t("git.changes"), "tracked.rs"));
    expect(menuItem(t("git.untrack")).getAttribute("aria-disabled")).toBeNull();

    await clickWith(rowOf(t("git.changes"), "tracked.rs"), {});
    await clickWith(rowOf(t("git.changes"), "new.rs"), { shiftKey: true });
    await rightClickOn(rowOf(t("git.changes"), "new.rs"));
    expect(itemNames()).toContain(t("git.untrack"));
    expect(menuItem(t("git.untrack")).getAttribute("aria-disabled")).toBe("true");
    // One path's history is the other door a set of rows is not in a state for.
    expect(menuItem(t("git.fileHistory")).getAttribute("aria-disabled")).toBe("true");
  });
  /// One press, one call out to git. Six files used to be six presses with the whole half down
  /// between them, and both doors have taken several paths all along — it was the side handing them
  /// over that passed one at a time.
  it("stages every row of the set in one call when the box on one of them is pressed", async () => {
    hoisted.git[ROOT] = four();
    await draw();
    await clickWith(rowOf(t("git.changes"), "a.rs"), {});
    await clickWith(rowOf(t("git.changes"), "c.rs"), { metaKey: true, ctrlKey: true });

    await press(box(t("git.changes"), "a.rs"));
    expect(hoisted.staged).toEqual([[["a.rs"], ["c.rs"]]]);
  });

  /// The rule the menu is read by, read the same way by the box: a press away from what is picked
  /// is a press about the row under it.
  it("stages the one row whose box was pressed where it is not in the set", async () => {
    hoisted.git[ROOT] = four();
    await draw();
    await clickWith(rowOf(t("git.changes"), "a.rs"), {});
    await clickWith(rowOf(t("git.changes"), "b.rs"), { metaKey: true, ctrlKey: true });

    await press(box(t("git.changes"), "d.rs"));
    expect(hoisted.staged).toEqual([[["d.rs"]]]);
  });

  /// The box is not a tab stop — the row is — so a reader working the list by keyboard would
  /// otherwise gather rows with no way to stage them.
  it("presses the box of the row the keyboard is on when Space is pressed", async () => {
    hoisted.git[ROOT] = four();
    await draw();
    await clickWith(rowOf(t("git.changes"), "b.rs"), {});
    await keyOn(rowOf(t("git.changes"), "b.rs"), "ArrowDown", { shiftKey: true });
    await keyOn(rowOf(t("git.changes"), "c.rs"), " ");
    expect(hoisted.staged).toEqual([[["b.rs"], ["c.rs"]]]);
  });

  /// Staging a conflict is the reader saying the merge is settled there, which is why those rows
  /// carry no box at all — and Space over them is that same press.
  it("leaves a conflicted row alone when Space is pressed on it", async () => {
    hoisted.git[ROOT] = says({
      rows: [row({ path: ["both.rs"], index: "U", worktree: "U" })],
    });
    await draw();
    await clickWith(rowOf(t("git.conflicts"), "both.rs"), {});
    await keyOn(rowOf(t("git.conflicts"), "both.rs"), " ");
    expect(hoisted.staged).toEqual([]);
  });

  /// The box on the line the list is named on. What it takes is the list and not the set — it is
  /// the one press for "all of this", which is what a reader reaches for when the set would be
  /// every row anyway.
  it("takes the whole list in one call from the box on its own line", async () => {
    hoisted.git[ROOT] = says({
      rows: [
        row({ path: ["a.rs"], worktree: "M" }),
        row({ path: ["b.rs"], worktree: "M" }),
        row({ path: ["kept.rs"], index: "M" }),
      ],
    });
    await draw();
    // Something picked out, to be sure this press is about the list and not about the set.
    await clickWith(rowOf(t("git.changes"), "b.rs"), {});
    await press(allBox(t("git.changes"))!);
    expect(hoisted.staged).toEqual([[["a.rs"], ["b.rs"]]]);

    await press(allBox(t("git.staged"))!);
    expect(hoisted.unstaged).toEqual([[["kept.rs"]]]);
  });

  /// A list with nothing in it has nothing to take, and a box over it would be one that answers
  /// every press with git's own refusal.
  it("draws no box on the line of a list with nothing in it", async () => {
    hoisted.git[ROOT] = says({ rows: [row({ path: ["a.rs"], worktree: "M" })] });
    await draw();
    expect(allBox(t("git.changes"))).toBeDefined();
    expect(allBox(t("git.staged"))).toBeUndefined();
  });
});

/// A row carried from one of git's lists to the other, which is the gesture the tree's rows are
/// already taken up by (`AMB-D-775`). What has to be right is that the whole set travels, that a
/// drop on the list the rows are already in asks git nothing, and that a conflict is not carried at
/// all — staging one is the reader declaring the merge settled (`AMB-D-906`, 2-7).
describe("the rail's git half, with a row in hand", () => {
  /** The section a drop would land in, by the name over it. */
  const listOf = (under: string): HTMLElement =>
    sectionOf(under) as HTMLElement;

  /** Press a row the way a hand takes hold of one. */
  async function takeHold(what: Element) {
    await act(async () => {
      what.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true }));
      await new Promise((r) => setTimeout(r, 0));
    });
  }

  /** Let the rows go over one of the lists, the way the gesture reports it. */
  async function letGoOn(list: HTMLElement, held: Held) {
    await act(async () => {
      carriedIntoStage(list, held);
      await new Promise((r) => setTimeout(r, 0));
    });
  }

  const two = (): FolderGitDto => says({
    rows: [
      row({ path: ["a.rs"], worktree: "M" }),
      row({ path: ["b.rs"], worktree: "M" }),
      row({ path: ["kept.rs"], index: "M" }),
    ],
  });

  it("hands the whole set to the gesture when one of its rows is taken hold of", async () => {
    hoisted.git[ROOT] = two();
    const taken: Held[] = [];
    await draw(ROOT, undefined, undefined, { onCarry: (held) => taken.push(held) });
    await act(async () => {
      rowOf(t("git.changes"), "a.rs").dispatchEvent(new MouseEvent("click", { bubbles: true }));
      await new Promise((r) => setTimeout(r, 0));
    });
    await act(async () => {
      rowOf(t("git.changes"), "b.rs")
        .dispatchEvent(new MouseEvent("click", { bubbles: true, metaKey: true, ctrlKey: true }));
      await new Promise((r) => setTimeout(r, 0));
    });

    await takeHold(rowOf(t("git.changes"), "b.rs"));
    expect(taken).toEqual([{
      wholes: [`${ROOT}/a.rs`, `${ROOT}/b.rs`],
      root: ROOT,
      paths: [["a.rs"], ["b.rs"]],
    }]);
  });

  /// The box is an act of its own, and a hand that slipped a few pixels while pressing it would
  /// stage nothing and put the row in the air instead.
  it("does not take a row up when the press was on its box", async () => {
    hoisted.git[ROOT] = two();
    const taken: Held[] = [];
    await draw(ROOT, undefined, undefined, { onCarry: (held) => taken.push(held) });
    await takeHold(box(t("git.changes"), "a.rs"));
    expect(taken).toEqual([]);
  });

  it("stages what was let go on the staged list, and takes back what was let go on the other", async () => {
    hoisted.git[ROOT] = two();
    await draw();
    await letGoOn(listOf(t("git.staged")), {
      wholes: [`${ROOT}/a.rs`, `${ROOT}/b.rs`],
      root: ROOT,
      paths: [["a.rs"], ["b.rs"]],
    });
    expect(hoisted.staged).toEqual([[["a.rs"], ["b.rs"]]]);

    await letGoOn(listOf(t("git.changes")), {
      wholes: [`${ROOT}/kept.rs`],
      root: ROOT,
      paths: [["kept.rs"]],
    });
    expect(hoisted.unstaged).toEqual([[["kept.rs"]]]);
  });

  /// There is no third state for a path to move to, so a drop where the rows already are would ask
  /// git to stage what is staged.
  it("asks git nothing when the rows are let go on the list they came from", async () => {
    hoisted.git[ROOT] = two();
    const taken: Held[] = [];
    await draw(ROOT, undefined, undefined, { onCarry: (held) => taken.push(held) });
    // Taken up from the changes list, which is what makes the drop below a gesture that moved
    // nothing — the half reads where they came from, not where git says they are.
    await takeHold(rowOf(t("git.changes"), "a.rs"));
    await letGoOn(listOf(t("git.changes")), taken[0]!);
    expect(hoisted.staged).toEqual([]);
    expect(hoisted.unstaged).toEqual([]);

    // And the same rows let go on the other list do move.
    await takeHold(rowOf(t("git.changes"), "a.rs"));
    await letGoOn(listOf(t("git.staged")), taken[1]!);
    expect(hoisted.staged).toEqual([[["a.rs"]]]);
  });

  it("marks the list a carried row is over, and unmarks it when the row leaves", async () => {
    hoisted.git[ROOT] = two();
    await draw();
    const list = listOf(t("git.staged"));
    expect(list.getAttribute(STAGE_ATTR)).toBe("staged");

    await act(async () => { carriedOverStage(list); await new Promise((r) => setTimeout(r, 0)); });
    expect(listOf(t("git.staged")).className).toContain("gitpanel__section--over");
    expect(listOf(t("git.changes")).className).not.toContain("gitpanel__section--over");

    await act(async () => { carriedOverStage(null); await new Promise((r) => setTimeout(r, 0)); });
    expect(listOf(t("git.staged")).className).not.toContain("gitpanel__section--over");
  });

  /// Staging a conflict is the one press that says the merge is settled there, so those rows are
  /// not in hand at all.
  it("leaves a conflicted row where it is when the hand presses on it", async () => {
    hoisted.git[ROOT] = says({
      rows: [row({ path: ["both.rs"], index: "U", worktree: "U" })],
    });
    const taken: Held[] = [];
    await draw(ROOT, undefined, undefined, { onCarry: (held) => taken.push(held) });
    await takeHold(rowOf(t("git.conflicts"), "both.rs"));
    expect(taken).toEqual([]);
  });
});

/// The set is read on the other side of the panes as well as acted on here: what the rows are
/// holding is a patch, and a patch wraps (`AMB-D-835`). So what has to be right is that the column
/// is told which paths and which of git's two halves — and that the press that opens it is a second
/// one on the row, the way it is in the tree.
describe("the rail's git half, read across the panes", () => {
  /** Press a row, the way a reader picking one out does. */
  const pressRow = (what: Element) => act(async () => {
    what.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await new Promise((r) => setTimeout(r, 0));
  });

  /** The second press, which is what asks for the patches. */
  const pressAgain = (what: Element) => act(async () => {
    what.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    what.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
    await new Promise((r) => setTimeout(r, 0));
  });

  /** One changed path and one staged one, which is enough to tell the two halves apart. */
  const both = (): FolderGitDto => says({
    rows: [
      row({ path: ["a.rs"], worktree: "M" }),
      row({ path: ["b.rs"], index: "M" }),
    ],
  });

  it("hands up the picked paths, and which of git's two halves they are in", async () => {
    hoisted.git[ROOT] = both();
    const seen: (DiffPick | null)[] = [];
    await draw(ROOT, undefined, undefined, { onPicked: (one) => seen.push(one) });
    // Nothing is picked out until a row is pressed, and that is what the column is told first.
    expect(seen[0]).toBeNull();

    await pressRow(rowOf(t("git.changes"), "a.rs"));
    expect(seen[seen.length - 1]).toEqual({ paths: [["a.rs"]], staged: false });

    // Picking in the other list puts the first set down, and the half travels with the paths.
    await pressRow(rowOf(t("git.staged"), "b.rs"));
    expect(seen[seen.length - 1]).toEqual({ paths: [["b.rs"]], staged: true });
  });

  /// A conflict is neither of the two halves: what this half draws about one is how much of it is
  /// left, and git answers about an unmerged path with a patch of another kind (`AMB-D-906`, 2-7).
  it("hands up nothing for a row of what the merge could not settle", async () => {
    hoisted.git[ROOT] = says({ rows: [row({ path: ["x.rs"], index: "U", worktree: "U" })] });
    const seen: (DiffPick | null)[] = [];
    await draw(ROOT, undefined, undefined, { onPicked: (one) => seen.push(one) });
    await pressRow(rowOf(t("git.conflicts"), "x.rs"));
    expect(seen[seen.length - 1]).toBeNull();
  });

  it("asks for the patches on a second press of the row, and not on the first", async () => {
    hoisted.git[ROOT] = both();
    let asked = 0;
    await draw(ROOT, undefined, undefined, { onDiff: () => { asked += 1; } });

    await pressRow(rowOf(t("git.changes"), "a.rs"));
    expect(asked).toBe(0);

    await pressAgain(rowOf(t("git.changes"), "a.rs"));
    expect(asked).toBe(1);
  });

  /// The box is what the list does to a path. Pressing it twice is staging and unstaging, and a
  /// reader doing that is not asking to read anything.
  it("does not ask for them when the box is pressed twice", async () => {
    hoisted.git[ROOT] = both();
    let asked = 0;
    await draw(ROOT, undefined, undefined, { onDiff: () => { asked += 1; } });
    await pressAgain(box(t("git.changes"), "a.rs"));
    expect(asked).toBe(0);
  });
});
