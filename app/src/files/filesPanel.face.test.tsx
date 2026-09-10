// @vitest-environment jsdom
// The face itself: the folder it watches, what it says out loud about that folder, and the tree it
// draws over it — down to a file dragged in from the desktop and the colour git puts on a row.
//
// It is rooted at the **project's** folder: the rows are asked for with the folder the project is
// bound to, so a pane switching underneath them changes nothing (`AMB-T-3602`). The tree is drawn at
// its first level and no deeper, one level per opening, because the names of the bound folder are
// what a reader opens this half for and everything under them is a read nobody asked for.
//
// **What the folder moving does is send everybody back to ask** (`AMB-D-785`). The host's word
// carries no rows, so what has to be right here is that the names of the open level and the colour
// beside them are read again — and that a word about another folder moves nothing in this one.
import { act } from "react";
import { describe, expect, it } from "vitest";
import {
  aFile, button, click, container, draw, drawOpen, hoisted, labelOf, openFile, press, pressable,
  ROOT, root, settle, standUpAgain, tell,
} from "./filesPanelKit";
import { formatNumber, t, tf } from "../core/i18n";
import { subscribeNotice } from "../core/notice";
import { carriedInto, carriedOver, type Held } from "./handDrag";

describe("the file face", () => {
  it("watches the project's folder, not a pane's", async () => {
    await drawOpen();
    expect(hoisted.asked).toContain(`watch:1:${ROOT}`);
    // And git is asked about the same folder, which is where the colour on each row comes from.
    expect(hoisted.asked).toContain(`git:${ROOT}`);
    expect(hoisted.asked).toContain(`entries:${ROOT}:`);
    expect(container.textContent).toContain("a.md");
  });

  it("draws the draft page as a tab, beside the files, rather than as a switch of its own", async () => {
    await draw();
    // No second row of controls: the draft page is one of the tabs, which is what the files beside
    // it are (`AMB-D-835`). The top row of the terminal face stays the way in while this column is
    // closed, and it is not drawn in here.
    expect(container.querySelector("[role=\"tablist\"]")).toBeNull();
    expect(container.querySelector(".files__tabname")?.textContent).toBe(t("files.memo"));
    // What did stay is the way out: it ends the column rather than choosing a half.
    expect(container.querySelector(".files__close")).not.toBeNull();
  });

  it("brings the draft page up from its tab, and it cannot be closed from one", async () => {
    hoisted.entries[""] = [{ name: "a.md", isDir: false, ignored: false }];
    await drawOpen();
    await openFile(button("a.md"));
    await settle();
    expect(container.querySelector(".termface__column--side textarea")).toBeNull();

    await click(container.querySelectorAll<HTMLElement>(".files__tabname")[0]);
    await settle();
    // The page is up, and the file is still held: what a tab does is choose between them.
    expect(container.querySelector(".termface__column--side textarea")).not.toBeNull();
    expect([...container.querySelectorAll<HTMLElement>(".files__tabname")].map((one) =>
      one.textContent)).toEqual([t("files.memo"), "a.md"]);
    // One cross, and it is the file's: a project has its draft page whether or not anyone opened it.
    expect(container.querySelectorAll(".files__tabclose")).toHaveLength(1);
  });

  it("goes and asks again when the host says the folder moved", async () => {
    await drawOpen();
    expect(container.textContent).toContain("a.md");
    hoisted.entries[""] = [{ name: "main.rs", isDir: false, ignored: false }];
    hoisted.git[ROOT] = [{ path: ["main.rs"], index: " ", worktree: "M", isDir: false }];
    hoisted.asked = [];

    await act(async () => {
      tell({ root: ROOT, capped: false, unwatched: false, gone: false });
      await new Promise((r) => setTimeout(r, 0));
    });
    // The word carries nothing, so both readers go back to the host: the names of the level that is
    // open, and what git says about them (`AMB-D-785`).
    expect(hoisted.asked).toContain(`entries:${ROOT}:`);
    expect(hoisted.asked).toContain(`git:${ROOT}`);
    expect(container.textContent).toContain("main.rs");
    expect(container.textContent).not.toContain("a.md");
    expect(container.querySelector(".files__file--git-modified")?.textContent).toContain("main.rs");
  });

  it("says out loud when the folder was too big to look through all of", async () => {
    hoisted.watching = { root: ROOT, capped: true, unwatched: false, gone: false };
    await draw();
    // An unwatched half looks exactly like a half where nothing happened, so it is said rather
    // than left to be assumed (`AMB-T-3604`).
    expect(container.textContent).toContain(t("files.capped"));
    // And it is not the other reason: the folder's size is what stopped the walk, and the machine
    // has watches to spare (`AMB-D-778`).
    expect(container.textContent).not.toContain(t("files.unwatched"));
  });

  it("says out loud when the machine ran out of watches, and how to get more", async () => {
    hoisted.watching = { root: ROOT, capped: false, unwatched: true, gone: false };
    await draw();
    expect(container.textContent).toContain(t("files.unwatched"));
    // The fact alone reads as something the reader did to their own project. What they can act on
    // is the machine's supply, so the way out is drawn with it.
    expect(container.textContent).toContain(t("files.unwatchedHow"));
    expect(container.textContent).not.toContain(t("files.capped"));
  });

  it("says both when both are true", async () => {
    hoisted.watching = { root: ROOT, capped: true, unwatched: true, gone: false };
    await draw();
    // Two separate things have happened to one folder, and folding them into whichever was noticed
    // first would leave the other half of the story untold.
    expect(container.textContent).toContain(t("files.capped"));
    expect(container.textContent).toContain(t("files.unwatched"));
  });

  it("leaves another folder's news alone", async () => {
    await drawOpen();
    hoisted.entries[""] = [{ name: "main.rs", isDir: false, ignored: false }];
    hoisted.asked = [];
    await act(async () => {
      tell({ root: "/work/other", capped: false, unwatched: false, gone: false });
      await new Promise((r) => setTimeout(r, 0));
    });
    // Every watched folder is told about through the one listener, so a section that asked again on
    // somebody else's news would be reading the disk for every folder every time any of them moved.
    expect(hoisted.asked).toEqual([]);
    expect(container.textContent).toContain("a.md");
    expect(container.textContent).not.toContain("main.rs");
  });

  it("takes its watch down when the face goes away", async () => {
    await draw();
    await act(async () => { root.unmount(); });
    expect(hoisted.asked).toContain(`unwatch:${ROOT}`);
    expect(hoisted.asked).toContain("unlisten");
    // Stood up again so afterEach's unmount has something to work on.
    standUpAgain();
  });

  it("hands a file to the machine rather than trying to be one", async () => {
    await drawOpen();
    const row = button("a.md")!;
    await act(async () => {
      row.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 10, clientY: 20 }));
    });
    // There is an editor here, and still a way out of it: what a person wants of a file is as often
    // to hand it to something else as to read it where it is.
    await click(button(t("files.openWith")));
    expect(hoisted.asked).toContain(`open:${ROOT}:a.md`);

    await act(async () => {
      button("a.md")!.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true }));
    });
    await click(button(t("files.reveal")));
    expect(hoisted.asked).toContain(`reveal:${ROOT}:a.md`);
  });

  it("draws the applications where the machine has no dialog of its own", async () => {
    // macOS: Launch Services only lists, so the list comes back and the menu draws it itself.
    hoisted.apps = [
      { name: "Zed", path: "/Applications/Zed.app", usual: true },
      { name: "MuseScore 4", path: "/Applications/MuseScore 4.app", usual: false },
    ];
    await drawOpen();
    await act(async () => {
      button("a.md")!.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true }));
    });
    await click(button(t("files.chooseApp")));
    expect(hoisted.asked).toContain(`ask:${ROOT}:a.md`);
    // The menu is still standing, now showing what came back — and the one the file would have
    // opened with anyway says so rather than relying on being first.
    expect(button(tf("files.appUsual", { name: "Zed" }))).toBeDefined();
    expect(button("MuseScore 4")).toBeDefined();
    // The three doors are gone: this is the same menu, not a second one over it.
    expect(button(t("files.reveal"))).toBeUndefined();

    await click(button("MuseScore 4"));
    expect(hoisted.asked).toContain(`with:${ROOT}:a.md:/Applications/MuseScore 4.app`);
  });

  it("steps aside where the machine drew the dialog itself", async () => {
    // Windows and Linux: the chooser was shown, the file is already open, and nothing came back.
    hoisted.apps = [];
    await drawOpen();
    await act(async () => {
      button("a.md")!.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true }));
    });
    await click(button(t("files.chooseApp")));
    expect(hoisted.asked).toContain(`ask:${ROOT}:a.md`);
    // An empty answer is not an empty list to draw — there is nothing left to ask.
    expect(button(t("files.chooseApp"))).toBeUndefined();
    expect(button(t("files.reveal"))).toBeUndefined();
  });

  it("closes the menu on the next thing the person does", async () => {
    await drawOpen();
    await act(async () => {
      button("a.md")!.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true }));
    });
    expect(button(t("files.openWith"))).toBeDefined();
    // A menu that outlived the next click would sit over rows it is no longer about — but the press
    // that starts a click on an item is not "the next thing", it is the choosing itself.
    await act(async () => {
      button(t("files.openWith"))!.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
    });
    expect(button(t("files.openWith"))).toBeDefined();
    await act(async () => {
      document.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
    });
    expect(button(t("files.openWith"))).toBeUndefined();
  });

  it("opens a path clicked in a pane, read against that pane's folder", async () => {
    hoisted.file = aFile({ text: "# from the pane" });
    await draw({ show: { target: "notes/a.md", cwd: ROOT, nth: 1 } });
    expect(hoisted.asked).toContain(`read:${ROOT}:notes/a.md`);
    expect(container.querySelector("h1")?.textContent).toBe("from the pane");
  });

  it("opens nothing for a path that lands outside the folder this face is rooted at", async () => {
    await draw({ show: { target: "/etc/passwd", cwd: ROOT, nth: 1 } });
    // The pane keeps the characters it drew; no reader is shown a file this face cannot answer for.
    expect(hoisted.asked.some((one) => one.startsWith("read:"))).toBe(false);
  });

  it("opens the same file again when it is clicked again", async () => {
    hoisted.file = aFile({ text: "# again" });
    await draw({ show: { target: "notes/a.md", cwd: ROOT, nth: 1 } });
    await click(button(t("files.closeFile")));
    await settle();
    expect(container.textContent).toContain(t("files.tree"));

    // The same file asked for a second time is a reader saying "open it" again.
    await draw({ show: { target: "notes/a.md", cwd: ROOT, nth: 2 } });
    expect(container.querySelector("h1")?.textContent).toBe("again");
  });

  // No project at all — the face before it has been told which one it is on, and the machine that has
  // none (`AMB-T-4358`). Nothing is read and nothing is said: the line under the tree is about a
  // project's folders, and there is no project here for it to be about.
  it("says nothing about a project's folders where there is no project", async () => {
    await draw({ projectId: null });
    expect(container.querySelector(".rail .files__none")).toBe(null);
    expect(hoisted.asked).toEqual([]);
  });

  it("draws the draft page for a project bound to no folder, which is where the files half stops", async () => {
    hoisted.bound = [];

    await draw({ tab: "memo" });
    expect(container.querySelector("textarea"), "the draft page was not drawn").toBeTruthy();
    expect(container.querySelector(".termface__column--side .files__none")).toBeFalsy();

    // The tree is the half that has to be rooted somewhere, and it is the only one the missing
    // folder stops — it says so in the rail, where it is drawn (`AMB-D-835`).
    await draw({ tab: "files" });
    expect(container.querySelector(".rail .files__none")?.textContent).toBe(t("files.noFolder"));
  });

  it("opens the first level from the start, and everything under it one at a time", async () => {
    hoisted.entries[""] = [
      { name: "src", isDir: true, ignored: false },
      { name: "README.md", isDir: false, ignored: false },
    ];
    hoisted.entries["src"] = [{ name: "main.rs", isDir: false, ignored: false }];
    await draw();
    await settle();
    // The names of the bound folder are what the half is opened for, so they are there without a
    // press: a heading over nothing was a control every reader had to work before seeing anything.
    expect(hoisted.asked).toContain(`entries:${ROOT}:`);
    expect(container.textContent).toContain("README.md");
    // A folder inside it is a name until it is opened — its children cost nothing until then.
    expect(hoisted.asked).not.toContain(`entries:${ROOT}:src`);

    await click(button("src"));
    await settle();
    expect(hoisted.asked).toContain(`entries:${ROOT}:src`);
    expect(container.textContent).toContain("main.rs");
  });

  it("folds the whole tree away on the heading, and stops reading it", async () => {
    hoisted.entries[""] = [{ name: "README.md", isDir: false, ignored: false }];
    await drawOpen();
    expect(container.textContent).toContain("README.md");

    // The press is still there and still means the same thing — a reader who wants the panes and
    // not the folder puts the tree away, and the reads it drives go with it (`AMB-D-785`).
    await click(button(t("files.tree")));
    await settle();
    expect(pressable(t("files.tree"))?.getAttribute("aria-expanded")).toBe("false");
    expect(container.textContent).not.toContain("README.md");
  });

  // The heading is the row over the tree, so it opens and shuts with the mark the rows inside it
  // carry — one drawing at one size, rather than a typed arrow whose size no rule reaches
  // (`AMB-D-686`).
  it("says on the heading whether the tree is open, with the mark the rows use", async () => {
    await drawOpen();
    const mark = () =>
      pressable(t("files.tree"))?.querySelector(".files__twisty [data-icon]")
        ?.getAttribute("data-icon");
    expect(mark()).toBe("chevronDown");

    await click(button(t("files.tree")));
    await settle();
    expect(mark()).toBe("chevronRight");
  });

  // Reading a file draws the reader over the tree rather than in place of it (`AMB-D-815`), so the
  // tree is still on the page while it is open. What a reader did to it has to outlive that: the
  // panel holds it, not the section (`./FilesPanel`).
  it("keeps the tree as the reader left it while a file is being read", async () => {
    hoisted.entries[""] = [
      { name: "src", isDir: true, ignored: false },
      { name: "a.md", isDir: false, ignored: false },
    ];
    hoisted.entries["src"] = [{ name: "main.rs", isDir: false, ignored: false }];
    await drawOpen();
    await click(button("src"));
    await settle();
    expect(container.textContent).toContain("main.rs");

    const stop = () =>
      [...container.querySelectorAll<HTMLElement>("[role=\"treeitem\"]")]
        .find((one) => one.tabIndex === 0);
    // By its own name and not loosely, so that a row is found by the whole of what it is called.
    const named = (name: string) =>
      [...container.querySelectorAll<HTMLElement>("[role=\"treeitem\"]")]
        .find((one) => labelOf(one) === name);
    // Onto the row and into the file in one gesture: a press is what puts the tab stop somewhere,
    // so pressing the row twice both stands the reader on it and opens what it names.
    await openFile(named("main.rs"));
    await settle();
    // Both at once: the file is up and the tree is still under it. Drawn in place of each other,
    // the tree came off the page here, and its state went with it.
    expect(pressable(t("files.closeFile"))).toBeDefined();
    expect(container.querySelector("[role=\"treeitem\"]")).not.toBeNull();

    await click(pressable(t("files.closeFile")));
    await settle();
    // Both openings still stand, and the stop is on the row it was left on — a tree that folded
    // itself shut here would send the reader back through every press they had already made.
    expect(pressable(t("files.tree"))?.getAttribute("aria-expanded")).toBe("true");
    expect(container.textContent).toContain("main.rs");
    expect(labelOf(stop()!)).toBe("main.rs");
  });

  // The tree stays on the screen while a file is read (`AMB-D-815`), so the row that was opened has
  // to say so: without a mark it is one name among the rest, and the panel over it is about a file
  // the reader can no longer point to.
  it("marks the row whose file is being read, and unmarks it when the file goes", async () => {
    hoisted.entries[""] = [
      { name: "a.md", isDir: false, ignored: false },
      { name: "b.md", isDir: false, ignored: false },
    ];
    await drawOpen();
    await openFile(button("a.md"));
    await settle();

    const chosen = () => container.querySelector(".files__file--chosen")?.textContent;
    expect(chosen()).toContain("a.md");

    // The other file, so that the mark is shown to move rather than merely to exist.
    await click(pressable(t("files.closeFile")));
    await settle();
    await openFile(button("b.md"));
    await settle();
    expect(chosen()).toContain("b.md");

    await click(pressable(t("files.closeFile")));
    await settle();
    expect(container.querySelector(".files__file--chosen")).toBeNull();
  });

  // Two things are over the page once a file is open, so "back" has to mean one of them at a time.
  // Both on the same key, because a reader pressing it twice is saying the same thing twice.
  it("takes one layer per Escape — the wide width first, and the column after it", async () => {
    hoisted.entries[""] = [{ name: "a.md", isDir: false, ignored: false }];
    let closed = 0;
    await drawOpen({ onClose: () => { closed += 1; } });
    // Opening a file asks for the wide width, which is the layer the first press takes off
    // (`AMB-D-835`).
    await openFile(button("a.md"));
    await settle();
    const width = () => container.querySelector<HTMLElement>(".files__width")!;
    expect(width().getAttribute("aria-pressed")).toBe("true");

    // On the column the file is in: the layers this key takes off are the width and the column
    // holding it, and the tree in the rail is neither of them.
    const column = () => container.querySelector(".termface__column--side .files")!;
    await press(column(), "Escape");
    await settle();
    // The column is narrow and it is not closed: one press, one layer. The file stays where it is —
    // what a reader asked for is the panes back, not the file put away.
    expect(width().getAttribute("aria-pressed")).toBe("false");
    expect(pressable(t("files.closeFile"))).toBeDefined();
    expect(closed).toBe(0);

    await press(column(), "Escape");
    await settle();
    expect(closed).toBe(1);
  });

  it("goes between the two widths from the control beside the way out", async () => {
    hoisted.entries[""] = [{ name: "a.md", isDir: false, ignored: false }];
    await drawOpen();
    const width = () => container.querySelector<HTMLElement>(".files__width")!;
    // Nothing open yet: the column is narrow, and the control offers the room to read in.
    expect(width().getAttribute("aria-pressed")).toBe("false");

    await click(width());
    await settle();
    expect(width().getAttribute("aria-pressed")).toBe("true");

    // Pressed again it gives the panes back: one control, two ends.
    await click(width());
    await settle();
    expect(width().getAttribute("aria-pressed")).toBe("false");
  });

  // Held by the folder it is about, so a binding somebody removed must take its own answer with it
  // — otherwise the reader who binds that path back is handed a tree they never opened.
  it("forgets how a folder was opened once nobody is bound to it", async () => {
    hoisted.entries[""] = [{ name: "src", isDir: true, ignored: false }];
    hoisted.entries["src"] = [{ name: "main.rs", isDir: false, ignored: false }];
    await drawOpen();
    await click(button("src"));
    await settle();
    expect(container.textContent).toContain("main.rs");

    hoisted.bound = [];
    await draw();
    hoisted.bound = [{ path: ROOT, exists: true }];
    await drawOpen();
    // Back to where a tree nobody has touched stands: the first level, and nothing under it.
    expect(pressable(t("files.tree"))?.getAttribute("aria-expanded")).toBe("true");
    expect(container.textContent).toContain("src");
    expect(container.textContent).not.toContain("main.rs");
  });

  // The host says where the pointer is and nothing else, so which folder a file would land in is
  // the panel's own answer — and the answer a reader can see before they let go (`AMB-D-775`).
  it("marks the folder a file dragged in from the desktop would land in", async () => {
    hoisted.entries[""] = [
      { name: "src", isDir: true, ignored: false },
      { name: "README.md", isDir: false, ignored: false },
    ];
    hoisted.entries["src"] = [{ name: "main.rs", isDir: false, ignored: false }];
    await drawOpen();
    await click(button("src"));
    await settle();

    // jsdom lays nothing out, so what is under the point is stated. What is being read is the walk
    // up from it: a file row belongs to the folder holding it, so hanging over `main.rs` is hanging
    // over `src` — the same folder as hanging over its name.
    // Each move is at a point of its own: a move that repeats the last one is dropped on the way in,
    // because macOS sends the same point twice while the drag stands still (`../core/hostDrop`).
    let step = 0;
    const over = (el: Element | null | undefined) => act(async () => {
      (document as unknown as { elementFromPoint: () => Element | null }).elementFromPoint =
        () => el ?? null;
      step += 1;
      hoisted.dragging?.({ payload: { type: "over", position: { x: step, y: 1 } } });
      await new Promise((r) => setTimeout(r, 0));
    });
    const marked = () => container.querySelector(".files__into")?.getAttribute("data-into");

    await over(button("main.rs"));
    expect(marked()).toBe("src");

    // A row belonging to no folder in the tree belongs to the tree, which is the root itself.
    await over(button("README.md"));
    expect(marked()).toBeUndefined();
    expect(container.querySelector(".files__row--into")?.getAttribute("data-into")).toBe("");

    // Nothing under the pointer is nothing marked: a highlight left standing would name a folder
    // the reader had already dragged away from.
    await over(null);
    expect(container.querySelector(".files__into, .files__row--into")).toBeNull();
  });

  // The highlight said which folder; letting go is the panel carrying the files into that one and no
  // other. Both halves of the landing travel, because the same path inside two bound folders is two
  // places (`AMB-T-3781`).
  it("carries what was dropped into the folder the highlight named", async () => {
    hoisted.entries[""] = [{ name: "src", isDir: true, ignored: false }];
    hoisted.entries["src"] = [{ name: "main.rs", isDir: false, ignored: false }];
    await drawOpen();
    await click(button("src"));
    await settle();

    const drop = (el: Element | null | undefined, paths: string[]) => act(async () => {
      (document as unknown as { elementFromPoint: () => Element | null }).elementFromPoint =
        () => el ?? null;
      hoisted.dragging?.({ payload: { type: "drop", position: { x: 1, y: 1 }, paths } });
      await new Promise((r) => setTimeout(r, 0));
    });

    await drop(button("main.rs"), ["/Users/someone/Desktop/note.md"]);
    expect(hoisted.imported).toEqual([{
      paths: ["/Users/someone/Desktop/note.md"],
      toRoot: ROOT,
      to: ["src"],
      // Neither modifier was held, and the host reads that as neither: what a plain drop means is
      // decided where the carry is made, and it copies.
      effect: "default",
    }]);
    // And the highlight is gone the moment the files are let go.
    expect(container.querySelector(".files__into, .files__row--into")).toBeNull();

    // The bound folder itself is a landing like any other, and its path is no segments at all.
    await drop(container.querySelector("[data-into=\"\"]"), ["/Users/someone/Desktop/other.md"]);
    expect(hoisted.imported[1]?.to).toEqual([]);
  });

  // Nothing is said about what arrived — the folder is watched and is about to list it. What stops
  // has nothing drawing it, so that is what is said, and it says how far the carry got.
  it("says what a carry stopped on, and how much of it had already arrived", async () => {
    await draw();
    const said: string[] = [];
    const stop = subscribeNotice((line) => said.push(line));
    const drop = (paths: string[]) => act(async () => {
      (document as unknown as { elementFromPoint: () => Element | null }).elementFromPoint =
        () => container.querySelector("[data-into]");
      hoisted.dragging?.({ payload: { type: "drop", position: { x: 1, y: 1 }, paths } });
      await new Promise((r) => setTimeout(r, 0));
    });

    // The machine's own words, carried through as they came: no code, so nothing here rewrites them.
    hoisted.carried = { arrived: [], stopped: { name: "note.md", code: null, why: "no room left" } };
    await drop(["/a/note.md"]);
    expect(said).toEqual([tf("files.dropStopped", { name: "note.md", why: "no room left" })]);

    hoisted.carried = {
      arrived: ["one.md"],
      stopped: { name: "note.md", code: null, why: "no room left" },
    };
    await drop(["/a/one.md", "/a/note.md"]);
    expect(said[1]).toBe(
      tf("files.dropPartly", { name: "note.md", why: "no room left", count: formatNumber(1) }),
    );

    // A carry that got the whole way through says nothing at all.
    hoisted.carried = { arrived: ["one.md"], stopped: null };
    await drop(["/a/one.md"]);
    expect(said).toHaveLength(2);
    stop();
  });

  // What Amenbo itself refused. The host sends the sentence as well as the code, in English, and the
  // face is asked to draw the code — a reader whose screen is in another language would otherwise be
  // handed the one sentence on it that was never translated.
  it("says a refusal of its own in the reader's language, not in the host's English", async () => {
    await draw();
    const said: string[] = [];
    const stop = subscribeNotice((line) => said.push(line));
    const drop = (paths: string[]) => act(async () => {
      (document as unknown as { elementFromPoint: () => Element | null }).elementFromPoint =
        () => container.querySelector("[data-into]");
      hoisted.dragging?.({ payload: { type: "drop", position: { x: 1, y: 1 }, paths } });
      await new Promise((r) => setTimeout(r, 0));
    });

    hoisted.carried = {
      arrived: [],
      stopped: { name: "note.md", code: "taken", why: "note.md is already there" },
    };
    await drop(["/a/note.md"]);
    expect(said).toEqual([
      tf("files.dropStopped", { name: "note.md", why: t("files.stoppedTaken") }),
    ]);
    stop();
  });

  it("draws a name the repository ignores, and draws it faintly", async () => {
    hoisted.entries[""] = [
      { name: "src", isDir: true, ignored: false },
      { name: ".env", isDir: false, ignored: true },
      { name: ".next", isDir: true, ignored: true },
    ];
    await drawOpen();
    // On the list, because what git does not record is still somebody's file — and faint, because
    // that is the whole of what being ignored says about it (`AMB-D-786`).
    expect(container.textContent).toContain(".env");
    expect(container.querySelector(".files__file--ignored")?.textContent).toContain(".env");
    expect(container.querySelector(".files__dir--ignored")?.textContent).toContain(".next");
    // The one nothing ignores is drawn as it always was.
    expect(container.querySelector(".files__dir:not(.files__dir--ignored)")?.textContent)
      .toContain("src");
  });

  it("wears what git says about each row, and nothing where git says nothing", async () => {
    hoisted.entries[""] = [
      { name: "changed.md", isDir: false, ignored: false },
      { name: "staged.md", isDir: false, ignored: false },
      { name: "new.md", isDir: false, ignored: false },
      { name: "same.md", isDir: false, ignored: false },
    ];
    hoisted.git[ROOT] = [
      { path: ["changed.md"], index: " ", worktree: "M", isDir: false },
      { path: ["staged.md"], index: "A", worktree: " ", isDir: false },
      { path: ["new.md"], index: "?", worktree: "?", isDir: false },
    ];
    await drawOpen();
    const marked = (mark: string) =>
      container.querySelector(`.files__file--git-${mark}`)?.textContent;
    expect(marked("modified")).toContain("changed.md");
    expect(marked("added")).toContain("staged.md");
    expect(marked("untracked")).toContain("new.md");
    // The row git said nothing about is the row nothing is said about: no colour is an answer.
    const plain = [...container.querySelectorAll(".files__file")]
      .find((one) => one.textContent === "same.md");
    expect(plain?.className).toBe("files__file");
  });

  it("colours what is inside a folder git named as a whole", async () => {
    hoisted.entries[""] = [{ name: "fresh", isDir: true, ignored: false }];
    hoisted.entries["fresh"] = [{ name: "one.md", isDir: false, ignored: false }];
    // What git does with an untracked folder: it names the folder and stops. A tree that matched
    // paths exactly would leave every file in a brand-new folder colourless.
    hoisted.git[ROOT] = [{ path: ["fresh"], index: "?", worktree: "?", isDir: true }];
    await drawOpen();
    expect(container.querySelector(".files__dir--git-untracked")?.textContent).toContain("fresh");
    await click(button("fresh"));
    await settle();
    expect(container.querySelector(".files__file--git-untracked")?.textContent).toContain("one.md");
  });

  it("colours a folded folder for what is under it, and lets it go plain when it opens", async () => {
    hoisted.entries[""] = [{ name: "src", isDir: true, ignored: false }];
    hoisted.entries["src"] = [{ name: "main.rs", isDir: false, ignored: false }];
    // git names the file and not the folder holding it, so a tree that matched what git said would
    // leave the folded row saying nothing about the change it is hiding (`AMB-D-795`).
    hoisted.git[ROOT] = [{ path: ["src", "main.rs"], index: " ", worktree: "M", isDir: false }];
    await drawOpen();
    expect(container.querySelector(".files__dir--git-modified")?.textContent).toContain("src");

    await click(button("src"));
    await settle();
    // Open, the row inside says it, and the folder stops saying it: two rows in one colour down one
    // column would be the tree saying "somewhere, something".
    expect(container.querySelector(".files__dir--git-modified")).toBeNull();
    expect(container.querySelector(".files__file--git-modified")?.textContent).toContain("main.rs");
  });

  it("colours a whole repository nothing is tracked in yet", async () => {
    hoisted.entries[""] = [{ name: "one.md", isDir: false, ignored: false }];
    // Zero segments is the bound folder itself, which is what git names when nothing under it is
    // tracked. Dropped, a new repository would have no colour anywhere.
    hoisted.git[ROOT] = [{ path: [], index: "?", worktree: "?", isDir: true }];
    await drawOpen();
    expect(container.querySelector(".files__file--git-untracked")?.textContent).toContain("one.md");
  });

});

