// The menu a row carries: what the machine can do with the paths a press is about — open them where
// the machine would, show them where they live, hand them to the pane being worked in, put them in
// the bin, or say something about them to git.
//
// **One menu wherever a row names a file** (`AMB-D-835`). It is opened on a row of the tree in the
// rail, on a changed path in the rail's git half, on a path inside a commit, and on the file being
// read on the other side of the panes — and what it acts on is paths, which are the same thing
// whichever of them named it.
//
// **What git can be told about a row is the caller's to say, not this menu's to work out.** Whether
// there is a change to throw away, whether a path can be un-followed at all, and where a history
// would open are answers each of those rows has and this box does not — so they arrive as the
// handful of callbacks below, and an item with nothing behind it is not drawn.
import { useState } from "react";
import type { FolderAppDto } from "../bindings/bindings";
import { Menu, MenuItem } from "../components/Menu";
import { t, tf } from "../core/i18n";
import {
  folderClipCopy, folderOpenFile, folderOpenFileWith, folderOpenWith, folderRevealFile,
} from "./folder";
import { fileAt } from "./fileUnder";

/**
 * Do one thing to each of the rows an act is about, in the order they were given.
 *
 * One after another rather than all at once: what is on the other side of these is the machine —
 * applications being started, a file manager being brought to the front — and a handful of those
 * asked for together arrive in whatever order the OS gets to them. The first refusal ends it, the
 * same way the bin's own list does (`crate::trash`): a caller who could not open the second file
 * has nothing to gain from opening the fifth.
 */
async function eachOf(paths: string[][], go: (path: string[]) => Promise<void>): Promise<void> {
  for (const one of paths) await go(one);
}

/**
 * One row per folder, in the order they were given.
 *
 * Showing a row where it lives is showing the folder holding it, and the machine's file manager
 * selects one row at a time: five rows of one folder asked for one after another would be five
 * presses fighting over the same window, each undoing the one before it. One press per folder is
 * what "show these where they live" comes to.
 */
function oncePerFolder(paths: string[][]): string[][] {
  const seen = new Set<string>();
  return paths.filter((one) => {
    const dir = one.slice(0, -1).join("/");
    if (seen.has(dir)) return false;
    seen.add(dir);
    return true;
  });
}

/**
 * What can be done with a file that is not reading it here: hand it to the machine.
 *
 * **This is the items, not the box.** Where the menu sits, what closes it, how the arrows walk it
 * and where the focus goes when it leaves are the shell's (`../components/Menu`), because the pane
 * rows wear the same one and two of them would be a pair that drifts.
 *
 * All three roads out are the OS's own — the application the reader already opens that kind of file
 * with, one they pick for this file alone, and the file manager they already keep their folders in.
 * None of them is a choice Amenbo makes or remembers (`AMB-T-3605`).
 *
 * **Picking one is two shapes behind a single item.** Where the operating system has a chooser of
 * its own it draws it and the file is open before anything comes back; where it has none — macOS —
 * the applications come back and are drawn here (`crate::open_with`). Which of those happened is
 * read off the answer and nothing else: a list to draw, or nothing to draw. That is why the menu
 * stays open across the call rather than closing on the click, and why an empty answer closes it.
 *
 * A failure is not drawn. What could go wrong is the file having gone since the row was drawn, and
 * the row itself is about to say so: the folder is watched, and a file that is not there stops being
 * listed. A line about it would be a second, slower account of the same fact.
 *
 * **What a folder can be handed to is nothing, so a folder is offered none of it.** The three doors
 * are about a file's own kind, and a menu that offered them over a folder would be offering to open a
 * directory in a text editor. What is left over a folder is what can be written into it.
 */
/**
 * What git can be told about the rows a menu is about.
 *
 * **What the row is decides what is drawn; what the set is decides what can be pressed**
 * (`AMB-D-906`, 2-8). A path inside a commit has no change to throw away and a row whose face
 * cannot open a history has nowhere to open one, so neither item is there at all. But a door the
 * rows are merely not all in a state for is drawn and greyed: rows are picked out several at a
 * time, a set mixing what git follows with what it has never seen is ordinary, and a menu whose
 * items came and went with the selection would be one a reader could not learn.
 */
