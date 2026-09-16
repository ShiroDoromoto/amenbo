// @vitest-environment jsdom
// What git can be told about a row, from the menu that row carries.
//
// What has to be right here is which of the four items a row is offered. Three of them are about
// the repository and stand over any row it knows; the fourth throws away something git has not
// recorded, and is offered only where git says there is something to throw away — it is the one
// press in the window that nothing undoes (`AMB-D-906`, `AMB-D-777`).
import { describe, expect, it } from "vitest";
import {
  anyButton, button, click, container, drawOpen, hoisted, menuOn, openFile, ROOT, settle,
} from "./filesPanelKit";
import { t } from "../core/i18n";

/** The rows the tree draws, and what git says about them. */
function folderHolds() {
  hoisted.entries = {
    "": [
      { name: "changed.md", isDir: false, ignored: false },
      { name: "recorded.md", isDir: false, ignored: false },
    ],
  };
  hoisted.git[ROOT] = [
    // git's `Y` is what the working tree says, and something other than a space there is the whole
    // of what there is to throw away.
    { path: ["changed.md"], index: " ", worktree: "M", isDir: false },
  ];
}

describe("what git can be told about a row of the tree", () => {
  it("offers the three that are about the repository over any row", async () => {
    folderHolds();
    await drawOpen({ onHistory: () => {} });
    await menuOn(button("recorded.md"));
    expect(button(t("git.fileHistory"))).toBeDefined();
    expect(button(t("git.ignore"))).toBeDefined();
    expect(button(t("git.untrack"))).toBeDefined();
  });

  /// There is nowhere for a history to open where nothing was handed down to open it, and an item
  /// that answers nothing is worse than one that is not there.
  it("offers no history where there is nowhere for one to open", async () => {
    folderHolds();
    await drawOpen();
    await menuOn(button("recorded.md"));
    expect(button(t("git.fileHistory"))).toBeUndefined();
    expect(button(t("git.ignore"))).toBeDefined();
  });

  /// Spelled from the folder the row is in, which is the folder that road is run in — so it is read
  /// as written (`crate::folder_git`).
  it("asks for the history in the folder's own spelling", async () => {
    hoisted.entries = { "": [{ name: "src", isDir: true, ignored: false }], src: [
      { name: "lib.rs", isDir: false, ignored: false },
    ] };
    const asked: string[] = [];
    await drawOpen({ onHistory: (path: string) => asked.push(path) });
    await click(button("src"));
    await settle();
    await menuOn(button("lib.rs"));
    await click(button(t("git.fileHistory")));
    expect(asked).toEqual(["src/lib.rs"]);
  });

  /// A row git says nothing about has nothing to lose, and the item that throws changes away would
  /// be a press that does nothing — which is the last press to offer idly.
  it("offers throwing changes away only where git says there are some", async () => {
    folderHolds();
    await drawOpen();
    await menuOn(button("recorded.md"));
    expect(button(t("git.restore"))).toBeUndefined();

    await menuOn(button("changed.md"));
    expect(button(t("git.restore"))).toBeDefined();
  });

  it("writes a row into the folder's own ignore file", async () => {
    folderHolds();
    await drawOpen();
    await menuOn(button("recorded.md"));
    await click(button(t("git.ignore")));
    expect(hoisted.asked).toContain(`ignore:${ROOT}:recorded.md`);
  });

  it("stops git following a row without asking, because writing it down puts it back", async () => {
    folderHolds();
    await drawOpen();
    await menuOn(button("recorded.md"));
    await click(button(t("git.untrack")));
    expect(hoisted.asked).toContain(`untrack:${ROOT}:recorded.md`);
    expect(anyButton(t("git.restoreGo"))).toBeUndefined();
  });

  /// The one road nothing walks back, so the question stands in front of it — and answering no
  /// leaves the change where it is.
  it("asks before throwing changes away, and asks nothing of git where the answer is no", async () => {
    folderHolds();
    await drawOpen();
    await menuOn(button("changed.md"));
    await click(button(t("git.restore")));
    await settle();
    expect(container.textContent + document.body.textContent).toContain(t("git.restoreGone"));
    expect(hoisted.asked.some((one) => one.startsWith("restore:"))).toBe(false);

    await click(anyButton(t("git.restoreKeep")));
    expect(hoisted.asked.some((one) => one.startsWith("restore:"))).toBe(false);

    await menuOn(button("changed.md"));
    await click(button(t("git.restore")));
    await settle();
    await click(anyButton(t("git.restoreGo")));
    await settle();
    expect(hoisted.asked).toContain(`restore:${ROOT}:changed.md`);
  });

  /// git's own sentence, word for word: what reaches the reader is what they would get in a
  /// terminal, not a rewriting of it (`AMB-D-906`).
  it("puts git's refusal in front of the reader as git wrote it", async () => {
    folderHolds();
    hoisted.refuseRestore = { code: "", message_en: "error: pathspec did not match any file" };
    await drawOpen();
    await menuOn(button("changed.md"));
    await click(button(t("git.restore")));
    await settle();
    await click(anyButton(t("git.restoreGo")));
    await settle();
    expect(container.textContent).toContain("error: pathspec did not match any file");
  });

  /// The bound folder itself is the binding, and none of these is a thing to say about a binding.
  it("says nothing about the bound folder's own row", async () => {
    folderHolds();
    await drawOpen({ onHistory: () => {} });
    await menuOn(button(t("files.tree")));
    expect(button(t("git.fileHistory"))).toBeUndefined();
    expect(button(t("git.ignore"))).toBeUndefined();
    expect(button(t("git.restore"))).toBeUndefined();
  });
});

