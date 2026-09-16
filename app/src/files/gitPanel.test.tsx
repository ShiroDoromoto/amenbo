// @vitest-environment jsdom
// The rail's git half: where the branch stands, and what git has to say about the folder under it.
//
// What has to be right here is that the half says git's own answer and no more — the two letters
// decide which of the two lists a path is in, a folder that is no repository is a sentence rather
// than an empty list, and a count that is nothing is not drawn at all.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { FolderChangesDto, FolderGitDto, GitEntryDto } from "../bindings/bindings";
import { t, tf } from "../core/i18n";

const hoisted = vi.hoisted(() => ({
  /** What git answers about each folder, by the folder it is about. */
  git: {} as Record<string, FolderGitDto>,
  /** Every folder the panel asked about, in order. */
  asked: [] as string[],
  /** Everyone listening for the host's word that a folder moved. */
  takers: [] as ((changes: FolderChangesDto) => void)[],
  /** Every call out to the remote, in order — the name of the one that was run. */
  ran: [] as string[],
  /** What the next call out to the remote answers with, and whether it answers by refusing. */
  answer: { text: "", refuse: null as unknown },
  /** Held back while a test wants a call to still be out. Let go with `hoisted.let()`. */
  held: null as null | (() => void),
  let: () => {},
}));

vi.mock("./folder", () => {
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
      return hoisted.git[root] ?? { prefix: "", branch: null, rows: [] };
    },
    onFolderChanged: async (take: (changes: FolderChangesDto) => void) => {
      hoisted.takers.push(take);
      return () => { hoisted.takers = hoisted.takers.filter((one) => one !== take); };
    },
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
  ...about,
});

async function draw(at: string | null = ROOT) {
  await act(async () => {
    root.render(createElement(GitPanel, { projectId: 1, root: at }));
  });
  await act(async () => { await new Promise((r) => setTimeout(r, 0)); });
}

/** The rows of one of the two lists, by the name over it. */
const listed = (under: string): string[] => {
  const section = [...container.querySelectorAll(".gitpanel__section")]
    .find((one) => one.querySelector(".gitpanel__head")?.textContent?.startsWith(under));
  return [...(section?.querySelectorAll(".gitpanel__name") ?? [])].map((one) => one.textContent ?? "");
};

/** The three buttons of the remote, in the order they are drawn. */
const net = (): HTMLButtonElement[] =>
  [...container.querySelectorAll<HTMLButtonElement>(".gitpanel__net .btn")];

/** Press one of them, and let what it set off settle. */
async function press(one: HTMLButtonElement): Promise<void> {
  await act(async () => {
    one.click();
    await new Promise((r) => setTimeout(r, 0));
  });
}

beforeEach(() => {
  hoisted.git = {};
  hoisted.asked = [];
  hoisted.takers = [];
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
    hoisted.git[ROOT] = { prefix: "", branch: null, rows: [] };
    await draw();
    expect(container.textContent).toContain(t("git.noRepo"));
    expect(container.querySelector(".gitpanel__section")).toBeNull();
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

  /// The face has not been told which folder it is on yet. Nothing is asked and nothing is said:
  /// there is no folder here for a sentence to be about.
  it("asks nothing where there is no folder to ask about", async () => {
    await draw(null);
    expect(hoisted.asked).toEqual([]);
    expect(container.textContent).toBe("");
  });

  /// The window runs the three itself (`AMB-T-4900`), and the one that sends carries the count of
  /// what it would send.
  it("draws the three that go out to the remote, with what push would send on it", async () => {
    hoisted.git[ROOT] = says({
      branch: { name: "main", upstream: "origin/main", ahead: 2, behind: 0 },
    });
    await draw();
    expect(net().map((one) => one.textContent))
      .toEqual([t("git.fetch"), t("git.pull"), `${t("git.push")} ↑2`]);
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