export type GitDoors = {
  /** Open the history of this row alone, in the column across the panes (`./GitHistory`). Handed
   *  the path as this row's own folder spells it, which is what that road takes. */
  onHistory?: (path: string) => void;
  /** Write these rows into the bound folder's own `.gitignore`. */
  onIgnore?: () => void;
  /** Stop git following these rows, leaving them on disk. */
  onUntrack?: () => void;
  /**
   * Whether git follows every row the menu is about.
   *
   * `false` greys the door above rather than taking it away: `git rm --cached` refuses a pathspec
   * naming a path git has never seen, and refuses the whole of it — so over five rows with one
   * untracked among them the press would fail for all five. Absent where the caller answers for one
   * row and has no set to judge.
   */
  followsAll?: boolean;
  /** Throw away what the working tree has done to them — asked about first (`./restore`). Absent
   *  where there is nothing to throw away, which is every row inside a commit. */
  onRestore?: () => void;
  /**
   * Take one whole side of a conflict — the reader's own branch, or the one being brought in.
   *
   * **Drawn on a conflicted row and nowhere else** (`AMB-D-906`, 2-7). Over any other path git
   * refuses it outright, and the pair is a short way round for the files where one side is the
   * whole answer — something built from something else, a lock file — rather than the way a
   * conflict is normally settled, which is in the file itself.
   */
  onTake?: (mine: boolean) => void;
};