describe("a row of the panel carried to one of its own folders", () => {
  /** The tree of the tests below: one folder, one file inside it, two files beside it. */
  async function tree() {
    hoisted.entries[""] = [
      { name: "src", isDir: true, ignored: false },
      { name: "note.md", isDir: false, ignored: false },
    ];
    hoisted.entries["src"] = [{ name: "main.rs", isDir: false, ignored: false }];
    await drawOpen();
    await click(button("src"));
    await settle();
  }

  /** `note.md`, as a press on it takes hold of it (`./handDrag`). */
  const note: Held = { wholes: [`${ROOT}/note.md`], root: ROOT, paths: [["note.md"]] };

  /** Let a row go over the folder drawn for `into`, with or without the key that asks for a copy. */
  const letGo = (into: string, copy = false) => act(async () => {
    const el = container.querySelector<HTMLElement>(`[data-into="${into}"]`)!;
    carriedInto(el, note, copy);
    await new Promise((r) => setTimeout(r, 0));
  });

  it("moves it there, both halves of the landing travelling", async () => {
    await tree();

    await letGo("src");

    expect(hoisted.carries).toEqual([{
      how: "move", root: ROOT, paths: [["note.md"]], toRoot: ROOT, to: ["src"],
    }]);
  });

  it("copies it instead where the key for a copy was held", async () => {
    await tree();

    await letGo("src", true);

    expect(hoisted.carries.map((one) => one.how)).toEqual(["copy"]);
  });

  it("does nothing at all where it was let go over the folder it is already in", async () => {
    await tree();

    // The bound folder itself, which is where `note.md` sits. The host would answer that it is
    // already there, which is a true sentence about a gesture that asked for nothing.
    await letGo("");

    expect(hoisted.carries).toEqual([]);
  });

  it("marks the folder under it, and lets the mark go with the gesture", async () => {
    await tree();

    await act(async () => {
      carriedOver(container.querySelector<HTMLElement>('[data-into="src"]'));
    });
    expect(container.querySelector(".files__into")?.getAttribute("data-into")).toBe("src");

    await act(async () => { carriedOver(null); });
    expect(container.querySelector(".files__into, .files__row--into")).toBeNull();
  });

  it("says what stopped the carry, and says nothing of what went", async () => {
    await tree();
    hoisted.carried = { arrived: [], stopped: { name: "note.md", code: "taken", why: "" } };
    const said: string[] = [];
    const stop = subscribeNotice((line) => said.push(line));

    await letGo("src");

    stop();
    // The folder is watched and is about to draw what arrived; what did not arrive is the only
    // half of the answer nothing else on the screen would say.
    expect(said).toEqual([
      tf("files.dropStopped", { name: "note.md", why: t("files.stoppedTaken") }),
    ]);
  });
});
