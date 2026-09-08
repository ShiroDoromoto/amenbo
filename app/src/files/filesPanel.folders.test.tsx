// @vitest-environment jsdom
// A project bound to several folders, which is where the panel stops having one root.
//
// Every section is watched, coloured, read and worked on its own — so what has to be right here is
// that a word about one folder moves nothing in another, and that a row is acted on in the folder
// its section was drawn for.
import { act } from "react";
import { describe, expect, it } from "vitest";
import {
  aFile, button, click, clickWith, container, draw, drawOpen, hoisted, holdRefusal, leave, menuOn,
  namebox, openFile, pickedIn, press, pressOn, ROOT, rowFor, rowIn, settle, tell, type,
} from "./filesPanelKit";
import { type CmdError, errLabel, t } from "../core/i18n";

describe("a project bound to several folders", () => {
  const OTHER = "/work/plugins";
  const both = () => { hoisted.bound = [{ path: ROOT, exists: true }, { path: OTHER, exists: true }]; };

  /** Both sections drawn with their trees unfolded — every row of both is then on the screen. Each
   *  section stands unfolded on its own, so this is the drawing and the settling after it. */
  async function openBothTrees() {
    await draw();
    await settle();
  }

  /** One folder's section, found by the heading over it — the sections are ordered by path, and a
   *  test that counted on that would be about the ordering rather than about what it says it is. */
  const folderNamed = (label: string) =>
    [...container.querySelectorAll(".files__folder")].find(
      (one) => one.querySelector(".files__foldername")?.textContent === label,
    )!;

  it("watches every one of them, not the first", async () => {
    both();
    await draw();
    // The first was never chosen — it was whichever sorted first — and the rest of the project was
    // invisible because of it (`AMB-D-778`).
    expect(hoisted.asked).toContain(`watch:1:${ROOT}`);
    expect(hoisted.asked).toContain(`watch:1:${OTHER}`);
  });

  it("names each one, and names none where there is only one to name", async () => {
    both();
    await draw();
    const headings = [...container.querySelectorAll(".files__foldername")];
    expect(headings.map((one) => one.textContent)).toEqual(["plugins", "repo"]);

    hoisted.bound = [{ path: ROOT, exists: true }];
    await draw();
    // One folder is drawn the way it always was: a heading over the only thing on the screen names
    // nothing the reader could confuse it with.
    expect(container.querySelectorAll(".files__foldername")).toHaveLength(0);
  });

  it("asks git about each folder on its own, and colours each one by its own answer", async () => {
    both();
    // One is a repository with something changed in it; the other answers with nothing, which is
    // what a folder that is no repository answers — and it is not the first one's business.
    hoisted.git = { [OTHER]: [{ path: ["a.md"], index: " ", worktree: "M", isDir: false }] };
    await openBothTrees();
    expect(hoisted.asked).toContain(`git:${ROOT}`);
    expect(hoisted.asked).toContain(`git:${OTHER}`);
    expect(folderNamed("repo").querySelector(".files__file--git")).toBeNull();
    expect(folderNamed("plugins").querySelector(".files__file--git-modified")?.textContent)
      .toContain("a.md");
  });

  it("reads a file out of the folder its row was drawn in", async () => {
    both();
    hoisted.file = aFile({ text: "hello" });
    await openBothTrees();
    await openFile(rowIn(folderNamed("plugins"), "a.md"));
    await settle();
    // The same path names a different file in each folder, so which folder the row was in has to
    // travel with it.
    expect(hoisted.asked).toContain(`read:${OTHER}:a.md`);
  });

  it("holds one selection for the panel, in whichever folder it was last made in", async () => {
    both();
    await openBothTrees();
    await clickWith(rowIn(folderNamed("repo"), "a.md"), { ctrlKey: true });
    expect(pickedIn(folderNamed("repo"))).toEqual(["a.md"]);

    // The sections are how several bound folders are drawn, not several selections to hold at once
    // (`AMB-D-778`): rows picked in one folder are put down when rows are picked in another, so
    // what is picked is always one folder's paths — which is all an act on them can be about.
    await clickWith(rowIn(folderNamed("plugins"), "a.md"), { ctrlKey: true });
    expect(pickedIn(folderNamed("plugins"))).toEqual(["a.md"]);
    expect(pickedIn(folderNamed("repo"))).toEqual([]);
  });

  it("keeps a folder that has gone, and says that is what happened", async () => {
    hoisted.bound = [{ path: ROOT, exists: true }, { path: OTHER, exists: false }];
    await draw();
    // Dropped from the list it would look like a binding nobody ever made, and a reader would have
    // no way to tell a folder that moved from one they unbound themselves.
    expect(container.textContent).toContain(t("files.folderGone"));
    expect(hoisted.asked).not.toContain(`watch:1:${OTHER}`);
  });

  /** Every section draws a row for its own root, and the trees under two of them can hold the same
   *  names. A landing that said only the path would light a row up in both. */
  it("marks a drop's landing in the section the pointer is in, and in no other", async () => {
    both();
    hoisted.entries[""] = [{ name: "src", isDir: true, ignored: false }];
    await openBothTrees();
    const trees = [...container.querySelectorAll(".files__folder")];
    expect(trees).toHaveLength(2);

    let step = 0;
    const over = (el: Element | null | undefined) => act(async () => {
      (document as unknown as { elementFromPoint: () => Element | null }).elementFromPoint =
        () => el ?? null;
      step += 1;
      hoisted.dragging?.({ payload: { type: "over", position: { x: step, y: 1 } } });
      await new Promise((r) => setTimeout(r, 0));
    });

    await over(rowIn(trees[1]!, "src"));
    // The row lit up is in the second folder, and the first folder's `src` is left alone.
    expect(trees[1]!.querySelectorAll(".files__into")).toHaveLength(1);
    expect(trees[0]!.querySelectorAll(".files__into")).toHaveLength(0);
  });

  // ── naming ──────────────────────────────────────────────────────────────────────────────────
  // Making a name and writing over one are the two doors `crate::folder_write` opens that have a
  // reader on this side. Both are typed into the row itself rather than into a dialog: what a person
  // is naming sits in a list, and the names already in it are what they are choosing against.

  it("makes a name in the folder that was pointed at, and looks at the folder again", async () => {
    hoisted.entries = { "": [] };
    await draw();
    // The heading is the folder's own row, and the only one there is to point at when the tree is
    // empty — which is exactly when a person wants to put something in it.
    await menuOn(button(t("files.tree")));
    // A folder is not something to hand to an application, so it is offered none of that.
    expect(button(t("files.openWith"))).toBeUndefined();

    await click(button(t("files.newFile")));
    await settle();
    const box = namebox();
    expect(box).not.toBeNull();

    await type(box!, "notes.md");
    await press(box!, "Enter");
    expect(hoisted.asked).toContain(`make:${ROOT}:notes.md:file`);
    // The names are read again rather than left to the watch, which says the same thing a debounce
    // later: a row somebody has just made belongs on the list before they look for it.
    expect(hoisted.asked.filter((one) => one === `entries:${ROOT}:`)).toHaveLength(2);
    expect(namebox()).toBeNull();
  });

  it("makes the name inside the folder that was pointed at, unfolding it to be typed in", async () => {
    hoisted.entries = {
      "": [
        { name: "src", isDir: true, ignored: false },
        { name: "README.md", isDir: false, ignored: false },
      ],
      src: [],
    };
    await drawOpen();

    await menuOn(button("src"));
    await click(button(t("files.newFolder")));
    await settle();
    // `src` was folded shut a moment ago. A box typed into a folder nobody can see would be a name
    // made somewhere the reader never looked.
    expect(hoisted.asked).toContain(`entries:${ROOT}:src`);
    await type(namebox()!, "deep");
    await press(namebox()!, "Enter");
    expect(hoisted.asked).toContain(`make:${ROOT}:src/deep:dir`);

    // A file is not something a name can be made in, so its menu offers none of it — what it is
    // offered is the hand-over doors and its own name.
    await menuOn(button("README.md"));
    expect(button(t("files.newFile"))).toBeUndefined();
    expect(button(t("files.rename"))).toBeDefined();
    expect(button(t("files.openWith"))).toBeDefined();
  });

  it("writes over the name of the row that was pointed at, starting from what it says", async () => {
    hoisted.entries = { "": [{ name: "notes.md", isDir: false, ignored: false }] };
    await drawOpen();

    await menuOn(button("notes.md"));
    await click(button(t("files.rename")));
    await settle();
    // The box opens on the name it is about, so changing one letter is one letter of typing —
    // which is the whole of what a case-only rename is (`crate::folder_write`).
    expect(namebox()?.value).toBe("notes.md");

    await type(namebox()!, "Notes.md");
    await press(namebox()!, "Enter");
    expect(hoisted.asked).toContain(`rename:${ROOT}:notes.md:Notes.md`);
  });

  /** Which names a machine will hold is the one thing a reader cannot work out for themselves, so
   *  the refusal is drawn where they are still typing — and drawn from the dictionary, because the
   *  sentence the command carries is English whoever is reading it (`AMB-D-413`). */
  it("says why a name was refused, and leaves what was typed where it was", async () => {
    const refusal: CmdError = {
      code: "folder_taken",
      message_en: "notes.md is already there",
      fields: { name: "notes.md" },
    };
    hoisted.entries = { "": [] };
    hoisted.refuse = refusal;
    // The answer is held back, which is the whole of what this is about: a browser blurs the box
    // the instant anything about it changes under the reader's fingers, and that lands while the
    // refusal is still on its way.
    const refusalArrives = holdRefusal();
    await draw();
    await menuOn(button(t("files.tree")));
    await click(button(t("files.newFile")));
    await settle();

    await type(namebox()!, "notes.md");
    const box = namebox()!;
    await press(box, "Enter");
    // Left while the answer is out. A box that closed itself here would take the refusal with it,
    // and the reader would watch the name they typed vanish with nothing said.
    await leave(box);
    await refusalArrives();
    expect(container.textContent).toContain(errLabel(refusal));
    // And said inside the box's own row, which is what carries it to a reader being read to. A tree
    // names what may be inside it, so a line drawn anywhere else is thrown out of what the machine
    // hands that reader: the words are on the screen and nowhere in the answer, which is a refusal
    // only somebody looking at the screen is told about (`AMB-T-4403`, the shape of `AMB-T-4396`).
    expect(namebox()!.closest("[role=\"treeitem\"]")?.textContent).toContain(errLabel(refusal));
    // Still there, still holding what was typed: a refusal a person has to type their way back to
    // is one they were told nothing by. And the name was asked for once, not once per leaving.
    expect(namebox()?.value).toBe("notes.md");
    expect(hoisted.asked.filter((one) => one.startsWith("make:"))).toHaveLength(1);

    // Leaving it now is giving up on a name the machine has already answered about — not asking the
    // same question again.
    await leave(namebox()!);
    expect(namebox()).toBeNull();
    expect(hoisted.asked.filter((one) => one.startsWith("make:"))).toHaveLength(1);
    // And the sentence is the dictionary's rather than the command's, which is what makes it read
    // in a language core holds no prose for.
    expect(errLabel(refusal, "ja")).not.toContain("already");
  });

  it("offers no rename for the bound folder, and asks for nothing when the box is escaped", async () => {
    hoisted.entries = { "": [] };
    await draw();
    await menuOn(button(t("files.tree")));
    // The section's own root is the binding, and where a binding is changed is the project's
    // settings — not a row in the tree.
    expect(button(t("files.rename"))).toBeUndefined();

    await click(button(t("files.newFolder")));
    await settle();
    await type(namebox()!, "notes");
    await press(namebox()!, "Escape");
    expect(namebox()).toBeNull();
    expect(hoisted.asked.some((one) => one.startsWith("make:"))).toBe(false);
  });

  // ── the machine's own copy and paste ─────────────────────────────────────────────────────────
  // What `⌘C` means here is what it means everywhere else on the machine, so both ends of it are the
  // host's — the webview cannot carry a file (`AMB-D-796`). What this side decides is which row a
  // copy is about and which folder a paste lands in.

  it("copies the row the keyboard is standing on", async () => {
    hoisted.entries = {
      "": [
        { name: "src", isDir: true, ignored: false },
        { name: "README.md", isDir: false, ignored: false },
      ],
    };
    await drawOpen();
    await pressOn(rowFor("README.md"), "c");
    expect(hoisted.asked).toContain(`clip-copy:${ROOT}:README.md`);
  });

  /** The keys are how a reader who knows them copies a path; the menu is where everybody else
   *  looks. Both reach the same copy, so what is on the clipboard afterwards cannot depend on which
   *  of the two the reader used (`AMB-D-832`). */
  it("copies the row the menu was opened on, the way the key does", async () => {
    hoisted.entries = {
      "": [
        { name: "notes", isDir: true, ignored: false },
        { name: "README.md", isDir: false, ignored: false },
      ],
    };
    await drawOpen();
    await menuOn(button("README.md"));
    await click(button(t("files.copyPath")));
    expect(hoisted.asked).toContain(`clip-copy:${ROOT}:README.md`);

    // One word over a folder as well: what is copied is the row, and a second wording would be
    // saying the two were different doors.
    await menuOn(button("notes"));
    await click(button(t("files.copyPath")));
    expect(hoisted.asked).toContain(`clip-copy:${ROOT}:notes`);
  });

  /** The clipboard is the machine's, not the pane's, so the item stands whether or not there is a
   *  pane beside the panel — which is also why it sits above the hand-over rather than below it. */
  it("offers the copy where there is no pane to hand a path to", async () => {
    await drawOpen();
    await menuOn(button("a.md"));
    expect(button(t("files.pasteFilePath"))).toBeUndefined();
    await click(button(t("files.copyPath")));
    expect(hoisted.asked).toContain(`clip-copy:${ROOT}:a.md`);
  });

  /** The same rule a drop's landing follows: a file's row belongs to the folder holding it, so
   *  pasting on a name means the same as pasting beside it. */
  it("pastes into the folder the row is in, and into the folder the row is", async () => {
    hoisted.entries = {
      "": [
        { name: "src", isDir: true, ignored: false },
        { name: "README.md", isDir: false, ignored: false },
      ],
      src: [{ name: "main.rs", isDir: false, ignored: false }],
    };
    await drawOpen();

    // A file at the top of the tree lands in the folder itself, which is named by nothing.
    await pressOn(rowFor("README.md"), "v");
    expect(hoisted.asked).toContain(`clip-paste:${ROOT}:`);

    await click(button("src"));
    await settle();
    await pressOn(rowFor("main.rs"), "v");
    expect(hoisted.asked).toContain(`clip-paste:${ROOT}:src`);
  });

  /** A file being read is drawn in an editor, and `⌘C` there is the editor's — it copies the words
   *  somebody selected. Taking the key on the panel would take it from them. */
  it("leaves copy and paste alone when the keyboard is not on a row", async () => {
    hoisted.file = aFile({ text: "echo hi" });
    hoisted.entries = { "": [{ name: "run.sh", isDir: false, ignored: false }] };
    await drawOpen();
    await openFile(button("run.sh"));
    await settle();

    await pressOn(container.querySelector(".files--reading"), "c");
    await pressOn(container.querySelector(".files--reading"), "v");
    expect(hoisted.asked.some((one) => one.startsWith("clip-"))).toBe(false);
  });

  it("says so when a folder goes while it is being looked at", async () => {
    both();
    await draw();
    expect(container.textContent).not.toContain(t("files.folderGone"));
    await act(async () => {
      tell({ root: OTHER, capped: false, unwatched: false, gone: true });
      await new Promise((r) => setTimeout(r, 0));
    });
    expect(container.textContent).toContain(t("files.folderGone"));
  });
});
