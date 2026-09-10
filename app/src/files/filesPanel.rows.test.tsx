// @vitest-environment jsdom
// The rows of the tree as a reader works them: the box a name is typed into, the keyboard walking
// them, the several that can be picked out at once, and what the doors do to what is picked out.
//
// The whole tree carries one stop in the tab order rather than one per row, and only the rows in
// view are drawn — so where the keyboard stands, and how it gets back to a row that was left out of
// the document, is what has to be right here.
import { act } from "react";
import { describe, expect, it } from "vitest";
import {
  anyButton, button, click, clickWith, container, draw, drawOpen, hoisted, labelOf, last, menuOn,
  namebox, onMac, openFile, pickedIn, press, pressOn, type Props, ROOT, rowFor, settle, tell,
  undo, undoOn,
} from "./filesPanelKit";
import { t, tf, tn } from "../core/i18n";

describe("the file face", () => {
  // ── walking the tree with the keys ──────────────────────────────────────────────────────────
  // Amenbo invents no key of its own; what is here is the tree pattern's own vocabulary and the
  // machine's own undo, which is the whole of what a face carrying a terminal may take (`AMB-D-780`).

  const rows = () => [...container.querySelectorAll<HTMLElement>("[role=\"treeitem\"]")];
  const at = () => labelOf(document.activeElement as HTMLElement);

  /** A tree of one folder with a file in it, and one file beside it, unfolded and stood on. */
  async function stood() {
    hoisted.entries = {
      "": [{ name: "src", isDir: true, ignored: false }, { name: "a.md", isDir: false, ignored: false }],
      src: [{ name: "main.rs", isDir: false, ignored: false }],
    };
    await drawOpen();
    rows()[0]!.focus();
  }

  // Nearly every rename keeps the extension, so a box that hands back the whole name selected takes
  // it away on the first keystroke.
  it("stands on the stem of the name it is renaming, and on the whole of one that is all stem", async () => {
    hoisted.entries = {
      "": [
        { name: "notes.md", isDir: false, ignored: false },
        { name: "Makefile", isDir: false, ignored: false },
        { name: ".gitignore", isDir: false, ignored: false },
        { name: "archive.tar.gz", isDir: false, ignored: false },
      ],
    };
    const selected = async (name: string) => {
      await press(rows().find((one) => labelOf(one) === name)!, "F2");
      await settle();
      const box = namebox()!;
      const held = box.value.slice(box.selectionStart ?? 0, box.selectionEnd ?? 0);
      await press(box, "Escape");
      await settle();
      return held;
    };
    await drawOpen();

    expect(await selected("notes.md")).toBe("notes");
    // The last dot, not the first: renaming this one is changing `archive.tar`.
    expect(await selected("archive.tar.gz")).toBe("archive.tar");
    // No dot at all, and a dot at the very front — a name, not an extension on an empty stem.
    expect(await selected("Makefile")).toBe("Makefile");
    expect(await selected(".gitignore")).toBe(".gitignore");
  });

  it("begins a rename on the row the keyboard is standing on, with F2", async () => {
    await stood();
    expect(at()).toBe("src");

    await press(rows()[0]!, "F2");
    await settle();
    // The row became the box, holding the name it had — a rename starts from what is there.
    expect(namebox()?.value).toBe("src");
  });

  // The box stands among the rows, so it has to be one of them: a tree names what may be inside it,
  // and a line drawn as anything else is thrown away before it reaches a reader who is being read
  // to — the box is on the screen and nowhere in what the machine says is there (`AMB-T-4396`).
  it("draws the box a name is typed into as a row of the tree, from both doors", async () => {
    await stood();

    await press(rows()[0]!, "F2");
    await settle();
    expect(namebox()!.closest("[role=\"treeitem\"]")).not.toBeNull();
    await press(namebox()!, "Escape");
    await settle();

    await menuOn(rowFor("src"));
    await click(button(t("files.newFile")));
    await settle();
    expect(namebox()!.closest("[role=\"treeitem\"]")).not.toBeNull();
  });

  // The pattern every tree is walked by once it is longer than the screen. Read off the row's own
  // path and not its drawn text: an open folder's row holds the text of everything under it.
  it("jumps to the next row starting with the letter that was typed, and round again", async () => {
    hoisted.entries = {
      "": [
        { name: "src", isDir: true, ignored: false },
        { name: "a.md", isDir: false, ignored: false },
        { name: "styles.css", isDir: false, ignored: false },
      ],
      src: [{ name: "main.rs", isDir: false, ignored: false }],
    };
    await drawOpen();
    rows()[0]!.focus();
    expect(at()).toBe("src");

    // Forward from where the reader is standing, so the row they are on is not the answer.
    await press(document.activeElement!, "s");
    expect(at()).toBe("styles.css");

    // And on round the end, back to the other one — pressing the letter again goes on rather than
    // sticking on the first match.
    await press(document.activeElement!, "s");
    expect(at()).toBe("src");

    // A letter nothing starts with leaves the reader where they were.
    await press(document.activeElement!, "z");
    expect(at()).toBe("src");

    await press(document.activeElement!, "a");
    expect(at()).toBe("a.md");
  });

  it("carries one stop in the tab order for the whole tree, not one for every row", async () => {
    await stood();
    await press(rows()[0]!, "ArrowRight");
    await settle();
    // Three rows drawn, and a reader tabbing past the panel stops once. Nothing caps what a level
    // answers with, so one stop per row is a thousand presses on a folder of a thousand names.
    expect(rows()).toHaveLength(3);
    expect(rows().filter((one) => one.tabIndex === 0)).toHaveLength(1);
  });

  it("walks the rows with the arrows, and opens and shuts a folder with them", async () => {
    await stood();
    expect(at()).toBe("src");

    // Into the folder: the first press opens it, the second steps onto what is inside.
    await press(rows()[0]!, "ArrowRight");
    await settle();
    expect(rows()[0]!.getAttribute("aria-expanded")).toBe("true");
    await press(rows()[0]!, "ArrowRight");
    expect(at()).toBe("main.rs");

    // And out of it: from a row inside, back to the folder holding it; then shut.
    await press(document.activeElement!, "ArrowLeft");
    expect(at()).toBe("src");
    await press(document.activeElement!, "ArrowLeft");
    await settle();
    expect(rows()[0]!.getAttribute("aria-expanded")).toBe("false");

    // Down and up along what is drawn, ends included.
    await press(rows()[0]!, "ArrowDown");
    expect(at()).toBe("a.md");
    await press(document.activeElement!, "ArrowUp");
    expect(at()).toBe("src");
    await press(document.activeElement!, "End");
    expect(at()).toBe("a.md");
    await press(document.activeElement!, "Home");
    expect(at()).toBe("src");
  });

  // Every row of every open folder is a line of one list. Nothing here is about how it looks — it is
  // what lets a row in the middle be left out of the document without the ones below it going with
  // it, which is what drawing only the rows on the screen asks for (`AMB-T-4108`).
  it("draws the open rows as one list and not as a box inside a box", async () => {
    await stood();
    await press(rows()[0]!, "ArrowRight");
    await settle();
    const tree = container.querySelector("[role=\"tree\"]")!;
    expect(container.querySelectorAll("[role=\"tree\"], [role=\"group\"]")).toHaveLength(1);
    // The row inside the folder is the list's own line, not something the folder's row holds.
    expect([...tree.children]).toHaveLength(3);
    const folder = rows()[0]!;
    expect(folder.querySelector("[role=\"treeitem\"]")).toBeNull();
    expect(labelOf(folder)).toBe("src");
    // And the step is the row's own, so that a row drawn on its own still stands where it belongs.
    expect(rows().find((one) => labelOf(one) === "main.rs")!.style.getPropertyValue("--depth"))
      .toBe("1");
    expect(folder.style.getPropertyValue("--depth")).toBe("0");
  });

  // Which row a press moves to is read off the rows in the order they are drawn, and standing on it
  // is what happens after. So the same row can be asked for twice: a reader who steps away and
  // presses the same key again is asking to be taken back where that key goes.
  it("takes the reader back to the row a key names, however they left it", async () => {
    await stood();
    await press(rows()[0]!, "End");
    expect(at()).toBe("a.md");

    // Away from it, the way a pointer takes a reader — the tab stop follows, and the last press is
    // still the one that named the last row.
    await act(async () => { rows()[0]!.focus(); });
    expect(at()).toBe("src");
    await press(document.activeElement!, "End");
    expect(at()).toBe("a.md");
  });

  // ── picking rows out ────────────────────────────────────────────────────────────────────────
  // Which rows an act is about is a set of its own: not the row the keyboard is standing on, and not
  // the file being read (`AMB-T-4229`). The gestures are the ones every file manager already has, so
  // what is tested is that they mean here what they mean there.

  /** A flat folder of four names, drawn and stood on. */
  async function four(props: Partial<Props> = {}) {
    hoisted.entries = {
      "": ["a.md", "b.md", "c.md", "d.md"].map((name) => ({ name, isDir: false, ignored: false })),
    };
    await drawOpen(props);
    rows()[0]!.focus();
  }

  const picked = () => pickedIn(container);

  /** A press with Shift down: the walk reaches rather than steps. */
  const shift = (el: Element, key: string) => act(async () => {
    el.dispatchEvent(new KeyboardEvent("keydown", { key, shiftKey: true, bubbles: true }));
    await new Promise((r) => setTimeout(r, 0));
  });

  it("says on the list that its rows can be picked out several at a time", async () => {
    await four();
    expect(container.querySelector('[role="tree"]')!.getAttribute("aria-multiselectable"))
      .toBe("true");
    // Every row answers and not only the picked ones: a row that said nothing would be read as one
    // that cannot be picked at all.
    expect(rows().every((one) => one.hasAttribute("aria-selected"))).toBe(true);
  });

  it("picks the one row a plain press is on, and reads nothing", async () => {
    await four();
    await click(rowFor("b.md"));
    await settle();
    expect(picked()).toEqual(["b.md"]);
    expect(hoisted.asked).not.toContain(`read:${ROOT}:b.md`);
  });

  // Reading one file while another stays open is what the tabs are for: the second one used to take
  // the first one's place, so following a reference cost a reader the file they followed it from
  // (`AMB-D-835`).
  it("reads the row on the second press, which the first one only picked out", async () => {
    await four();
    await openFile(rowFor("b.md"));
    await settle();
    expect(picked()).toEqual(["b.md"]);
    expect(hoisted.asked).toContain(`read:${ROOT}:b.md`);
  });

  it("reads nothing where the second press was a reader gathering another row", async () => {
    await four();
    await openFile(rowFor("b.md"), { ctrlKey: true });
    await settle();
    expect(hoisted.asked).not.toContain(`read:${ROOT}:b.md`);
  });

  it("takes a row in and back out with the machine's own key, without opening it", async () => {
    await four();
    await clickWith(rowFor("a.md"), { ctrlKey: true });
    await clickWith(rowFor("c.md"), { ctrlKey: true });
    expect(picked()).toEqual(["a.md", "c.md"]);

    // The same press again is the row leaving, which is what makes it the way to correct a slip.
    await clickWith(rowFor("a.md"), { ctrlKey: true });
    expect(picked()).toEqual(["c.md"]);
    // And nothing was read on the way: a reader gathering rows is not asking to be shown each one.
    expect(hoisted.asked.filter((one) => one.startsWith("read:"))).toEqual([]);
  });

  it("takes a row in with the key the machine has, and not with the one that opens its menu", async () => {
    await four();
    onMac();
    // On a Mac the key is ⌘ — Ctrl and a press is that machine's way of asking for the menu, and a
    // row taken into the selection by it would be one press answering twice.
    await clickWith(rowFor("a.md"), { metaKey: true });
    await clickWith(rowFor("c.md"), { metaKey: true });
    expect(picked()).toEqual(["a.md", "c.md"]);

    await clickWith(rowFor("b.md"), { ctrlKey: true });
    expect(picked()).toEqual(["a.md", "c.md"]);
    // And it is not read as a plain press either: nothing was opened and nothing was put down.
    expect(hoisted.asked.filter((one) => one.startsWith("read:"))).toEqual([]);
  });

  it("reaches from the end the range is measured from to the row Shift was pressed on", async () => {
    await four();
    await clickWith(rowFor("b.md"), { ctrlKey: true });
    await clickWith(rowFor("d.md"), { shiftKey: true });
    expect(picked()).toEqual(["b.md", "c.md", "d.md"]);

    // The end does not move with the range, so reaching the other way is the rows above it and not
    // the ones already picked turned over.
    await clickWith(rowFor("a.md"), { shiftKey: true });
    expect(picked()).toEqual(["a.md", "b.md"]);
  });

  it("grows the range with Shift and the arrows, and shrinks it back", async () => {
    await four();
    // A step without Shift is one row: what was picked is put down, and this is where the next
    // range is measured from.
    await press(rows()[0]!, "ArrowDown");
    expect(picked()).toEqual(["b.md"]);

    await shift(document.activeElement!, "ArrowDown");
    await shift(document.activeElement!, "ArrowDown");
    expect(picked()).toEqual(["b.md", "c.md", "d.md"]);

    await shift(document.activeElement!, "ArrowUp");
    expect(picked()).toEqual(["b.md", "c.md"]);
    // Back past the end it was measured from, which is where a range does turn over.
    await shift(document.activeElement!, "ArrowUp");
    await shift(document.activeElement!, "ArrowUp");
    expect(picked()).toEqual(["a.md", "b.md"]);
  });

  it("reaches both ends of the tree with Shift, the way the steps do", async () => {
    await four();
    await press(rows()[0]!, "ArrowDown");
    await shift(document.activeElement!, "End");
    expect(picked()).toEqual(["b.md", "c.md", "d.md"]);
    await shift(document.activeElement!, "Home");
    expect(picked()).toEqual(["a.md", "b.md"]);
  });

  it("puts the range down when the walk steps without Shift", async () => {
    await four();
    await clickWith(rowFor("a.md"), { ctrlKey: true });
    await clickWith(rowFor("c.md"), { shiftKey: true });
    expect(picked()).toEqual(["a.md", "b.md", "c.md"]);

    // The keyboard followed the press that reached the range, and a step from there is one row.
    await press(document.activeElement!, "ArrowDown");
    expect(picked()).toEqual(["d.md"]);
  });

  it("picks the row a menu was opened away from the selection on", async () => {
    await four();
    await clickWith(rowFor("a.md"), { ctrlKey: true });
    await clickWith(rowFor("b.md"), { ctrlKey: true });

    // A menu standing over one row and acting on others is the thing to keep out (`AMB-T-4230`).
    await menuOn(rowFor("d.md"));
    expect(picked()).toEqual(["d.md"]);
  });

  it("leaves the selection alone for a menu opened inside it", async () => {
    await four();
    await clickWith(rowFor("a.md"), { ctrlKey: true });
    await clickWith(rowFor("b.md"), { ctrlKey: true });

    // Which is the press a reader makes to act on what they gathered.
    await menuOn(rowFor("b.md"));
    expect(picked()).toEqual(["a.md", "b.md"]);
  });

  it("lets go of a row the folder no longer holds", async () => {
    await four();
    await clickWith(rowFor("b.md"), { ctrlKey: true });
    await clickWith(rowFor("c.md"), { ctrlKey: true });
    expect(picked()).toEqual(["b.md", "c.md"]);

    hoisted.entries[""] = ["a.md", "c.md", "d.md"]
      .map((name) => ({ name, isDir: false, ignored: false }));
    await act(async () => {
      tell({ root: ROOT, capped: false, unwatched: false, gone: false });
      await new Promise((r) => setTimeout(r, 0));
    });
    // A row that went under the reader is out of the selection before anything can be asked of it
    // (`AMB-T-4230`).
    expect(picked()).toEqual(["c.md"]);
  });

  it("gives a folded folder's rows back to the reader who folded it shut", async () => {
    await stood();
    await press(rows()[0]!, "ArrowRight");
    await settle();
    await clickWith(rowFor("main.rs"), { ctrlKey: true });
    expect(picked()).toEqual(["main.rs"]);

    // Nothing is drawn to pick while the folder is shut, and the row is not gone — nobody said the
    // folder moved, and what is inside it was only let go of because nobody is looking at it.
    await press(rows()[0]!, "ArrowLeft");
    await settle();
    expect(picked()).toEqual([]);
    await press(rows()[0]!, "ArrowRight");
    await settle();
    expect(picked()).toEqual(["main.rs"]);
  });

  // ── acting on what is picked out ────────────────────────────────────────────────────────────
  // Picking rows out is only worth anything if the doors act on them (`AMB-T-4230`). Which rows a
  // door is about is the same answer everywhere: the ones picked out where the press was aimed at
  // one of them, and the row it was aimed at alone where it was not.

  /** Two of the four rows picked out, the way a reader gathers them one at a time. */
  async function twoPicked() {
    await four();
    await clickWith(rowFor("a.md"), { ctrlKey: true });
    await clickWith(rowFor("c.md"), { ctrlKey: true });
    expect(picked()).toEqual(["a.md", "c.md"]);
  }

  it("copies every row that is picked out, in one press of the key", async () => {
    await twoPicked();
    await pressOn(rowFor("c.md"), "c");
    expect(hoisted.asked).toContain(`clip-copy:${ROOT}:a.md,c.md`);
  });

  it("copies the one row a key was pressed on, where it is not one of the picked", async () => {
    await twoPicked();
    // The press is aimed at a row outside the selection: what a reader means by it is that row.
    await pressOn(rowFor("b.md"), "c");
    expect(hoisted.asked).toContain(`clip-copy:${ROOT}:b.md`);
  });

  /**
   * The band and the keyboard are two answers about a tree, and a press has to move both: the band
   * says which rows are picked out, the keyboard which row `⌘C` and the arrows are standing on.
   *
   * A press that moved only the band left the two on different rows, and what a reader saw was the
   * copy taking the row before the one they had just pressed (`AMB-T-4368`). The presses above hand
   * the key to the row a reader meant, so none of them can catch that — this one asks the document
   * which row the key would actually arrive on.
   */
  it("stands the keyboard on the row a press landed on, so the copy is about it", async () => {
    // The keyboard starts on the first row, which is the row a broken press leaves it on.
    await four();
    await clickWith(rowFor("c.md"), {});
    expect(labelOf(document.activeElement as HTMLElement)).toBe("c.md");

    await pressOn(document.activeElement, "c");
    expect(hoisted.asked).toContain(`clip-copy:${ROOT}:c.md`);
  });

  it("copies every picked row off the menu as well as off the key", async () => {
    await twoPicked();
    await menuOn(rowFor("c.md"));
    await click(button(t("files.copyPath")));
    expect(hoisted.asked).toContain(`clip-copy:${ROOT}:a.md,c.md`);
  });

  it("puts down the selection when the menu is opened away from it", async () => {
    await twoPicked();
    await menuOn(rowFor("b.md"));
    expect(picked()).toEqual(["b.md"]);
    await click(button(t("files.copyPath")));
    expect(hoisted.asked).toContain(`clip-copy:${ROOT}:b.md`);
  });

  it("bins every picked row in one press, and counts them in the question", async () => {
    await twoPicked();
    await menuOn(rowFor("a.md"));
    await click(button(t("files.trash")));
    // Counted rather than named: five names in front of a press already decided on is reading
    // matter, and what a reader checks is how many.
    expect(document.body.textContent).toContain(tn("files.trashAskMany", 2));
    await click(anyButton(t("files.trashGo")));
    // One press however many rows, which is what makes undo put back what the press took away.
    expect(hoisted.asked).toContain(`trash:${ROOT}:a.md,c.md`);
  });

  it("names the one row where a bin is about one row", async () => {
    await four();
    await menuOn(rowFor("b.md"));
    await click(button(t("files.trash")));
    expect(document.body.textContent).toContain(tf("files.trashAsk", { name: "b.md" }));
  });

  it("bins every picked row from the key that means delete", async () => {
    await twoPicked();
    await press(rowFor("c.md")!, "Delete");
    await click(anyButton(t("files.trashGo")));
    expect(hoisted.asked).toContain(`trash:${ROOT}:a.md,c.md`);
  });

  it("opens every picked row with the machine's own answer", async () => {
    await twoPicked();
    await menuOn(rowFor("a.md"));
    await click(button(t("files.openWith")));
    await settle();
    expect(hoisted.asked).toContain(`open:${ROOT}:a.md`);
    expect(hoisted.asked).toContain(`open:${ROOT}:c.md`);
  });

  it("opens every picked row with the one application the reader chose", async () => {
    hoisted.apps = [{ name: "Zed", path: "/Applications/Zed.app", usual: true }];
    await twoPicked();
    await menuOn(rowFor("a.md"));
    await click(button(t("files.chooseApp")));
    // The list is asked about the row the menu was opened on: what a person choosing an application
    // for two files has chosen is one application.
    expect(hoisted.asked).toContain(`ask:${ROOT}:a.md`);
    await click(button(tf("files.appUsual", { name: "Zed" })));
    await settle();
    expect(hoisted.asked).toContain(`with:${ROOT}:a.md:/Applications/Zed.app`);
    expect(hoisted.asked).toContain(`with:${ROOT}:c.md:/Applications/Zed.app`);
  });

  it("shows the picked rows where they live, one press per folder", async () => {
    await stood();
    await press(rows()[0]!, "ArrowRight");
    await settle();
    // Two rows of the bound folder and one inside `src`: three rows, two folders.
    await clickWith(rowFor("main.rs"), { ctrlKey: true });
    await clickWith(rowFor("a.md"), { ctrlKey: true });
    expect(picked()).toEqual(["main.rs", "a.md"]);

    await menuOn(rowFor("a.md"));
    await click(button(t("files.reveal")));
    await settle();
    expect(hoisted.asked).toContain(`reveal:${ROOT}:src/main.rs`);
    expect(hoisted.asked).toContain(`reveal:${ROOT}:a.md`);
  });

  it("asks the file manager once for rows that live in the same folder", async () => {
    await twoPicked();
    await menuOn(rowFor("a.md"));
    await click(button(t("files.reveal")));
    await settle();
    // Showing a row is showing the folder holding it, and a file manager selects one row at a time:
    // two presses on one folder would be the second undoing the first.
    expect(hoisted.asked.filter((one) => one.startsWith("reveal:"))).toEqual([`reveal:${ROOT}:a.md`]);
  });

  it("draws no door that names one thing while several rows are picked out", async () => {
    await four({ onHandOver: () => {} });
    await clickWith(rowFor("a.md"), { ctrlKey: true });
    await menuOn(rowFor("a.md"));
    // One row: naming it is a thing to do, and the pane is offered the file's own wording.
    expect(button(t("files.rename"))).toBeDefined();
    expect(button(t("files.pasteFilePath"))).toBeDefined();

    await click(button(t("files.rename")));
    await clickWith(rowFor("c.md"), { ctrlKey: true });
    await menuOn(rowFor("c.md"));
    // Two: a rename over both would be a press to refuse afterwards.
    expect(button(t("files.rename"))).toBeUndefined();
    // What acts on several is still there.
    expect(button(t("files.copyPath"))).toBeDefined();
    expect(button(t("files.trash"))).toBeDefined();
  });

  it("hands every picked row to the pane, under a word that names no kind", async () => {
    const handed: string[][] = [];
    await four({ onHandOver: (wholes) => handed.push(wholes) });
    await clickWith(rowFor("a.md"), { ctrlKey: true });
    await clickWith(rowFor("c.md"), { ctrlKey: true });
    await menuOn(rowFor("c.md"));
    // The rows gathered can be a folder and four files, and the panel is not told which are which —
    // so the word names no kind (`AMB-T-4242`).
    expect(button(t("files.pasteFilePath"))).toBeUndefined();
    await click(button(t("files.pastePaths")));
    expect(handed).toEqual([[`${ROOT}/a.md`, `${ROOT}/c.md`]]);
  });

  it("carries every picked row when the hand takes hold of one of them", async () => {
    const carried: string[][] = [];
    // The whole paths, which are the half of what is taken hold of that a pane is handed
    // (`./handDrag`).
    await four({ onCarry: (taken) => carried.push(taken.wholes) });
    await clickWith(rowFor("a.md"), { ctrlKey: true });
    await clickWith(rowFor("c.md"), { ctrlKey: true });
    await act(async () => {
      rowFor("c.md")?.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
    });
    expect(last(carried)).toEqual([`${ROOT}/a.md`, `${ROOT}/c.md`]);

    // And a row taken hold of away from the selection is carried on its own — the same answer the
    // menu gives when it is opened there.
    await act(async () => {
      rowFor("b.md")?.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
    });
    expect(last(carried)).toEqual([`${ROOT}/b.md`]);
  });

  it("bins the file being read and not what is picked out behind it", async () => {
    await twoPicked();
    // The file on the screen is one nobody picked out: the reading face is about one file.
    await openFile(rowFor("b.md"));
    await settle();
    // The bin on the reading face, which wears the word as its title rather than on its face.
    await click(container.querySelector(".files__trash"));
    await click(anyButton(t("files.trashGo")));
    expect(hoisted.asked).toContain(`trash:${ROOT}:b.md`);
  });

  // ── only what is in view ────────────────────────────────────────────────────────────────────
  // Nothing caps what a folder answers with, so a tree opened on `node_modules` is thousands of
  // rows. What is drawn is the run in view; the rest is stood in for by its height.

  /** A folder of a hundred names, with the box it is drawn in laid out around it. */
  async function tall(scrolled: number) {
    hoisted.entries[""] = Array.from({ length: 100 }, (_, i) => ({
      name: `f${String(i).padStart(3, "0")}.md`, isDir: false, ignored: false,
    }));
    await drawOpen();
    // jsdom lays nothing out, so the two measurements the window is read off are stated: a box ten
    // rows tall, with the list's top `scrolled` rows above it.
    const box = container.querySelector<HTMLElement>(".files")!;
    const list = container.querySelector<HTMLElement>('[role="tree"]')!;
    Object.defineProperty(box, "clientHeight", { value: 10 * 22, configurable: true });
    box.getBoundingClientRect = () => ({ top: 0 }) as DOMRect;
    list.getBoundingClientRect = () => ({ top: -scrolled * 22 }) as DOMRect;
    await act(async () => {
      box.dispatchEvent(new Event("scroll"));
      await new Promise((r) => setTimeout(r, 0));
    });
    return box;
  }

  const spacers = () =>
    [...container.querySelectorAll<HTMLElement>('[role="tree"] > li[aria-hidden="true"]')]
      .map((one) => one.style.height);

  it("draws the rows in view and leaves the height of the rest behind", async () => {
    await tall(20);
    // The ten rows of the box, and six either side so that a scroll has somewhere to land before
    // the next drawing catches up.
    expect(rows()).toHaveLength(22);
    expect(labelOf(rows()[0]!)).toBe("f014.md");
    expect(labelOf(rows()[21]!)).toBe("f035.md");
    // What a row says about itself is its place in the tree and not among the rows drawn — which
    // is the whole of why the levels were flattened first (`AMB-T-4106`).
    expect(rows()[0]!.getAttribute("aria-posinset")).toBe("15");
    expect(rows()[0]!.getAttribute("aria-setsize")).toBe("100");
    // And the rows that are not drawn are still as tall as they were.
    expect(spacers()).toEqual([`${14 * 22}px`, `${64 * 22}px`]);
    // The tree keeps its one stop, on a row that is in the document.
    expect(rows().filter((one) => one.tabIndex === 0)).toHaveLength(1);
  });

  it("moves the box to a row a key named from outside the window", async () => {
    const box = await tall(20);
    expect(rows().some((one) => labelOf(one) === "f099.md")).toBe(false);

    await press(rows()[0]!, "End");
    // The last row stands 99 rows down the list, which starts 20 rows above the box: the box is
    // moved just far enough to put its foot at the bottom of the box, and the drawing that follows
    // is what the focus lands in.
    expect(box.scrollTop).toBe((99 - 20) * 22 + 22 - 10 * 22);
  });

  it("says how deep a row is, how many it stands among, and which one it is", async () => {
    await stood();
    await press(rows()[0]!, "ArrowRight");
    await settle();
    const inside = rows().find((one) => labelOf(one) === "main.rs")!;
    // The rows are one flat list, so nothing about where a row sits in the document says how far
    // down it is or how many it stands among.
    expect(inside.getAttribute("aria-level")).toBe("2");
    expect(inside.getAttribute("aria-setsize")).toBe("1");
    expect(inside.getAttribute("aria-posinset")).toBe("1");
    expect(rows()[0]!.getAttribute("aria-level")).toBe("1");
    expect(rows()[0]!.getAttribute("aria-setsize")).toBe("2");
  });

  it("opens the file the reader is standing on", async () => {
    await stood();
    await press(rows()[0]!, "ArrowDown");
    await press(document.activeElement!, "Enter");
    await settle();
    expect(hoisted.asked).toContain(`read:${ROOT}:a.md`);
  });

  it("puts the row the reader is standing on to the question about the bin", async () => {
    await stood();
    await press(rows()[0]!, "ArrowDown");
    await press(document.activeElement!, "Delete");
    await settle();
    // The key reaches the same question the menu item does, rather than a second road to the bin.
    expect(document.body.textContent).toContain(tf("files.trashAsk", { name: "a.md" }));
    await click(anyButton(t("files.trashGo")));
    expect(hoisted.asked).toContain(`trash:${ROOT}:a.md`);
  });

  /** The reverse of a path drawn in a pane opening the file here: the row puts the whole path in
   *  front of what is running, which is the only spelling a shell can do anything with. */
  it("hands the file to the pane as the whole path it is at", async () => {
    const handed: string[][] = [];
    await drawOpen({ onHandOver: (wholes) => handed.push(wholes) });
    await menuOn(button("a.md"));
    await click(button(t("files.pasteFilePath")));
    expect(handed).toEqual([[`${ROOT}/a.md`]]);
    // And the menu is gone: the path has been put in front of the agent, and there is nothing more
    // to pick.
    expect(button(t("files.openWith"))).toBeUndefined();
  });

  /** A panel drawn beside a face with nothing running has nowhere to hand a file to, and an item
   *  that answers nothing is worse than an item that is not there. */
  it("offers no hand-over where there is no pane to hand one to", async () => {
    await drawOpen();
    await menuOn(button("a.md"));
    expect(button(t("files.openWith"))).toBeDefined();
    expect(button(t("files.pasteFilePath"))).toBeUndefined();
  });

  /** A folder is named the same way a file is: nothing is carried, so the tree under it costs no
   *  more to name than one file does (`AMB-D-820`). What the row says is which of the two it is. */
  it("hands a folder over too, and says so in the item's own words", async () => {
    const handed: string[][] = [];
    hoisted.entries[""] = [{ name: "notes", isDir: true, ignored: false }];
    await drawOpen({ onHandOver: (wholes) => handed.push(wholes) });
    await menuOn(button("notes"));
    expect(button(t("files.pasteFilePath")), "a folder was offered the file's wording")
      .toBeUndefined();
    await click(button(t("files.pasteFolderPath")));
    expect(handed).toEqual([[`${ROOT}/notes`]]);
  });

  it("leaves the menu open while the arrows are walking it", async () => {
    await stood();
    await menuOn(button("a.md"));
    expect(button(t("files.openWith"))).toBeDefined();

    // This closed on every key once, so every press meant to walk the tree shut it on the way past.
    await press(document.body, "ArrowDown");
    await settle();
    expect(button(t("files.openWith"))).toBeDefined();

    await press(document.body, "Escape");
    await settle();
    expect(button(t("files.openWith"))).toBeUndefined();
    // And the reader is standing on the row again, not on nothing: a menu that took the focus and
    // did not give it back leaves the next arrow reaching no tree at all.
    expect(at()).toBe("a.md");
  });

  it("walks its own items with the arrows, which is what it names itself a menu for", async () => {
    await stood();
    await menuOn(button("a.md"));
    // Opened on its first item, because a list nothing is standing on is one every key falls out of.
    const items = [...document.querySelectorAll<HTMLElement>(".menu__item")];
    expect(items.length).toBeGreaterThan(1);
    expect(document.activeElement).toBe(items[0]);
    await press(document.activeElement!, "ArrowDown");
    expect(document.activeElement).toBe(items[1]);
    await press(document.activeElement!, "ArrowUp");
    expect(document.activeElement).toBe(items[0]);
    // Round, because a menu is short and a reader who walks off the end of one means to go on.
    await press(document.activeElement!, "ArrowUp");
    expect(document.activeElement).toBe(items[items.length - 1]);
  });

  it("asks before it puts a row in the bin, and bins it on yes", async () => {
    await drawOpen();
    await menuOn(button("a.md"));
    await click(button(t("files.trash")));
    // Nothing has gone yet: the item opens the question, and the question is where the answer is.
    expect(hoisted.asked.some((one) => one.startsWith("trash:"))).toBe(false);
    expect(document.body.textContent).toContain(tf("files.trashAsk", { name: "a.md" }));

    await click(anyButton(t("files.trashGo")));
    expect(hoisted.asked).toContain(`trash:${ROOT}:a.md`);
  });

  it("bins nothing when the answer is no", async () => {
    await drawOpen();
    await menuOn(button("a.md"));
    await click(button(t("files.trash")));
    await click(anyButton(t("files.trashKeep")));
    expect(hoisted.asked.some((one) => one.startsWith("trash:"))).toBe(false);
    expect(document.body.textContent).not.toContain(tf("files.trashAsk", { name: "a.md" }));
  });

  it("stops asking once the reader says not to, and only then", async () => {
    await drawOpen();
    await menuOn(button("a.md"));
    await click(button(t("files.trash")));
    // The checkbox takes effect on the answer, not on the tick: a reader who ticks it and cancels
    // has agreed to nothing.
    const quiet = document.querySelector<HTMLInputElement>(".trashask__quiet input")!;
    await act(async () => { quiet.click(); });
    await click(anyButton(t("files.trashGo")));
    expect(hoisted.asked).toContain(`trash:${ROOT}:a.md`);

    hoisted.asked = [];
    await menuOn(button("a.md"));
    await click(button(t("files.trash")));
    // No question this time, and the row is in the bin.
    expect(anyButton(t("files.trashGo"))).toBeUndefined();
    expect(hoisted.asked).toContain(`trash:${ROOT}:a.md`);
  });

  it("puts back what the last press binned, on the machine's own key", async () => {
    await drawOpen();
    await undo();
    expect(hoisted.asked).toContain("untrash");
  });

  // The bin is the tree's, and the tree is in the rail — so the column beside it has no press to
  // take, and the one it was taking belonged to whoever was writing there (`AMB-T-4523`).
  it("leaves undo to the draft page, where there is no tree to put anything back into", async () => {
    await draw({ tab: "memo" });
    await undoOn(container.querySelector("textarea")!);
    expect(hoisted.asked).not.toContain("untrash");
  });

  it("leaves undo to the box a name is being typed into", async () => {
    await drawOpen();
    await press(rows()[0]!, "F2");
    await settle();
    await undoOn(namebox()!);
    expect(hoisted.asked).not.toContain("untrash");
  });

  it("says what the machine said about a row that would not go", async () => {
    hoisted.trashed = {
      gone: [],
      stopped: { name: "a.md", why: "the volume \u201cAMBRO\u201d does not have one" },
    };
    await drawOpen();
    await menuOn(button("a.md"));
    await click(button(t("files.trash")));
    await click(anyButton(t("files.trashGo")));
    // The row it is about is gone from the list either way, so the sentence stands in the panel —
    // and it is the machine's own words, not a code with a template behind it.
    expect(container.querySelector(".files__stopped")?.textContent)
      .toContain("does not have one");
  });
});
