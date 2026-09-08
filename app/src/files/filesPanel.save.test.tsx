// @vitest-environment jsdom
// Saving what a reader typed, and what a file written to underneath them does to it (`AMB-D-784`).
//
// A save carries the mark of what was read, which is what the host weighs the file against. So what
// has to be right here is that nothing is taken away from a reader who has typed, and that a writer
// who got in between the reading and the save is said out loud rather than written over.
import { act } from "react";
import { describe, expect, it } from "vitest";
import type { FolderFileDto } from "../bindings/bindings";
import {
  aFile, button, click, container, diffButton, drawOpen, hoisted, last, openFile, pressable, ROOT,
  settle, tell,
} from "./filesPanelKit";
import { type CmdError, errLabel, t } from "../core/i18n";

describe("the file face", () => {
  describe("saving what was typed", () => {
    /** Open `run.sh` in the editor, ready to be typed into. */
    async function open(about: Partial<FolderFileDto> = {}) {
      hoisted.entries[""] = [{ name: "run.sh", isDir: false, ignored: false }];
      hoisted.file = aFile({
        text: "#!/bin/sh\necho hi", encoding: "UTF-8", digest: "before", ...about,
      });
      await drawOpen();
      await openFile(button("run.sh"));
      await settle();
    }

    /** The reader typing, as the editor reports it, and then the text it now holds. */
    async function type(text: string) {
      await act(async () => {
        const drawn = container.querySelector(".cm-editor");
        if (drawn !== null) drawn.textContent = text;
        hoisted.typing?.();
        await new Promise((r) => setTimeout(r, 0));
      });
    }

    it("offers nothing to press until somebody has typed", async () => {
      await open();
      // The one control, saying which of the three it is — and there is nothing to save yet.
      expect(pressable(t("files.saved"))?.disabled).toBe(true);

      await type("#!/bin/sh\necho there");
      expect(pressable(t("files.save"))?.disabled).toBe(false);
    });

    /** What the read answered with is what the save carries back: the encoding the bytes were in,
     *  the mark the file began with, and how its lines end. The host remembers none of it between
     *  the two calls (`AMB-D-773`). */
    it("sends the text back in what the file was read in", async () => {
      await open({ encoding: "Shift_JIS", bom: true, lineEnding: "crlf" });
      await type("書き換えました");
      await click(button(t("files.save")));
      await settle();

      expect(last(hoisted.saved)).toEqual({
        path: "run.sh",
        text: "書き換えました",
        encoding: "Shift_JIS",
        bom: true,
        lineEnding: "crlf",
        // And the mark of what was read, which is what the host refuses to write over a file that
        // no longer answers to (`AMB-D-784`).
        seen: "before",
      });
      // And there is nothing left to save, which is what the control says once it is through.
      expect(pressable(t("files.saved"))?.disabled).toBe(true);
    });

    /** A file with both kinds of newline in it comes out of a save with one kind, which changes
     *  every line of the other. There is no right answer to guess at, so the reader is told and
     *  asked, and nothing is written until they have said (`AMB-D-773`). */
    it("will not save a file with both newlines until the reader picks one", async () => {
      await open({ lineEnding: "mixed" });
      expect(container.textContent).toContain(t("files.newlinesMixed"));
      await type("#!/bin/sh\necho there");
      expect(pressable(t("files.save"))?.disabled).toBe(true);

      const picker = container.querySelector<HTMLSelectElement>(".files__newline");
      expect(picker).not.toBeNull();
      await act(async () => {
        if (picker !== null) {
          picker.value = "crlf";
          picker.dispatchEvent(new Event("change", { bubbles: true }));
        }
        await new Promise((r) => setTimeout(r, 0));
      });
      await click(button(t("files.save")));
      await settle();
      expect(last(hoisted.saved)?.lineEnding).toBe("crlf");
      // Asked once: what is on the disk now has one kind, so the question is gone.
      expect(container.querySelector(".files__newline")).toBeNull();
    });

    /** A refusal is the reader's sentence, in their own language, and what they typed is still
     *  there to try again with — the file was not half written (`crate::folder_save`). */
    it("says why a save did not happen and keeps what was typed", async () => {
      await open({ encoding: "Shift_JIS" });
      hoisted.refuseSave = {
        code: "folder_unwritable_character",
        message_en: "✓ cannot be written in Shift_JIS",
        fields: { character: "✓", encoding: "Shift_JIS" },
      };
      await type("これは ✓ です");
      await click(button(t("files.save")));
      await settle();

      expect(container.textContent).toContain("✓");
      expect(container.textContent).toContain("Shift_JIS");
      expect(hoisted.saved).toEqual([]);
      // Still unsaved, so the way to try again is still there.
      expect(pressable(t("files.save"))?.disabled).toBe(false);
    });

    /** A file the panel could never write back has no way to save it at all — not a control that
     *  refuses, which would be a promise it cannot keep. */
    it("offers no way to save a file it could not write back", async () => {
      await open({ truncated: true, clean: false });
      expect(button(t("files.saved"))).toBeUndefined();
      expect(button(t("files.save"))).toBeUndefined();
    });

    /** The keystroke everything else in the world saves with, taken on the window because the
     *  reader may have clicked away from the editor — and asking what the control beside the name
     *  asks, so a page nobody has typed into is not written back over itself by a reader who
     *  pressed it out of habit. */
    it("saves on the machine's own key, and only while something is waiting", async () => {
      await open();
      await act(async () => {
        window.dispatchEvent(new KeyboardEvent("keydown", { key: "s", metaKey: true }));
        await new Promise((r) => setTimeout(r, 0));
      });
      expect(hoisted.saved).toEqual([]);

      await type("#!/bin/sh\necho there");
      await act(async () => {
        window.dispatchEvent(new KeyboardEvent("keydown", { key: "s", metaKey: true }));
        await new Promise((r) => setTimeout(r, 0));
      });
      await settle();
      expect(last(hoisted.saved)?.text).toBe("#!/bin/sh\necho there");
    });

    /** A Markdown file being drawn is not a file that cannot be written back. What a person typed is
     *  caught on the way off the editor and the rendering is drawn from it, so the offer stands over
     *  the same text either way.
     *
     *  The rendering is the form a Markdown file opens in, so a save the rendering does not carry is
     *  a file whose whole first screen says nothing about whether it has been written
     *  (`AMB-T-4560`). */
    it("keeps the save on a Markdown file while its rendering is what is on the screen", async () => {
      hoisted.entries[""] = [{ name: "notes.md", isDir: false, ignored: false }];
      hoisted.file = aFile({ text: "# a heading", encoding: "UTF-8", digest: "before" });
      await drawOpen();
      await openFile(button("notes.md"));
      await settle();
      expect(container.querySelector("h1")).not.toBeNull();
      // Nothing typed yet, and the control says so rather than being absent.
      expect(pressable(t("files.saved"))?.disabled).toBe(true);

      await click(button(t("files.edit")));
      await settle();
      await type("# what was typed");
      expect(pressable(t("files.save"))?.disabled).toBe(false);

      // Back on the rendering it still stands, over the text that was caught on the way out.
      await click(button(t("files.read")));
      await settle();
      expect(container.querySelector("h1")?.textContent).toBe("what was typed");
      expect(pressable(t("files.save"))?.disabled).toBe(false);

      // And the press writes that text, rather than the copy the disk is still on.
      await click(button(t("files.save")));
      await settle();
      expect(last(hoisted.saved)?.text).toBe("# what was typed");
      expect(pressable(t("files.saved"))?.disabled).toBe(true);
    });
  });

  /** The file moving under the reader while they have it open (`AMB-D-784`).
   *
   *  This panel sits beside an agent that edits the same files, so the case is the ordinary one and
   *  not the corner: what has to be right is that a reader looking at a file sees what it says now,
   *  and that a reader who has typed into it keeps what they typed until they say otherwise. */
  describe("a file written to while it is open", () => {
    /** Open `run.sh` with a mark on it, and the panel watching the folder for it. */
    async function open(about: Partial<FolderFileDto> = {}) {
      hoisted.entries[""] = [{ name: "run.sh", isDir: false, ignored: false }];
      hoisted.file = aFile({
        text: "#!/bin/sh\necho hi", encoding: "UTF-8", digest: "before", ...about,
      });
      await drawOpen();
      await openFile(button("run.sh"));
      await settle();
    }

    /** Somebody else writing to the file, and the host saying the folder moved. */
    async function written(text: string, digest: string) {
      hoisted.file = aFile({ text, encoding: "UTF-8", digest });
      await act(async () => {
        tell({ root: ROOT, capped: false, unwatched: false, gone: false });
        await new Promise((r) => setTimeout(r, 0));
      });
      await settle();
    }

    /** The reader typing, as the editor reports it. */
    async function type(text: string) {
      await act(async () => {
        const drawn = container.querySelector(".cm-editor");
        if (drawn !== null) drawn.textContent = text;
        hoisted.typing?.();
        await new Promise((r) => setTimeout(r, 0));
      });
    }

    /** The tree is not on the page while a file is being read, so the face reading it is what
     *  keeps a watch over the folder — without one, nothing would ever say the file had moved. */
    it("watches the folder while it is showing a file", async () => {
      await open();
      expect(hoisted.asked).toContain(`watch:1:${ROOT}`);
    });

    /** A picture is watched on the same terms and redrawn without being asked about — there is no
     *  editor over it and so nothing of the reader's to lose. What redraws it is the address, and
     *  only the address: an `<img>` handed back the URL it already has draws the copy it already
     *  has, however the file behind it moved (`AMB-D-797`). */
    it("asks for a rewritten picture at a new address", async () => {
      hoisted.entries[""] = [{ name: "chart.png", isDir: false, ignored: false }];
      hoisted.file = aFile({ image: { mime: "image/png" }, digest: "before" });
      await drawOpen();
      await openFile(button("chart.png"));
      await settle();
      expect(hoisted.asked).toContain(`watch:1:${ROOT}`);
      expect(container.querySelector("img")?.getAttribute("src")).toContain("mark=before");

      hoisted.file = aFile({ image: { mime: "image/png" }, digest: "after" });
      await act(async () => {
        tell({ root: ROOT, capped: false, unwatched: false, gone: false });
        await new Promise((r) => setTimeout(r, 0));
      });
      await settle();
      expect(container.querySelector("img")?.getAttribute("src")).toContain("mark=after");
      // And the reader is told nothing: there was nothing of theirs in the way of the redraw.
      expect(container.textContent).not.toContain(t("files.changedUnderneath"));
    });

    /** Nothing of the reader's is at stake, so the panel simply shows what the file says now. A
     *  reader looking at what the agent changed an hour ago reads it as the agent having done
     *  nothing at all. */
    it("shows what the file says now, where nobody has typed", async () => {
      await open();
      await written("#!/bin/sh\necho the agent was here", "after");
      expect(last(hoisted.shown)).toContain("the agent was here");
      expect(container.querySelector(".cm-editor")?.textContent).toContain("the agent was here");
      expect(container.textContent).not.toContain(t("files.changedUnderneath"));
    });

    /** The word carries no rows, so every move of the folder brings the panel back to read the
     *  file — and the mark is what tells "this file moved" from "something else in the folder
     *  did", which is most of what arrives. */
    it("draws nothing where the folder moved and this file did not", async () => {
      await open();
      hoisted.shown = [];
      await written("#!/bin/sh\necho hi", "before");
      expect(hoisted.shown).toEqual([]);
    });

    /** What a reader has typed is theirs. They are told, and reading the file again is a thing they
     *  ask for — which is the one thing here that loses somebody's work. */
    it("tells a reader who has typed, and takes nothing away from them", async () => {
      await open();
      await type("#!/bin/sh\necho mine");
      await written("#!/bin/sh\necho theirs", "after");

      expect(container.textContent).toContain(t("files.changedUnderneath"));
      expect(container.querySelector(".cm-editor")?.textContent).toContain("mine");

      await click(button(t("files.readAgain")));
      await settle();
      expect(container.querySelector(".cm-editor")?.textContent).toContain("theirs");
      expect(container.textContent).not.toContain(t("files.changedUnderneath"));
      // And there is nothing of theirs left unsaved, so the control says so.
      expect(pressable(t("files.saved"))?.disabled).toBe(true);
    });

    /** Taking what the disk says is the other press that loses what a reader typed, so it is asked
     *  first — and a no leaves them exactly where they were: their text in the editor, and the news
     *  still on the screen (`AMB-T-4561`). */
    it("asks before it takes the disk's text, and keeps theirs on a no", async () => {
      await open();
      await type("#!/bin/sh\necho mine");
      await written("#!/bin/sh\necho theirs", "after");

      hoisted.answers = [false];
      await click(button(t("files.readAgain")));
      await settle();
      expect(hoisted.confirmed).toEqual([t("files.readAgainConfirm")]);
      expect(container.querySelector(".cm-editor")?.textContent).toContain("mine");
      expect(container.textContent).toContain(t("files.changedUnderneath"));

      // And the press is still there to make: a question answered no is not an offer withdrawn.
      await click(button(t("files.readAgain")));
      await settle();
      expect(container.querySelector(".cm-editor")?.textContent).toContain("theirs");
    });

    /** The belt behind the watch: a move the panel never heard about is still refused at the door,
     *  and what the reader gets is the same offer rather than a sentence to read. The press is
     *  answered once — by the offer appearing — and the control shuts behind it, because a second
     *  press would go to the same door for the same refusal. */
    it("says so when the save is the thing that finds out", async () => {
      await open();
      hoisted.refuseSave = {
        code: "folder_changed_underneath",
        message_en: "somebody wrote to this file after it was read here",
        fields: {},
      };
      await type("#!/bin/sh\necho mine");
      await click(button(t("files.save")));
      await settle();

      expect(container.textContent).toContain(t("files.changedUnderneath"));
      expect(hoisted.saved).toEqual([]);
      // What they typed is still theirs and still unsaved — the word on the control says so.
      expect(pressable(t("files.save"))?.disabled).toBe(true);
    });

    /** The press a reader actually makes: they were told the file moved, and they press save
     *  anyway. The panel is already holding the mark the door refuses, so an open control took the
     *  press, spent a round trip on a refusal it could see coming, and put them back on the screen
     *  they were already reading with nothing said about any of it (`AMB-T-4401`). */
    it("stops offering a save it cannot take, once it has said the file moved", async () => {
      await open();
      await type("#!/bin/sh\necho mine");
      await written("#!/bin/sh\necho theirs", "after");
      expect(container.textContent).toContain(t("files.changedUnderneath"));

      expect(pressable(t("files.save"))?.disabled).toBe(true);
      await click(button(t("files.save")));
      await settle();
      expect(hoisted.saved).toEqual([]);

      // And it is shut rather than dead: the offer beside it is the way back to a save that works.
      await click(button(t("files.readAgain")));
      await settle();
      await type("#!/bin/sh\necho theirs, and mine");
      await click(button(t("files.save")));
      await settle();
      expect(last(hoisted.saved)?.seen).toBe("after");
    });

    /** The other answer, and the one there was no way to give before: the reader keeps what they
     *  typed and the file is written over. The mark the panel is holding is the one the door
     *  refuses, so the file is read again for the mark it answers to now and the save carries that
     *  (`AMB-D-863`). */
    it("writes the reader's text over the file, on the mark it answers to now", async () => {
      await open();
      hoisted.keptDigest = "mine";
      await type("#!/bin/sh\necho mine");
      await written("#!/bin/sh\necho theirs", "after");
      expect(container.textContent).toContain(t("files.changedUnderneath"));

      await click(button(t("files.keepMine")));
      await settle();
      expect(last(hoisted.saved)).toMatchObject({ text: "#!/bin/sh\necho mine", seen: "after" });
      // What they typed is on the disk, so there is nothing left of the news or of the offer, and
      // the editor was never handed a document of somebody else's.
      expect(container.textContent).not.toContain(t("files.changedUnderneath"));
      expect(container.querySelector(".cm-editor")?.textContent).toContain("mine");
      expect(pressable(t("files.saved"))?.disabled).toBe(true);

      // And the file is known by what this save wrote: the folder moving because of it says
      // nothing, and the next save answers to that mark.
      await written("#!/bin/sh\necho mine", "mine");
      expect(container.textContent).not.toContain(t("files.changedUnderneath"));
      await type("#!/bin/sh\necho mine again");
      await click(button(t("files.save")));
      await settle();
      expect(last(hoisted.saved)?.seen).toBe("mine");
    });

    /** Somebody writing to the file in the moment between that reading and the save. The press
     *  wrote nothing, and a press answered by the same screen it was made on says nothing at all —
     *  so the refusal is the sentence, and the offer stays where it is. */
    it("says so when a writer got in between the reading and the save", async () => {
      await open();
      await type("#!/bin/sh\necho mine");
      await written("#!/bin/sh\necho theirs", "after");
      hoisted.refuseSave = {
        code: "folder_changed_underneath",
        message_en: "somebody wrote to this file after it was read here",
        fields: {},
      };
      await click(button(t("files.keepMine")));
      await settle();

      expect(hoisted.saved).toEqual([]);
      expect(container.textContent).toContain(errLabel(hoisted.refuseSave as CmdError));
      // Theirs is still theirs, and both ways out are still on the screen.
      expect(container.querySelector(".cm-editor")?.textContent).toContain("mine");
      expect(button(t("files.keepMine"))).toBeDefined();
      expect(button(t("files.readAgain"))).toBeDefined();
    });

    /** Before either answer there is a question a reader cannot settle from the notice alone: what
     *  actually differs. The screen is opened on the two texts — the disk as it stands now, read at
     *  the press, and what is in the editor — and carries both answers, so nobody chooses from
     *  memory after closing it (`AMB-D-863`). */
    it("puts the two texts side by side, and answers from there", async () => {
      await open();
      hoisted.keptDigest = "mine";
      await type("#!/bin/sh\necho mine");
      await written("#!/bin/sh\necho theirs", "after");

      await click(button(t("files.seeDifference")));
      await settle();
      expect(last(hoisted.compared)).toEqual({
        theirs: "#!/bin/sh\necho theirs",
        mine: "#!/bin/sh\necho mine",
      });
      // The screen names which side is which — the whole choice would otherwise be guessed from
      // which of the two texts looks familiar.
      expect(document.body.textContent).toContain(t("files.diffTheirs"));
      expect(document.body.textContent).toContain(t("files.diffMine"));

      // And the answer is on the screen the reader is looking at.
      await click(diffButton(t("files.keepMine")));
      await settle();
      expect(last(hoisted.saved)).toMatchObject({ text: "#!/bin/sh\necho mine", seen: "after" });
      expect(document.querySelector(".filediff")).toBeNull();
      expect(container.textContent).not.toContain(t("files.changedUnderneath"));
    });

    /** The same screen, opened from where a Markdown file actually sits when a reader comes back to
     *  it: the rendering, with no editor on the page to ask for the text. A Markdown file opens on
     *  its rendering every time — `asText` goes back with every open — so this is the state a reader
     *  reaches by leaving the tab and returning, and asking the editor alone left the one control
     *  that answers what changed doing nothing at all there (`AMB-T-4573`). */
    it("puts the two texts side by side from the rendering too", async () => {
      hoisted.entries[""] = [{ name: "notes.md", isDir: false, ignored: false }];
      hoisted.file = aFile({ text: "# a heading", encoding: "UTF-8", digest: "before" });
      await drawOpen();
      await openFile(button("notes.md"));
      await settle();

      await click(button(t("files.edit")));
      await settle();
      await type("# what was typed");
      // Back on the rendering, which is the editor leaving the page and taking its text with it.
      await click(button(t("files.read")));
      await settle();
      expect(container.querySelector(".cm-editor")).toBeNull();

      await written("# theirs", "after");
      await click(button(t("files.seeDifference")));
      await settle();
      expect(last(hoisted.compared)).toEqual({ theirs: "# theirs", mine: "# what was typed" });
    });

    /** The other answer, from the same screen: the reader looked, and the disk's text is the one
     *  they want. It is the one press here that loses what they typed, which is why it is a press
     *  and not something the panel does for them. */
    it("takes the disk's text from the screen too", async () => {
      await open();
      await type("#!/bin/sh\necho mine");
      await written("#!/bin/sh\necho theirs", "after");
      await click(button(t("files.seeDifference")));
      await settle();

      await click(diffButton(t("files.readAgain")));
      await settle();
      expect(document.querySelector(".filediff")).toBeNull();
      expect(container.querySelector(".cm-editor")?.textContent).toContain("theirs");
      expect(container.textContent).not.toContain(t("files.changedUnderneath"));
    });

    /** The same question from the screen the two texts are on, and the screen stays up behind it: a
     *  reader who says no is one still weighing the two, and taking the comparison away would make
     *  them open it again to answer (`AMB-T-4561`). */
    it("keeps the two texts up where the question was answered no", async () => {
      await open();
      await type("#!/bin/sh\necho mine");
      await written("#!/bin/sh\necho theirs", "after");
      await click(button(t("files.seeDifference")));
      await settle();

      hoisted.answers = [false];
      await click(diffButton(t("files.readAgain")));
      await settle();
      expect(hoisted.confirmed).toEqual([t("files.readAgainConfirm")]);
      expect(document.querySelector(".filediff")).not.toBeNull();
      expect(container.querySelector(".cm-editor")?.textContent).toContain("mine");
    });

    /** Closing it changes nothing: it is a screen to read, and the file it was opened over is still
     *  in the state the reader was told about. */
    it("leaves the news where it was when the screen is closed", async () => {
      await open();
      await type("#!/bin/sh\necho mine");
      await written("#!/bin/sh\necho theirs", "after");
      await click(button(t("files.seeDifference")));
      await settle();

      await click(diffButton(t("files.diffClose")));
      await settle();
      expect(document.querySelector(".filediff")).toBeNull();
      expect(hoisted.saved).toEqual([]);
      expect(container.querySelector(".cm-editor")?.textContent).toContain("mine");
      expect(container.textContent).toContain(t("files.changedUnderneath"));
    });

    /** A save answers with the mark of what it wrote, and the panel takes it: without that, the
     *  panel's own writing would come back as the folder having moved and be read as somebody
     *  else's. */
    it("knows the file by what its own save wrote", async () => {
      await open();
      await type("#!/bin/sh\necho mine");
      await click(button(t("files.save")));
      await settle();
      // The folder moves because of that very save, and the file answers to the new mark.
      await written("#!/bin/sh\necho mine", "after");
      expect(container.textContent).not.toContain(t("files.changedUnderneath"));

      await type("#!/bin/sh\necho mine again");
      await click(button(t("files.save")));
      await settle();
      expect(last(hoisted.saved)?.seen).toBe("after");
    });
  });


});
