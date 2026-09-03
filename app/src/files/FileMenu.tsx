// The menu a row carries: what the machine can do with the paths a press is about — open them where
// the machine would, show them where they live, hand them to the pane being worked in, or put them
// in the bin.
//
// **One menu for both columns** (`AMB-D-835`). It is opened on a row of the tree in the rail and on
// the file being read on the other side of the panes, and what it acts on is paths — which are the
// same thing whichever column named them.
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
export function FileMenu({ projectId, root, path, about, dir, at, naming, onClose, onTrash, onHandOver }: {
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
   * The doors that only make sense one at a time — writing a new name into a folder, and renaming —
   * are drawn only where this is one row. There is nothing to be gained by offering a rename over
   * five rows except a press that has to be refused afterwards.
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
   * (`../shell/TerminalFace`).
   *
   * **Absent where there is no pane to hand it to**, and then the item is not drawn: the panel is
   * open beside a terminal face with nothing running in it as readily as beside one with four, and
   * an item that answers nothing is worse than an item that is not there.
   */
  onHandOver?: (wholes: string[]) => void;
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
          {naming !== undefined && dir && alone && (
            <>
              <MenuItem onClick={() => pick(() => naming.onMake(false))}>
                {t("files.newFile")}
              </MenuItem>
              <MenuItem onClick={() => pick(() => naming.onMake(true))}>
                {t("files.newFolder")}
              </MenuItem>
            </>
          )}
          {rename !== null && alone
            && <MenuItem onClick={() => pick(rename)}>{t("files.rename")}</MenuItem>}
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
              (`../shell/TerminalFace`). It stands under the copy because the two are about the same
              path, and under rather than over it because this one is gone wherever there is no pane
              to hand anything to — the copy above would otherwise move up the menu depending on
              what else is open. Of the doors here it is the one whose answer stays inside the app,
              and it is over a folder as much as over a file: nothing is carried, so a folder costs
              no more to name than a file does (`AMB-D-820`).

              Several rows go over together, quoted one by one with a space between them — the same
              line a drop of several files puts in a pane (`../shell/TerminalFace`, `AMB-D-801`).
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
