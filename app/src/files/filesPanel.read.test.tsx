// @vitest-environment jsdom
// Reading a file, none of which is visible in the markup on its own.
//
// The panel asks the host what a file is rather than deciding from its name: Markdown is drawn as
// Markdown and can be turned over to the text it is, the bytes and the newlines are said out loud on
// the file's own row, and where there is nothing a panel can show it says so plainly.
import { act } from "react";
import { describe, expect, it } from "vitest";
import {
  aFile, button, click, container, draw, drawOpen, hoisted, last, megabytes, openFile, press,
  ROOT, settle,
} from "./filesPanelKit";
import { type CmdError, errLabel, formatNumber, t, tf } from "../core/i18n";

describe("the file face", () => {
  it("draws a Markdown file as Markdown", async () => {
    hoisted.file = aFile({ text: "# A heading" });
    await drawOpen();
    await openFile(button("a.md"));
    await settle();
    expect(hoisted.asked).toContain(`read:${ROOT}:a.md`);
    expect(container.querySelector("h1")?.textContent).toBe("A heading");
  });

  /** The text is what the file holds and the rendering is a view of it (`AMB-D-41`). Until now the
   *  one kind of file an agent writes most was the one kind nobody could correct: `.md` went to the
   *  renderer and never to the editor. */
  it("switches a Markdown file between what it draws and the text it is", async () => {
    hoisted.file = aFile({ text: "# A heading" });
    await drawOpen();
    await openFile(button("a.md"));
    await settle();
    // It opens on the rendering: what a person opens a Markdown file for is to read it.
    expect(container.querySelector("h1")?.textContent).toBe("A heading");
    expect(hoisted.editing).toHaveLength(0);

    await click(button(t("files.edit")));
    await settle();
    expect(container.querySelector("h1")).toBeNull();
    expect(last(hoisted.editing)).toEqual({ text: "# A heading", editable: true, name: "a.md" });

    await click(button(t("files.read")));
    await settle();
    expect(container.querySelector("h1")?.textContent).toBe("A heading");
  });

  /** Switching over to the rendering takes the editor off the page, and what a person typed is
   *  only in there — so it is caught on the way out, and the rendering and the editor that comes
   *  back are both drawn from what was caught. Nothing on this road warns or asks, which is why
   *  the catching is what has to be right: the offer to save stands over the text either way.
   *
   *  Driven with a file that can be written back, so that what the save carries is part of what is
   *  under test: keeping the text on the screen and losing it at the door would be the same bug
   *  one step further along. */
  describe("a Markdown file switched over with something typed in it", () => {
    /** The reader typing, as the stand-in editor reports it, and then the text it now holds. */
    async function typeInto(text: string) {
      await act(async () => {
        const drawn = container.querySelector(".cm-editor");
        if (drawn !== null) drawn.textContent = text;
        hoisted.typing?.();
        await new Promise((r) => setTimeout(r, 0));
      });
    }

    /** Open `notes.md` in the editor, with something typed into it. */
    async function typedInNotes() {
      hoisted.entries[""] = [{ name: "notes.md", isDir: false, ignored: false }];
      hoisted.file = aFile({ text: "# A heading", encoding: "UTF-8", digest: "before" });
      await drawOpen();
      await openFile(button("notes.md"));
      await settle();
      await click(button(t("files.edit")));
      await settle();
      await typeInto("# What was typed");
    }

    it("draws the rendering from what was typed, and gives it back to the editor", async () => {
      await typedInNotes();

      // The text is what the file holds and the rendering is a view of it (`AMB-D-41`) — of what
      // is being written, not of the copy the disk is still on.
      await click(button(t("files.read")));
      await settle();
      expect(container.querySelector("h1")?.textContent).toBe("What was typed");

      await click(button(t("files.edit")));
      await settle();
      expect(last(hoisted.editing)?.text).toBe("# What was typed");
      // And it is still theirs to save, which is what the control was saying all along.
      await click(button(t("files.save")));
      await settle();
      expect(last(hoisted.saved)?.text).toBe("# What was typed");
    });

    it("carries none of it to the next file opened", async () => {
      await typedInNotes();
      await click(button(t("files.read")));
      await settle();

      hoisted.file = aFile({ text: "# Another file" });
      await draw({ show: { target: "notes/b.md", cwd: ROOT, nth: 1 } });
      expect(container.querySelector("h1")?.textContent).toBe("Another file");
    });
  });

  it("draws no switch on a file there is only one way to show", async () => {
    hoisted.entries[""] = [{ name: "run.sh", isDir: false, ignored: false }];
    hoisted.file = aFile({ text: "echo hi" });
    await drawOpen();
    await openFile(button("run.sh"));
    await settle();
    // A switch with nowhere to switch to is a control that answers nothing.
    expect(button(t("files.edit"))).toBeUndefined();
    expect(button(t("files.read"))).toBeUndefined();
  });

  /** A choice that outlived the file would be a setting nobody set: one edit, and every Markdown
   *  file afterwards opens as source — the ones they only wanted to read included.
   *
   *  Driven through a path clicked in a pane, which is the one road that changes the file under a
   *  reader that stays on the screen. Going back to the list and picking another row takes the
   *  reader off the page and would prove only that a fresh one starts fresh. */
  it("opens the next Markdown file on the rendering, whatever the last one was left on", async () => {
    hoisted.file = aFile({ text: "# A heading" });
    await draw({ show: { target: "notes/a.md", cwd: ROOT, nth: 1 } });
    await click(button(t("files.edit")));
    await settle();
    expect(container.querySelector("h1")).toBeNull();

    await draw({ show: { target: "notes/b.md", cwd: ROOT, nth: 2 } });
    expect(hoisted.asked).toContain(`read:${ROOT}:notes/b.md`);
    expect(container.querySelector("h1")?.textContent).toBe("A heading");
  });

  it("draws text that is not Markdown as it was written", async () => {
    hoisted.entries[""] = [{ name: "run.sh", isDir: false, ignored: false }];
    hoisted.file = aFile({ text: "#!/bin/sh\necho hi" });
    await drawOpen();
    await openFile(button("run.sh"));
    await settle();
    // Not a heading: the hash in a shell script is a comment, and a name is what decides that.
    expect(container.querySelector("h1")).toBeNull();
    expect(container.querySelector(".cm-editor")?.textContent).toBe("#!/bin/sh\necho hi");
    // And it is a file this panel could save, so it is one somebody may type into. The name goes
    // with it: what language a file is written in is the only thing that says how to colour it.
    expect(last(hoisted.editing))
      .toEqual({ text: "#!/bin/sh\necho hi", editable: true, name: "run.sh" });
  });

  /** A file the panel could never write back is read-only from the moment it opens. Saying so after
   *  somebody has typed into it would be worse than not letting them: the text they wrote would
   *  have nowhere to go (`AMB-D-773`). */
  it("opens a file it could not save without letting anyone type into it", async () => {
    hoisted.entries[""] = [{ name: "cut.txt", isDir: false, ignored: false }];
    hoisted.file = aFile({ text: "as far as it goes", truncated: true, clean: false });
    await drawOpen();
    await openFile(button("cut.txt"));
    await settle();
    expect(last(hoisted.editing)?.editable).toBe(false);

    // Whole, but in an encoding nothing writes back: the same answer, for the other reason.
    hoisted.file = aFile({ text: "read me", clean: false });
    await click(button(t("files.closeFile")));
    await settle();
    await openFile(button("cut.txt"));
    await settle();
    expect(last(hoisted.editing)?.editable).toBe(false);
  });

  /** The guess reports no confidence and breaks nothing visible when it is wrong — 46 files were
   *  misread with not one damaged character between them — so the reader is the only one who can
   *  catch it, and only if they are told what was guessed (`AMB-D-773`). */
  it("says on the file's own row what the bytes were read as, and how the lines end", async () => {
    hoisted.file = aFile({ text: "a", encoding: "Shift_JIS", lineEnding: "crlf" });
    await drawOpen();
    await openFile(button("a.md"));
    await settle();
    expect(button("Shift_JIS")?.textContent).toContain("CRLF");
  });

  it("says in words that a file's newlines are mixed, rather than in a token nobody reads", async () => {
    hoisted.file = aFile({ text: "a", encoding: "UTF-8", lineEnding: "mixed" });
    await drawOpen();
    await openFile(button("a.md"));
    await settle();
    expect(container.textContent).toContain(t("files.lineEndingMixed"));
  });

  it("asks the host to read the file again in the encoding the reader named", async () => {
    hoisted.file = aFile({ text: "a", encoding: "windows-1252" });
    await drawOpen();
    await openFile(button("a.md"));
    await settle();
    expect(hoisted.asked).toContain(`read:${ROOT}:a.md`);

    await click(button("windows-1252"));
    await settle();
    // The list is the host's, not a copy kept here: what may be offered is what can be written back.
    expect(hoisted.asked).toContain("encodings");

    hoisted.asked = [];
    hoisted.file = aFile({ text: "あ", encoding: "Shift_JIS" });
    await click(button("Shift_JIS"));
    await settle();
    expect(hoisted.asked).toContain(`read:${ROOT}:a.md:Shift_JIS`);
    expect(container.textContent).toContain("あ");
  });

  /** The list of encodings wore a box of its own, and that box closed on every key — the bug
   *  `AMB-D-780` took out of the file rows' menu and left standing here. */
  it("leaves the encodings open while the arrows are walking them", async () => {
    hoisted.file = aFile({ text: "a", encoding: "windows-1252" });
    await drawOpen();
    await openFile(button("a.md"));
    await settle();
    await click(button("windows-1252"));
    await settle();
    expect(button("Shift_JIS")).toBeDefined();

    await press(document.activeElement!, "ArrowDown");
    await settle();
    expect(button("Shift_JIS")).toBeDefined();

    await press(document.body, "Escape");
    await settle();
    expect(button("Shift_JIS")).toBeUndefined();
  });

  /** A file whose bytes and text no longer say the same thing is exactly the file a wrong guess
   *  produces, so the road out of a wrong guess has to be open on it. */
  it("offers the encodings on a file it could not save", async () => {
    // Not Markdown: a file drawn as a document never reaches the editor, and what is under test
    // here is that the road out is open on the very file the editor has locked.
    hoisted.entries[""] = [{ name: "guessed.txt", isDir: false, ignored: false }];
    hoisted.file = aFile({ text: "?????", encoding: "windows-1252", clean: false });
    await drawOpen();
    await openFile(button("guessed.txt"));
    await settle();
    expect(last(hoisted.editing)?.editable).toBe(false);
    expect(button("windows-1252")).toBeDefined();
  });

  /** The name is the only thing on the bar that can give way, every control beside it being as wide
   *  as its own words — so it gave way to almost nothing once three tasks had each put one there.
   *  What acts on the file stands on a row of its own now, and the name has the bar to itself. */
  it("keeps the name on a row of its own, with what acts on the file under it", async () => {
    hoisted.entries[""] = [{ name: "run.sh", isDir: false, ignored: false }];
    hoisted.file = aFile({ text: "#!/bin/sh\necho hi", encoding: "UTF-8" });
    await drawOpen();
    await openFile(button("run.sh"));
    await settle();

    const bar = container.querySelector(".files__bar");
    expect(bar?.querySelector(".files__name")?.textContent).toBe("run.sh");
    // Nothing that acts on the file shares the row, which is the whole of the fix.
    for (const one of [".files__view", ".files__encoding", ".files__keep"]) {
      expect(bar?.querySelector(one), `${one} is still on the name's row`).toBeNull();
    }
    // And they are all on the row under it, rather than gone.
    const tools = container.querySelector(".files__tools");
    expect(tools?.querySelector(".files__encoding")).toBeTruthy();
    expect(tools?.querySelector(".files__keep")).toBeTruthy();
  });

  /** A picture has nothing to switch, nothing to reopen and nothing to save, so the row that would
   *  hold them is not drawn at all — a panel this narrow does not spend a line on an empty one. */
  it("draws no second row where there is nothing to put on it", async () => {
    hoisted.file = aFile({ image: { mime: "image/png" } });
    await drawOpen();
    await openFile(button("a.md"));
    await settle();

    expect(container.querySelector(".files__name")?.textContent).toBe("a.md");
    expect(container.querySelector(".files__tools")).toBeNull();
  });

  it("says nothing about the encoding of a picture", async () => {
    hoisted.file = aFile({ image: { mime: "image/png" } });
    await drawOpen();
    await openFile(button("a.md"));
    await settle();
    // A picture has no encoding to be wrong about, and a control that asked about one would be
    // asking a question the file cannot answer.
    expect(container.querySelector(".files__encoding")).toBeNull();
  });

  /** Opening a file the panel can write back and typing into it: the one door that changes what is
   *  in the folder rather than what is on the screen (`AMB-D-776`). */
  it("says so when the file is not something a panel can show", async () => {
    hoisted.file = aFile();
    await drawOpen();
    await openFile(button("a.md"));
    await settle();
    // The name says Markdown and the bytes say otherwise; the bytes win (`crate::folder`).
    expect(container.textContent).toContain(t("files.notText"));

    // A file this face cannot draw is still a file the reader wants opened, and what they are
    // pointed at is the menu the rows already carry — nothing of its own for this state
    // (`AMB-T-4352`).
    await click(button(t("files.openElsewhere")));
    await click(button(t("files.openWith")));
    expect(hoisted.asked).toContain(`open:${ROOT}:a.md`);
  });

  /** A link is refused on purpose (`AMB-D-782`), and answering it with the sentence every other
   *  refusal gets told the person most likely to meet it — somebody sharing one `CLAUDE.md` between
   *  projects — that their file was broken. */
  it("says a name it would not follow is a link, rather than a file it could not read", async () => {
    const link: CmdError = {
      code: "folder_link",
      message_en: "this name is a link, and a link is not followed here",
      fields: null,
    };
    hoisted.refuseRead = link;
    await drawOpen();
    await openFile(button("a.md"));
    await settle();
    expect(container.textContent).toContain(errLabel(link));
    expect(container.textContent).not.toContain(t("files.unreadable"));
    // And no way on, because handing the name to the machine is following the link after all — the
    // one thing this refusal exists to not do (`AMB-D-782`).
    expect(button(t("files.openElsewhere"))).toBeUndefined();
    // The sentence is the reader's, not the English one that came with the refusal.
    expect(container.textContent).not.toContain(link.message_en);
  });

  /** Everything else keeps the one sentence, because everything else is what the host had nothing
   *  finer to say about — and core's own English would name this project's folders at a reader who
   *  asked about a file. */
  it("says only that a file could not be read where that is all the host said", async () => {
    hoisted.refuseRead = {
      code: "not_found",
      message_en: "no such file in this project's folder",
      fields: null,
    };
    await drawOpen();
    await openFile(button("a.md"));
    await settle();
    expect(container.textContent).toContain(t("files.unreadable"));
    expect(container.textContent).not.toContain("no such file");

    // What Amenbo could not read, another application may well open (`AMB-T-4352`).
    await click(button(t("files.openElsewhere")));
    await click(button(t("files.openWith")));
    expect(hoisted.asked).toContain(`open:${ROOT}:a.md`);
  });

  it("points a picture at the door that hands out a file, not at bytes of its own", async () => {
    hoisted.file = aFile({ image: { mime: "image/png" } });
    await drawOpen();
    await openFile(button("a.md"));
    await settle();
    // The address is the project, the folder and the path this reader was opened on — the same
    // three the host resolved the answer through, so nothing had to be carried (`AMB-D-783`).
    expect(container.querySelector("img")?.getAttribute("src"))
      .toBe("amenbofile://localhost/1/%2Fwork%2Frepo/a.md?mime=image%2Fpng");
  });

  it("says what a picture it would not draw was measured at, and offers the way on", async () => {
    hoisted.file = aFile({ oversize: { bytes: 6 * 1024 * 1024, width: 40000, height: 30000 } });
    await drawOpen();
    await openFile(button("a.md"));
    await settle();
    // A refusal drawn as nothing at all reads as a damaged file, so both numbers travel with it and
    // the reader is pointed at something built to open it (`AMB-D-783`).
    expect(container.textContent).toContain(t("files.tooBig"));
    expect(container.textContent).toContain(megabytes(6));
    expect(container.textContent).toContain(
      tf("files.tooBigPixels", { width: formatNumber(40000), height: formatNumber(30000) }),
    );
    // Not the sentence for a file there is nothing to show of: this one is a picture, and it is
    // being refused rather than failed to be read.
    expect(container.textContent).not.toContain(t("files.notText"));

    await click(button(t("files.openElsewhere")));
    // The way on is the one the list rows already open — nothing new was invented for this state.
    expect(button(t("files.chooseApp"))).toBeDefined();
    await click(button(t("files.openWith")));
    expect(hoisted.asked).toContain(`open:${ROOT}:a.md`);
  });

  it("refuses on bytes alone where the picture would not say its size", async () => {
    hoisted.file = aFile({ oversize: { bytes: 6 * 1024 * 1024 } });
    await drawOpen();
    await openFile(button("a.md"));
    await settle();
    // A size nobody could read is not printed as a guess (`crate::folder`).
    expect(container.textContent).toContain(megabytes(6));
    expect(container.textContent).not.toContain("×");
  });

  it("writes a refused picture's size in a unit that says something about it", async () => {
    // The pictures that cost the most to draw are the ones that compress best, so a refusal on
    // pixels alone is commonly a file of a few kilobytes. Rounded to megabytes it would read "0 MB"
    // — the file said to be empty, which is the opposite of what it is (`AMB-D-783`).
    hoisted.file = aFile({ oversize: { bytes: 10 * 1024, width: 16000, height: 16000 } });
    await drawOpen();
    await openFile(button("a.md"));
    await settle();
    expect(container.textContent).toContain(
      formatNumber(10, { style: "unit", unit: "kilobyte", unitDisplay: "short", maximumFractionDigits: 0 }),
    );
  });

  it("goes all the way down to bytes rather than round a refused picture to nothing", async () => {
    // A header alone is the cheapest thing that can claim thirty thousand square, and it is under a
    // kilobyte. Rounded up a unit it would read "0 kB" — the same lie one unit further down.
    hoisted.file = aFile({ oversize: { bytes: 33, width: 30000, height: 30000 } });
    await drawOpen();
    await openFile(button("a.md"));
    await settle();
    expect(container.textContent).toContain(
      formatNumber(33, { style: "unit", unit: "byte", unitDisplay: "short", maximumFractionDigits: 0 }),
    );
  });

  it("says out loud when only the head of a long file is shown", async () => {
    hoisted.file = aFile({ text: "x".repeat(10), truncated: true, clean: false });
    await drawOpen();
    await openFile(button("a.md"));
    await settle();
    expect(container.textContent).toContain(t("files.cut"));
  });

  it("leaves this face when a reference in a file is followed", async () => {
    const left: number[] = [];
    hoisted.file = aFile({ text: "see AMB-T-12" });
    await drawOpen({ onOpenLedger: () => left.push(1) });
    await openFile(button("a.md"));
    await settle();
    // A reference selects on the other face. Following one from here without leaving would land on
    // a pane the reader cannot see — a link that looks alive and is not (`AMB-D-747`).
    const ref = [...container.querySelectorAll("a")].find((a) => a.textContent === "AMB-T-12");
    expect(ref).toBeDefined();
    await click(ref);
    expect(left).toEqual([1]);
  });
});