export function FileMenu({
  projectId, root, path, about, dir, at, naming, onClose, onTrash, onHandOver, git,
}: {
  projectId: number;
  root: string;
  /** The row the menu was opened on — what the menu is drawn from, and what it asks the host about
   *  where a question is about one row (`folderOpenWith`). */
  path: string[];
  /**
   * The rows every door here acts on: `path` alone, or the rows a reader picked out where this one
   * is among them (`FilesPanel`).
   *
   * **What the menu is drawn from is still the one row.** Whether it is a folder decides which
   * doors there are, and a menu whose shape changed with what else was picked out would be a menu
   * a reader could not learn.
   *
   * The doors that only make sense one at a time — writing a new name into a folder, renaming, and
   * one path's history — are drawn over five rows and greyed. A rename of five rows is one press
   * that would have to be refused afterwards; an item that vanished as the fifth row was picked out
   * is a menu that changes shape under the reader.
   */
  about: string[][];
  /** Whether the row is a folder. It decides the whole of what the menu holds. */
  dir: boolean;
  at: { x: number; y: number };
  /**
   * Naming, where the menu was opened on a row in the tree. The reading face opens this same menu
   * with no row under the pointer (`FileReader`) and passes none, so the doors are all it draws.
   * `onRename` is absent for the one row that has no name of its own — the bound folder.
   */
  naming?: { onMake: (dir: boolean) => void; onRename: (() => void) | null };
  onClose: () => void;
  /** Send this row to the machine's bin — asked about first, unless the reader turned that off. */
  onTrash: () => void;
  /**
   * Hand the file to the pane the reader is working in, as the whole path it is at
   * (`../shell/WorkspaceFace`).
   *
   * **Absent where there is no pane to hand it to**, and then the item is not drawn: the panel is
   * open beside a workspace with nothing running in it as readily as beside one with four, and
   * an item that answers nothing is worse than an item that is not there.
   */
  onHandOver?: (wholes: string[]) => void;
  /**
   * What git can be told about this row, where the row's own face can answer for it.
   *
   * **Absent where git has nothing to say** — a folder that is no repository, and a face with no
   * way to answer. Then none of the items is drawn, which is what a menu that cannot act should
   * look like.
   */
  git?: GitDoors;
}) {
  // The applications to pick from, once they have been asked for and there are any — the second
  // face of this one menu, drawn where the OS has no chooser to draw it for us.
  const [apps, setApps] = useState<FolderAppDto[] | null>(null);

  const act = (go: () => Promise<void>) => {
    onClose();
    void go().catch(() => {});
  };

  /** An item whose work is on the panel rather than out at the machine: the menu goes, and the box a
   *  name is typed into takes its place. */
  const pick = (go: () => void) => {
    onClose();
    go();
  };

  // Read out once, so the item below is drawn from the same answer it calls.
  const rename = naming?.onRename ?? null;
  /** Whether the menu is about one row, which is what the doors that name a single thing need. */
  const alone = about.length === 1;

  /** Ask, and then either draw what came back or step aside because the OS already asked. */
  const choose = () => {
    void folderOpenWith(projectId, root, path)
      .then((found) => (found.length > 0 ? setApps(found) : onClose()))
      .catch(() => onClose());
  };

  return (
    // The face is handed over because the items are replaced whole when the applications come back,
    // and the reader would otherwise be left standing on a button that is no longer there.
    <Menu at={at} face={apps} onClose={onClose}>
      {apps === null ? (
        <>
          {naming !== undefined && dir && (
            <>
              <MenuItem off={!alone} onClick={() => pick(() => naming.onMake(false))}>
                {t("files.newFile")}
              </MenuItem>
              <MenuItem off={!alone} onClick={() => pick(() => naming.onMake(true))}>
                {t("files.newFolder")}
              </MenuItem>
            </>
          )}
          {rename !== null
            && <MenuItem off={!alone} onClick={() => pick(rename)}>{t("files.rename")}</MenuItem>}
          {/* What `⌘C` on the row already does, said out loud: the keys are how a reader who knows
              them copies a path, and the menu is where everybody else looks (`AMB-D-832`). It puts
              the file itself and the plain path on the machine's clipboard in one press, which is
              why one word covers a folder as well as a file — what is copied is the row, and the
              row is named the same either way. Nothing is drawn afterwards: a clipboard says what
              it holds when it is pasted, and a line here would be read as something having gone
              wrong.

              Several rows go on in one press, as the files they are and as their paths on one line
              each: what the clipboard holds is what was picked out, and the press that put it there
              is the one a reader made (`AMB-D-832`). */}
          <MenuItem onClick={() => act(() => folderClipCopy(projectId, root, about))}>
            {t("files.copyPath")}
          </MenuItem>
          {/* The one door that goes the other way: everything beside it hands the row out to the
              machine, and this puts the path it is at in front of what is running in the pane —
              which is the reverse of a path drawn in a pane opening the file here
              (`../shell/WorkspaceFace`). It stands under the copy because the two are about the same
              path, and under rather than over it because this one is gone wherever there is no pane
              to hand anything to — the copy above would otherwise move up the menu depending on
              what else is open. Of the doors here it is the one whose answer stays inside the app,
              and it is over a folder as much as over a file: nothing is carried, so a folder costs
              no more to name than a file does (`AMB-D-820`).

              Several rows go over together, quoted one by one with a space between them — the same
              line a drop of several files puts in a pane (`../shell/WorkspaceFace`, `AMB-D-801`).
              What they are called then says no kind: the rows a reader gathered can be a folder and
              four files, and the panel is not told which of them are which (`Tree`). */}
          {onHandOver !== undefined && (
            <MenuItem
              onClick={() => { onClose(); onHandOver(about.map((one) => fileAt(root, one))); }}
            >
              {alone
                ? (dir ? t("files.pasteFolderPath") : t("files.pasteFilePath"))
                : t("files.pastePaths")}
            </MenuItem>
          )}
          {/* The three doors out to the machine, each one taken for every row the menu is about.
              A folder among them is opened the way the machine opens a folder — which is what a
              reader who picked one out and asked for it to be opened meant. */}
          {!dir && (
            <>
              <MenuItem
                onClick={() => act(() => eachOf(about, (one) => folderOpenFile(projectId, root, one)))}
              >
                {t("files.openWith")}
              </MenuItem>
              <MenuItem onClick={choose}>{t("files.chooseApp")}</MenuItem>
              <MenuItem
                onClick={() => act(() => eachOf(
                  oncePerFolder(about),
                  (one) => folderRevealFile(projectId, root, one),
                ))}
              >
                {t("files.reveal")}
              </MenuItem>
            </>
          )}
          {/* What git can be told about the row, held off from the doors above because they hand
              the row to the machine and these change what the repository records about it. The
              history is first: it is the one of the four that only reads, and it is what a reader
              opens the others from having looked at (`AMB-D-906`). */}
          {git?.onHistory !== undefined && path.length > 0 && (
            <MenuItem
              apart
              // One row's history and not a set's: what `git log` is asked is one path, and five
              // answers are five lists nothing could draw as one.
              off={!alone}
              // The path as this row's own folder spells it, which is the folder git is run in
              // for that road — so it is read as written (`crate::folder_git`).
              onClick={() => { onClose(); git.onHistory?.(path.join("/")); }}
            >
              {t("git.fileHistory")}
            </MenuItem>
          )}
          {git?.onIgnore !== undefined && path.length > 0 && (
            <MenuItem onClick={() => { onClose(); git.onIgnore?.(); }}>{t("git.ignore")}</MenuItem>
          )}
          {git?.onUntrack !== undefined && path.length > 0 && (
            <MenuItem
              off={git.followsAll === false}
              onClick={() => { onClose(); git.onUntrack?.(); }}
            >
              {t("git.untrack")}
            </MenuItem>
          )}
          {/* The two that settle a conflict by taking one side of it whole. They are together and
              set off from the rest, because they are the only items here that are about a row being
              in conflict — and what they write is the file, not the index: the count of what is
              left drops to none and the row is still the reader's to declare settled
              (`crate::folder_git_write`). */}
          {git?.onTake !== undefined && path.length > 0 && (
            <>
              <MenuItem apart onClick={() => { onClose(); git.onTake?.(true); }}>
                {t("git.takeOurs")}
              </MenuItem>
              <MenuItem onClick={() => { onClose(); git.onTake?.(false); }}>
                {t("git.takeTheirs")}
              </MenuItem>
            </>
          )}
          {/* The one road here nothing walks back. It is last, and the question in front of it is
              the bin's own shape for the reason that decision gives (`./RestoreAsk`, `AMB-D-777`). */}
          {git?.onRestore !== undefined && path.length > 0 && (
            <MenuItem onClick={() => { onClose(); git.onRestore?.(); }}>{t("git.restore")}</MenuItem>
          )}
          {/* Over a folder as much as over a file: the bin takes one whole, and the undo brings it
              back whole. The bound folder is the one row it is not offered over — that row is the
              binding, and what a binding is changed from is the project's own settings.

              Set apart from the doors above it, because it is the one item that changes the folder
              rather than handing it to something else: a press meant for the row above must not be
              able to land on this one by half a pixel. */}
          {path.length > 0 && (
            <MenuItem apart onClick={() => { onClose(); onTrash(); }}>
              {t("files.trash")}
            </MenuItem>
          )}
        </>
      ) : (
        apps.map((app) => (
          <MenuItem
            key={app.path}
            // The list was asked about the row the menu was opened on, and every row it is about is
            // opened with what the reader picked off it: what a person choosing an application for
            // five files has chosen is one application.
            onClick={() => act(() => eachOf(
              about,
              (one) => folderOpenFileWith(projectId, root, one, app.path),
            ))}
          >
            {/* The one the file would have opened with anyway is said to be that, not just put
                first: a list whose order carries the meaning loses it the moment somebody reads
                from the middle. */}
            {app.usual ? tf("files.appUsual", { name: app.name }) : app.name}
          </MenuItem>
        ))
      )}
    </Menu>
  );
}
