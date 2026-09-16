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
) {
  await act(async () => {
    root.render(createElement(GitPanel, { projectId: 1, root: at, onHistory, onRead }));
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
