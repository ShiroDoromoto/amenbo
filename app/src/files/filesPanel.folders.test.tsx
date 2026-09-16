// @vitest-environment jsdom
// A project bound to several folders, which is where the panel stops having one root.
//
// One of them is drawn, and a reader says which (`AMB-D-905`). So what has to be right here is that
// the window is on one folder at a time — watched, coloured, read and worked as that one — and that
// going to another takes the whole of it along: the watch, git's answer, and what a row acts on.
import { act } from "react";
import { describe, expect, it } from "vitest";
import {
  aFile, button, click, clickWith, container, draw, drawOpen, goRoot, hoisted, holdRefusal, leave,
  menuOn, namebox, openFile, pickedIn, press, pressOn, ROOT, rowFor, settle, tell, type,
} from "./filesPanelKit";
import { type CmdError, errLabel, t } from "../core/i18n";

describe("a project bound to several folders", () => {
  const OTHER = "/work/plugins";
  const both = () => { hoisted.bound = [{ path: ROOT, exists: true }, { path: OTHER, exists: true }]; };

  /** The picker's list, as the names it offers — the folders are ordered by path (`./sections`). */
  const offered = () =>
    [...container.querySelectorAll<HTMLElement>(".menu__item")].map((one) => one.textContent);

  /** Whether the list says this folder has something changed in it. */
  const dotted = (label: string) =>
    [...container.querySelectorAll<HTMLElement>(".menu__item")]
      .find((one) => one.textContent === label)
      ?.querySelector("[data-icon=\"dot\"]") !== null;

  it("draws one folder and watches that one, and takes the watch along to the next", async () => {
    both();
    await draw();
    // The first by path, which is where a reader who has picked nothing starts.
    expect(container.querySelectorAll(".files__folder")).toHaveLength(1);
    expect(hoisted.asked).toContain(`watch:1:${OTHER}`);
    expect(hoisted.asked).not.toContain(`watch:1:${ROOT}`);

    await goRoot("repo");
    // The folder left goes back to being unwatched. Watching all six of Amenbo's own was what this
    // was costing, and one window answering for one repository is what buys it back (`AMB-D-905`).
    expect(hoisted.asked).toContain(`watch:1:${ROOT}`);
    expect(hoisted.asked).toContain(`unwatch:${OTHER}`);
    expect(container.querySelectorAll(".files__folder")).toHaveLength(1);
  });

  it("names the folders apart on the list, and offers no list where there is one folder", async () => {
    both();
    await draw();
    await click(container.querySelector(".rootpick__on"));
    expect(offered()).toEqual(["plugins", "repo"]);

    hoisted.bound = [{ path: ROOT, exists: true }];
    await draw();
    // A project bound to one folder is drawn the way it always was: a control that can only ever
    // say the same thing is a press nobody needs.
    expect(container.querySelector(".rootpick__on")).toBeNull();
  });

  it("says on the list which folders have something changed in them", async () => {
    both();
    // What the tree can no longer say by colouring six trees at once (`AMB-D-785`), said coarsely:
    // something in here, or nothing.
    hoisted.git = { [ROOT]: [{ path: ["a.md"], index: " ", worktree: "M", isDir: false }] };
    await draw();
    // Not before the list is opened. Asking every folder on every draw is the cost the one-folder
    // window was taken to stop paying.
    expect(hoisted.asked).not.toContain(`git:${ROOT}`);

    await click(container.querySelector(".rootpick__on"));
    await settle();
    expect(hoisted.asked).toContain(`git:${ROOT}`);
    expect(dotted("repo")).toBe(true);
    expect(dotted("plugins")).toBe(false);
  });

  it("asks git about the folder it draws, and colours that one by its answer", async () => {
    both();
    hoisted.git = { [ROOT]: [{ path: ["a.md"], index: " ", worktree: "M", isDir: false }] };
    await draw();
    // The folder drawn is `plugins`, which answers with nothing — which is what a folder that is no
    // repository answers, and not the other folder's business.
    expect(container.querySelector(".files__file--git")).toBeNull();

    await goRoot("repo");
    expect(hoisted.asked).toContain(`git:${ROOT}`);
    expect(container.querySelector(".files__file--git-modified")?.textContent).toContain("a.md");
  });

  it("reads a file out of the folder the window is on", async () => {
    both();
    hoisted.file = aFile({ text: "hello" });
    await draw();
    await goRoot("repo");
    await openFile(button("a.md"));
    await settle();
    // The same path names a different file in each folder, so which folder the row was in has to
    // travel with it.
    expect(hoisted.asked).toContain(`read:${ROOT}:a.md`);
    expect(hoisted.asked).not.toContain(`read:${OTHER}:a.md`);
  });

  it("puts down rows picked in a folder the reader has gone away from", async () => {
    both();
    await draw();
    await clickWith(button("a.md"), { ctrlKey: true });
    expect(pickedIn(container)).toEqual(["a.md"]);

    // One selection for the rail, wherever it was made: what is done with picked rows is done to
    // one folder's paths, so rows left standing in a folder nobody is looking at would come back
    // under a reader who has since picked others.
    await goRoot("repo");
    await clickWith(button("a.md"), { ctrlKey: true });
    await goRoot("plugins");
    expect(pickedIn(container)).toEqual([]);
  });

  it("opens on a folder that is there, and keeps the one that has gone on the list", async () => {
    // `plugins` sorts first and is the one that went. Opening on it would say the project has
    // nothing while the folder beside it sits there — which is the reading the stacked rail was
    // taken down for in the first place.
    hoisted.bound = [{ path: ROOT, exists: true }, { path: OTHER, exists: false }];
    await draw();
    expect(container.textContent).not.toContain(t("files.folderGone"));
    expect(hoisted.asked).toContain(`watch:1:${ROOT}`);
    expect(hoisted.asked).not.toContain(`watch:1:${OTHER}`);

    // Dropped from the list it would look like a binding nobody ever made, and a reader would have
    // no way to tell a folder that moved from one they unbound themselves.
    await click(container.querySelector(".rootpick__on"));
    expect(offered()).toEqual(["plugins", "repo"]);

    await goRoot("plugins");
    expect(container.textContent).toContain(t("files.folderGone"));
    expect(hoisted.asked).not.toContain(`watch:1:${OTHER}`);
  });

  /** The same path inside two bound folders is two places, so a landing says which folder as well
   *  as which path — and the folder it says is the one the window is on. */
  it("carries a drop into the folder the window is on", async () => {
    both();
    hoisted.entries[""] = [{ name: "src", isDir: true, ignored: false }];
    await draw();
    await goRoot("repo");

    const at = (el: Element | null | undefined, how: "over" | "drop") => act(async () => {
      (document as unknown as { elementFromPoint: () => Element | null }).elementFromPoint =
        () => el ?? null;
      hoisted.dragging?.({
        payload: { type: how, position: { x: 1, y: 1 }, paths: ["/Users/someone/Desktop/note.md"] },
      });
      await new Promise((r) => setTimeout(r, 0));
    });

    await at(button("src"), "over");
    expect(container.querySelectorAll(".files__into")).toHaveLength(1);

    await at(button("src"), "drop");
    expect(hoisted.imported[0]?.toRoot).toBe(ROOT);
    expect(hoisted.imported[0]?.to).toEqual(["src"]);
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