/**
 * The same item, on the file being read.
 *
 * The menu on this side is opened from the way out of a file the column cannot draw — a picture's
 * bytes, or a file too big to put on the screen (`./FilesPanel`). That is the reader who most needs
 * it: what the editor cannot show, the editor cannot be used to put back.
 */
describe("what git can be told about the file being read", () => {
  /** A folder of two files, one of them changed, and a read that comes back as nothing to draw —
   *  which is what puts the way out on the screen. */
  function reading() {
    hoisted.entries = {
      "": [
        { name: "changed.png", isDir: false, ignored: false },
        { name: "recorded.png", isDir: false, ignored: false },
      ],
    };
    hoisted.git[ROOT] = [{ path: ["changed.png"], index: " ", worktree: "M", isDir: false }];
  }

  it("offers throwing changes away only where git says there are some", async () => {
    reading();
    await drawOpen();
    await openFile(button("recorded.png"));
    await click(button(t("files.openElsewhere")));
    expect(button(t("git.restore"))).toBeUndefined();

    await drawOpen();
    await openFile(button("changed.png"));
    await click(button(t("files.openElsewhere")));
    expect(button(t("git.restore"))).toBeDefined();
  });

  /// The file on the screen and never what is picked out in the rail: this column is about one
  /// file, and a press made here is about the one being read.
  it("asks git about the file being read, after the question is answered", async () => {
    reading();
    await drawOpen();
    await openFile(button("changed.png"));
    await click(button(t("files.openElsewhere")));
    await click(button(t("git.restore")));
    await settle();
    expect(hoisted.asked.some((one) => one.startsWith("restore:"))).toBe(false);

    await click(anyButton(t("git.restoreGo")));
    await settle();
    expect(hoisted.asked).toContain(`restore:${ROOT}:changed.png`);
  });

  /// A folder that is no repository answers with nothing, which is a menu without that item rather
  /// than a menu that failed to draw.
  it("offers nothing to throw away where git says nothing about the folder", async () => {
    reading();
    delete hoisted.git[ROOT];
    await drawOpen();
    await openFile(button("changed.png"));
    await click(button(t("files.openElsewhere")));
    expect(button(t("git.restore"))).toBeUndefined();
    expect(button(t("git.ignore"))).toBeDefined();
  });
});
