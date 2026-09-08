// @vitest-environment jsdom
// The files the column is holding: the tabs over them, and what each one that is not on top keeps.
//
// What a reader typed belongs to the face around the column rather than to the file being drawn,
// because the column draws one of them and the rest are off the screen — so it is given back when
// the tab comes up again, and it goes when the tab does.
import { act } from "react";
import { describe, expect, it } from "vitest";
import {
  aFile, button, click, container, drawOpen, hoisted, last, openFile, pressable, ROOT, settle,
} from "./filesPanelKit";
import { t, tf } from "../core/i18n";

describe("the file face", () => {
  describe("the files the column is holding", () => {
    const tabs = () => [...container.querySelectorAll<HTMLElement>(".files__tabname")]
      .map((one) => one.textContent);
    const onTop = () =>
      container.querySelector<HTMLElement>('.files__tabname[aria-current="true"]')?.textContent
        ?? null;
    const twoOpen = async () => {
      hoisted.entries[""] = [
        { name: "a.md", isDir: false, ignored: false },
        { name: "b.md", isDir: false, ignored: false },
      ];
      await drawOpen();
      await openFile(button("a.md"));
      await settle();
      await openFile(button("b.md"));
      await settle();
    };

    it("keeps the first open when the second is, and puts the new one on top", async () => {
      await twoOpen();
      // The draft page is the first tab and is always there (`./MemoPage`).
      expect(tabs()).toEqual([t("files.memo"), "a.md", "b.md"]);
      expect(onTop()).toBe("b.md");
      expect(hoisted.asked).toContain(`read:${ROOT}:b.md`);
    });

    it("brings one back up from its tab", async () => {
      await twoOpen();
      await click(container.querySelectorAll<HTMLElement>(".files__tabname")[1]);
      await settle();
      expect(onTop()).toBe("a.md");
      // Both are still held: pressing a tab is choosing between them, not closing one.
      expect(tabs()).toEqual([t("files.memo"), "a.md", "b.md"]);
    });

    it("lets one go from its own cross, and stands on the tab beside it", async () => {
      await twoOpen();
      await click(container.querySelectorAll<HTMLElement>(".files__tabclose")[1]);
      await settle();
      expect(tabs()).toEqual([t("files.memo"), "a.md"]);
      expect(onTop()).toBe("a.md");
    });

    it("says nothing is open once the last of them has gone", async () => {
      await twoOpen();
      await click(container.querySelectorAll<HTMLElement>(".files__tabclose")[1]);
      await settle();
      await click(container.querySelectorAll<HTMLElement>(".files__tabclose")[0]);
      await settle();
      expect(container.querySelector(".termface__column--side .files__none")?.textContent)
        .toBe(t("files.nothingOpen"));
    });

    /** What a file that is not on top is holding (`AMB-D-835`).
     *
     *  One file is drawn at a time, so moving between tabs takes a file off the screen with the
     *  text a person typed into it — the editor is where that text is, and the editor goes with the
     *  file. What is caught on the way out is handed to the face and given back when the tab comes
     *  up again, so the row of tabs is a row a reader can walk without counting the cost.
     *
     *  Driven on files this panel can write back, because the save is the point of keeping the
     *  text: one that came back on the screen and not into the file would be half a promise. */
    describe("what a file that is not on top is holding", () => {
      /** Two writable files open, `b.sh` on top and the editor on it. */
      const twoWritable = async () => {
        hoisted.entries[""] = [
          { name: "a.sh", isDir: false, ignored: false },
          { name: "b.sh", isDir: false, ignored: false },
        ];
        hoisted.file = aFile({ text: "echo hi", encoding: "UTF-8", digest: "before" });
        await drawOpen();
        await openFile(button("a.sh"));
        await settle();
        await openFile(button("b.sh"));
        await settle();
      };

      /** The reader typing, as the stand-in editor reports it. */
      async function typeInto(text: string) {
        await act(async () => {
          const drawn = container.querySelector(".cm-editor");
          if (drawn !== null) drawn.textContent = text;
          hoisted.typing?.();
          await new Promise((r) => setTimeout(r, 0));
        });
      }

      /** What the editor is drawing, which is what a reader coming back to a tab sees. */
      const inEditor = () => container.querySelector(".cm-editor")?.textContent;

      it("gives it back when the tab comes up again, still unsaved", async () => {
        await twoWritable();
        await typeInto("echo mine");

        await click(tabFor("a.sh"));
        await settle();
        // The other file is its own: what one tab is holding is not drawn over another.
        expect(inEditor()).toBe("echo hi");

        await click(tabFor("b.sh"));
        await settle();
        expect(inEditor()).toBe("echo mine");
        // And the one control still says there is something to save, because there is.
        expect(pressable(t("files.save"))?.disabled).toBe(false);
        await click(button(t("files.save")));
        await settle();
        expect(last(hoisted.saved)?.text).toBe("echo mine");
      });

      // The draft page is a tab like the others, and the file goes off the screen the same way.
      it("gives it back after the draft page has been up", async () => {
        await twoWritable();
        await typeInto("echo mine");

        await click(container.querySelectorAll<HTMLElement>(".files__tabname")[0]);
        await settle();
        await click(tabFor("b.sh"));
        await settle();
        expect(inEditor()).toBe("echo mine");
      });

      /** Closing is the reader saying they are finished with the file, and what they typed goes
       *  with it: opening it again is opening the file, and a file that came up on text from
       *  before it was closed would be one nobody could get back to what it says. */
      it("lets it go with the tab", async () => {
        await twoWritable();
        await typeInto("echo mine");
        await click(container.querySelectorAll<HTMLElement>(".files__tabclose")[1]);
        await settle();

        await openFile(button("b.sh"));
        await settle();
        expect(inEditor()).toBe("echo hi");
      });

      /** The file is read afresh when the tab comes back, so this is the first sight of it since
       *  the reader moved off — and a file somebody wrote to in between is one they have to be
       *  asked about, exactly as if they had been looking at it the whole time (`AMB-D-784`). */
      it("comes back on the offer where the file moved while it was away", async () => {
        await twoWritable();
        await typeInto("echo mine");
        await click(tabFor("a.sh"));
        await settle();

        hoisted.file = aFile({ text: "echo theirs", encoding: "UTF-8", digest: "after" });
        await click(tabFor("b.sh"));
        await settle();

        expect(inEditor()).toBe("echo mine");
        expect(container.textContent).toContain(t("files.changedUnderneath"));
        // Shut, because a save from here would write over a writer nobody was told about.
        expect(pressable(t("files.save"))?.disabled).toBe(true);
      });
    });

    /** One file's tab, by the name on it. */
    const tabFor = (name: string) =>
      [...container.querySelectorAll<HTMLElement>(".files__tabname")]
        .find((one) => one.textContent === name);

    /** Which tab is drawn with the mark saying it is holding something not on the disk, by name. */
    const marked = () => [...container.querySelectorAll<HTMLElement>(".files__tab")]
      .filter((one) => one.querySelector(".files__unsaved") !== null)
      .map((one) => one.querySelector(".files__tabname")?.textContent);

    /** Two files open that go straight to the editor, the second of them on top. */
    const twoTextOpen = async () => {
      hoisted.entries[""] = [
        { name: "a.txt", isDir: false, ignored: false },
        { name: "b.txt", isDir: false, ignored: false },
      ];
      hoisted.file = aFile({ text: "echo hi", encoding: "UTF-8", digest: "before" });
      await drawOpen();
      await openFile(button("a.txt"));
      await settle();
      await openFile(button("b.txt"));
      await settle();
    };

    /** The reader typing into the file that is up, as the stand-in editor reports it. */
    const typeIn = async (text: string) => {
      await act(async () => {
        const drawn = container.querySelector(".cm-editor");
        if (drawn !== null) drawn.textContent = text;
        hoisted.typing?.();
        await new Promise((r) => setTimeout(r, 0));
      });
    };

    /** The state of a file the panel holds and the disk does not, said on the tab rather than only
     *  on the save control — which is the one place it was said before, and which a reader on a
     *  Markdown rendering could not see at all (`AMB-T-4560`). */
    it("marks the tab of the file something was typed into", async () => {
      await twoTextOpen();
      expect(marked()).toEqual([]);

      await typeIn("echo there");
      // Named by the file rather than by whichever tab is on: the mark belongs to the text.
      expect(marked()).toEqual(["b.txt"]);
    });

    it("takes the mark off once the file has been written", async () => {
      await twoTextOpen();
      await typeIn("echo there");
      await click(button(t("files.save")));
      await settle();
      expect(marked()).toEqual([]);
    });

    /** What a person types goes off the screen with the file and is held for them (`Typed`), so
     *  the mark goes with it rather than out: a tab with something to lose says so whether or not
     *  the reader is standing on it. */
    it("keeps the mark on a file that left the screen holding something", async () => {
      await twoTextOpen();
      await typeIn("echo there");
      await click(button(t("files.memo")));
      await settle();
      expect(marked()).toEqual(["b.txt"]);

      // And on the file itself, not on whichever tab the reader went to.
      await click(tabFor("a.txt"));
      await settle();
      expect(marked()).toEqual(["b.txt"]);
    });

    /** Closing the tab is the press that throws what was typed away (`FilesPanel`), so it asks —
     *  and the question names the file, because several tabs can be holding something at once and
     *  the cross that was pressed is the only thing saying which (`AMB-T-4561`). */
    it("asks before it closes a tab holding something unsaved", async () => {
      await twoTextOpen();
      await typeIn("echo there");

      hoisted.answers = [false];
      await click(container.querySelectorAll<HTMLElement>(".files__tabclose")[1]);
      await settle();
      expect(hoisted.confirmed).toEqual([tf("files.closeConfirm", { name: "b.txt" })]);
      // Answered no, so the file is still open and still holding what was typed.
      expect(tabFor("b.txt")).toBeDefined();
      expect(marked()).toEqual(["b.txt"]);

      await click(container.querySelectorAll<HTMLElement>(".files__tabclose")[1]);
      await settle();
      expect(tabFor("b.txt")).toBeUndefined();
    });

    /** The row above the file being read closes the same file, so it asks the same question: which
     *  control a reader reached for is no reason to be warned on one road and not the other. */
    it("asks on the way out of the file being read, too", async () => {
      await twoTextOpen();
      await typeIn("echo there");

      hoisted.answers = [false];
      await click(button(t("files.closeFile")));
      await settle();
      expect(hoisted.confirmed).toEqual([tf("files.closeConfirm", { name: "b.txt" })]);
      expect(tabFor("b.txt")).toBeDefined();
    });

    /** A file that has been written has nothing to lose by being closed. A question there is one
     *  with a single sensible answer, which is a press a reader learns to make without reading. */
    it("closes a file with nothing unsaved without a word", async () => {
      await twoTextOpen();
      await click(container.querySelectorAll<HTMLElement>(".files__tabclose")[1]);
      await settle();
      expect(hoisted.confirmed).toEqual([]);
      expect(tabFor("b.txt")).toBeUndefined();
    });

    // The row scrolls rather than paging, so the tab that is off the end of it is reached by name.
    it("lists everything it is holding, by name", async () => {
      await twoOpen();
      await click(container.querySelector<HTMLElement>(".files__more"));
      await settle();
      expect([...container.querySelectorAll<HTMLElement>('[role="menuitem"]')]
        .map((one) => one.textContent)).toEqual(["a.md", "b.md"]);

      await click([...container.querySelectorAll<HTMLElement>('[role="menuitem"]')][0]);
      await settle();
      expect(onTop()).toBe("a.md");
    });
  });

});
