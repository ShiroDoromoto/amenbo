// The file face: the right side of the terminal face, where what the agent in the pane is doing to
// the folder can be seen without leaving the window (`AMB-T-3602`).
//
// **The folder it draws belongs to the project.** It is rooted at a folder the project is bound to,
// so switching panes does not move it — what changed in the repository is the same question
// whichever terminal is in front of it.
//
// **Every bound folder is drawn, each in a section of its own** (`AMB-D-778`). A project with one
// folder is drawn without a heading, because a heading over the only thing on the screen names
// nothing the reader could confuse it with. A folder that has gone keeps its section and says so:
// dropped from the list it would look like one nobody ever bound, and the reader would have no way
// to tell a folder that moved from a binding they removed.
//
// **What has changed is git's answer, drawn on the tree's own rows** (`AMB-D-785`). A list of the
// files written to most recently used to stand above the tree, and what it answered was "yesterday"
// over and over: a branch switched, a formatter run, a build — none of it what a person is looking
// for. `M`, `A` and `??` mean something, so the colour goes where the names already are.
//
// **The folder is watched, not asked for.** The host lays a watch over it and says when it moves
// (`crate::folder_watch`), and that word is the moment to ask again — for the names of the level
// that is open, and for what git says about them. What it cannot watch — a folder too large to
// walk, a watch the kernel refused — is drawn as the line saying so, because an unwatched half
// looks exactly like a half where nothing happened (`AMB-T-3604`).
//
// **What a file is, is the host's answer, not this side's guess.** A NUL in the head makes it
// binary and the first bytes make it a picture (`crate::folder`); the name decides only whether
// text is drawn as Markdown, which is a question about rendering rather than about what the file
// is.
//
// **A name is made and changed in the row itself, never in a dialog.** What a person is naming sits
// in a list, and the names already in it are what they are choosing against — so the box takes the
// row's place and the refusal is drawn under it, where what they typed is still in front of them.
// Which names a machine will hold is the host's answer and no rule written here
// (`crate::folder_write`).
//
// **A file dragged in from the desktop lands on the application, not on this page** (`AMB-D-775`).
// So the panel is not told which row is under the pointer — it is told a point, and the folder that
// point falls in is worked out here (`../core/hostDrop`). Every folder in the tree is one, and so is
// the tree itself; a file row belongs to the folder holding it, which is what makes dropping on the
// name of a file mean the same as dropping just beside it.
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type {
  CSSProperties, KeyboardEvent as ReactKeyboardEvent, ReactNode, RefObject,
} from "react";
import type {
  FolderAppDto, FolderChangesDto, FolderEntryDto, FolderFileDto, GitEntryDto,
} from "../bindings/bindings";
import { Markdown } from "../components/Markdown";
import { Menu, MenuItem } from "../components/Menu";
import { useBoundFolders } from "../core/boundFolders";
import { watchHostDrop } from "../core/hostDrop";
import { fileUrl } from "../core/fileUrl";
import { errText, formatNumber, isErr, t, tf } from "../core/i18n";
import { asTyped } from "../core/keys";
import { pushNotice } from "../core/notice";
import { RefNavProvider, useRefNav, type RefNav } from "../core/refNav";
import {
  folderClipCopy, folderClipPaste, folderEncodings, folderEntries, folderGitStatus, folderImport,
  folderMake, folderOpenFile,
  folderOpenFileWith, folderOpenWith, folderRead, folderRename, folderRevealFile, folderSave,
  folderTrash, folderUnwatch, folderUntrash, folderWatch, onFolderChanged,
} from "./folder";
import { stoppedLine, whyStopped } from "./stopped";
import { asksBeforeTrash } from "./askBeforeTrash";
import { TrashAsk } from "./TrashAsk";
import { FileEditor } from "./FileEditor";
import { MemoPage } from "./MemoPage";
import { fileAt, fileUnderAny } from "./fileUnder";
import { gitMarks, type GitMark } from "./gitMark";
import { sectionsOf } from "./sections";
import { Icon } from "../components/Icon";

/** The names a file's text is drawn as Markdown under. The one thing here the name decides. */
const MARKDOWN = [".md", ".markdown"];

/**
 * Where a dragged file would land: the bound folder, and the folder inside it as its segments joined
 * ("" being the bound folder itself).
 *
 * Both halves are needed because every section draws a row for its own root, and two sections have a
 * `src` each — a landing that said only `src` would light one up in both.
 */
type Landing = { root: string; into: string };

/**
 * A name being typed into the tree: one being made in a folder, or one being written over.
 *
 * It is one at a time and it belongs to the panel rather than to a row, for the reason the menu does:
 * a row that held it would take it away with itself the moment the list moved underneath.
 */
type Edit =
  | { kind: "make"; root: string; into: string[]; dir: boolean }
  | { kind: "rename"; root: string; path: string[] };

/**
 * How a reader has opened one folder's tree: whether the tree itself is unfolded, which folders
 * inside it are, and which row holds the tree's stop in the tab order.
 *
 * **The panel holds it, not the section.** Reading a file draws the reader in the sections' place
 * (`FileReader`), and what a component holds goes when it is unmounted — so a tree whose state lived
 * in its own section would be folded shut again every time somebody came back from a file, with
 * their place in it gone too.
 *
 * **Only how the reader opened it.** What is read off the disk — the names, git's answer, whether
 * the folder moved — stays with the watch, which is the section's own and has to stop when the
 * section does.
 *
 * `open` is held for the whole tree rather than per level, because opening a folder is also
 * something the section does on a reader's behalf: a name being made in a folder that is folded shut
 * would be typed where nobody could see it.
 */
type Opened = {
  treeOpen: boolean;
  /** Which folders of the tree are unfolded, as their paths joined. */
  open: string[];
  /** Which row holds the stop, as its path joined — nothing until a reader has been on one. */
  cursor: string | null;
};

/**
 * A tree nobody has touched yet: the first level open, nothing under it, and no stop taken.
 *
 * **The names of the bound folder are what a reader opens this half for.** So they are on the
 * screen when it is drawn: a heading standing over nothing is a control every reader has to work
 * before seeing anything, and a press that asks for what somebody came for need not be asked.
 *
 * `open` stays empty, which is what makes it the **first** level and not the whole tree: the levels
 * under it cost a read each and are asked for when somebody opens them (`Tree`).
 */
const AT_FIRST: Opened = { treeOpen: true, open: [], cursor: null };

/** The name being typed in one section, and the end of typing it. */
type Naming = {
  /** The one being typed here, or nothing — a section is handed only its own. */
  edit: Edit | null;
  /** End it. `made` says whether the folder now holds something it did not, which is what tells the
   *  levels under it to look again rather than wait for the host to say the folder moved. */
  end: (made: boolean) => void;
  /** Begin renaming one row of this section, named by its path from the bound folder. It is here
   *  rather than only on the row's menu because a reader walking the tree with the arrows has no
   *  pointer in their hand to open one with. */
  rename: (path: string[]) => void;
};

/**
 * Where a name's stem ends — the part of it a rename is usually about.
 *
 * The last dot and not the first: `archive.tar.gz` is renamed by changing `archive.tar`, and a name
 * cut at the first dot would hand back a stem nobody meant. A dot at the very front is not one of
 * these — `.gitignore` is a name, not an extension on an empty stem — and a name with no dot in it
 * is all stem, so both answer with the whole length (the convention `./grammars` reads names by).
 */
function stemEnd(name: string): number {
  const dot = name.lastIndexOf(".");
  return dot <= 0 ? name.length : dot;
}

/** Whether an edit is the renaming of this row. */
function renaming(edit: Edit | null, root: string, path: string[]): boolean {
  return edit?.kind === "rename" && edit.root === root && edit.path.join("/") === path.join("/");
}

/** The row under the pointer, read as a landing — its folder and its path, or nothing off a row. */
function landingOf(el: Element | null | undefined): Landing | null {
  const into = el?.getAttribute("data-into");
  if (into === null || into === undefined) return null;
  return { root: el?.getAttribute("data-root") ?? "", into };
}

/** Whether this folder of this section is the one a drop would land in. */
function landed(landing: Landing | null, root: string, into: string | undefined): boolean {
  return into !== undefined && landing?.root === root && landing.into === into;
}

/**
 * A landing's folder as the segments the host takes, the bound folder itself being none of them.
 *
 * The join is this face's own — a name cannot hold a `/` on any of the three — so splitting it back
 * is exact rather than a guess at where one name ended.
 */
function segmentsOf(into: string): string[] {
  return into === "" ? [] : into.split("/");
}

export function FilesPanel({ projectId, onOpenLedger, show, tab, onTab, onClose, onHandOver }: {
  /** The project whose folder the face is rooted at; nothing is drawn without one. */
  projectId: number | null;
  /** Leave the terminal face for the ledger — what a reference or a record means when it is clicked. */
  onOpenLedger?: () => void;
  /**
   * A path clicked in a pane, as it was drawn, with the folder that pane is in — what a relative one
   * is read against. `nth` counts the asking, so the same file clicked twice opens twice
   * (`AMB-T-3630`).
   */
  show?: { target: string; cwd: string | null; nth: number } | null;
  /**
   * Which of the two halves is up, and how to ask for one.
   *
   * **The switch is the terminal face's top row and nowhere else.** This panel drew tabs of its own
   * as well, and two controls that do the same thing leave a reader looking for the right one; the
   * row that stayed is the one that is also there while the panel is closed
   * (`../shell/TerminalFace`). So which half is up is the caller's answer, and this panel only
   * reads it — except that a file clicked in a pane asks for the files half, which is how a panel
   * that had been closed comes back to show it.
   */
  tab: "files" | "memo";
  onTab: (tab: "files" | "memo") => void;
  /** Put the panel away. What opens it again is the top row, which is where it was opened from. */
  onClose: () => void;
  /**
   * Hand a file to the pane the reader is working in, as the whole path it is at — the reverse of a
   * path drawn in a pane opening the file here (`../shell/TerminalFace`).
   *
   * Which pane that is, and whether there is one at all, is the terminal face's own answer: with
   * none, none of this is handed down and the row's menu draws no item for it.
   */
  onHandOver?: (whole: string) => void;
}) {
  // `0` names no project, which is what the folder read then answers with: none. A window with no
  // project on it draws the invitation, the same as one whose project has no folder.
  const folders = useBoundFolders(projectId ?? 0);
  // Every folder recorded rather than every folder that is there: one that has gone is a section
  // saying so, and it can only say so if it is still on the list.
  const sections = useMemo(() => sectionsOf(folders.all), [folders.all]);
  const live = folders.live.map((one) => one.path);
  // Which file is being read, and which folder it is in. The folder travels with the path because
  // the same path means a different file in each section.
  const [reading, setReading] = useState<{ root: string; path: string[] } | null>(null);
  // The file a right-click was on, and where the pointer was. One menu for the face rather than one
  // per row: only one can be open, and a row that held its own would keep it after the list moved
  // under it (`AMB-T-3605`).
  const [menu, setMenu] = useState<
    { root: string; path: string[]; dir: boolean; x: number; y: number } | null
  >(null);
  // The name being typed into the tree, if any. Held here rather than in the row it is in, so that a
  // list redrawn under it — which is what making a name does — does not take the box away with the
  // row it was on.
  const [edit, setEdit] = useState<Edit | null>(null);
  // Where a file being dragged in would land: which bound folder, and which folder inside it ("" is
  // the bound folder itself). Null while nothing is over the panel. The folder is half of it because
  // every section has a row for its own root, and the same path inside two of them is two places.
  const [landing, setLanding] = useState<Landing | null>(null);
  // How each bound folder's tree is opened, by the folder it is about — the folder is the key
  // because a project draws a section per binding and each of them is opened on its own (`Opened`).
  const [opened, setOpened] = useState<Record<string, Opened>>({});
  // The row a question about the bin is standing over, or nothing while none is up.
  const [asking, setAsking] = useState<{ root: string; path: string[] } | null>(null);
  // What the machine said about the last row that would not go — kept until the next press, because
  // the row it is about is no longer on the list to say it for itself.
  const [stopped, setStopped] = useState<string | null>(null);
  // How many times this side has changed a folder. What redraws the rows is not this but the host's
  // word that the folder moved, which each section counts for itself; this is only how the focus
  // knows a press has landed.
  const [acted, setActed] = useState(0);
  const box = useRef<HTMLDivElement | null>(null);
  // A path clicked in a pane. It opens only where it lands inside one of the folders this face is
  // rooted at — the same fence the host applies. One that lands outside opens nothing: the pane
  // keeps the characters it drew, and no reader is shown a file from somewhere this face cannot
  // answer for (`AMB-D-747`).
  const roots = live.join("\0");
  useEffect(() => {
    if (show === undefined || show === null) return;
    const found = fileUnderAny(live, show.cwd, show.target);
    // A file asked for is a file to be looked at: the panel comes back off the page to show it.
    if (found) {
      setReading(found);
      onTab("files");
    }
    // `nth` is what makes the same file asked for twice two answers, and the folders are joined
    // because the array itself is rebuilt on every render.
  }, [show?.nth, roots]);

  // A folder nobody is bound to any more takes how it was opened with it. Unbinding one, or moving
  // to another project, leaves a key here that names a section that is no longer drawn — and it
  // would be read again by whoever bound the same path back, as an answer about a tree they never
  // opened. The sections rather than the live folders decide: a folder that has gone is still drawn
  // and still says so, and a reader who left it open should find it that way when it comes back.
  const recorded = sections.map((one) => one.path).join("\0");
  useEffect(() => {
    setOpened((was) => {
      const kept = Object.entries(was).filter(([root]) => sections.some((one) => one.path === root));
      return kept.length === Object.keys(was).length ? was : Object.fromEntries(kept);
    });
  }, [recorded]);

  // Files dragged in from the desktop. The panel hears about them from the host rather than from
  // the DOM, so the highlight under the pointer — and the scroll when the pointer hangs at an edge —
  // are this side's to drive (`../core/hostDrop`).
  //
  // **The folder the highlight named is the folder the files are carried into** (`./folder`). Both
  // halves of the landing go to the host, not the path alone: every section draws a row for its own
  // root, and two projects' folders each holding a `src` are two places (`AMB-T-3781`).
  //
  // **What arrived is not said, and what did not is.** The folder is watched and the tree reads its
  // names again on every move, so a file that came in is about to be a row — a line saying so would
  // be a second, slower account of what the panel is already drawing. A carry that stopped leaves
  // nothing to draw, and that is what the toast is for.
  useEffect(() => {
    if (projectId === null || sections.length === 0 || tab !== "files") return;
    let alive = true;
    let stop: (() => void) | null = null;
    void watchHostDrop({
      select: "[data-into]",
      scroller: () => box.current,
      over: ({ el }) => { if (alive) setLanding(landingOf(el)); },
      leave: () => { if (alive) setLanding(null); },
      drop: ({ el }, paths, effect) => {
        if (!alive) return;
        setLanding(null);
        const at = landingOf(el);
        if (at === null) return;
        void folderImport(projectId, paths, at.root, segmentsOf(at.into), effect)
          .then((carried) => {
            const line = stoppedLine(carried);
            if (line !== null) pushNotice(line);
          })
          // The refusal is the host's own sentence — the folder having gone since the row was drawn
          // is the whole of what it can be, and it is worth saying rather than swallowing.
          .catch((e: unknown) => pushNotice(errText(e)));
      },
    }).then((off) => { if (alive) stop = off; else off(); });
    return () => {
      alive = false;
      stop?.();
      setLanding(null);
    };
  }, [projectId, roots, tab]);

  // The panel takes the focus once it has changed a folder, so that undo is the next thing a reader
  // can press. What did the changing was a menu item that is gone by the time the answer lands, and
  // a key nothing is focused on reaches nothing (`AMB-D-780`).
  //
  // After the state has settled rather than inside it: the list is only mounted again once the
  // reading state has cleared, so the element to focus does not exist yet where the row was binned
  // from the reader.
  useEffect(() => {
    if (acted > 0) box.current?.focus();
  }, [acted]);

  // Put one row in the machine's bin. Nothing here deletes: what the host offers is the bin, and a
  // machine that cannot offer one refuses rather than deleting instead (`./folder`).
  const bin = (root: string, path: string[]) => {
    if (projectId === null) return;
    setStopped(null);
    void folderTrash(projectId, root, [path])
      .then((done) => {
        setActed((n) => n + 1);
        setStopped(done.stopped === null ? null : whyStopped(done.stopped));
        // A file being read that has just gone is not a file to go on reading.
        if (done.gone.length > 0) {
          setReading((now) =>
            now !== null && now.root === root && now.path.join("/") === path.join("/") ? null : now
          );
        }
      })
      .catch((e: unknown) => setStopped(errText(e)));
  };

  // The question first, unless this reader has said they do not want it (`./askBeforeTrash`).
  const askTrash = (root: string, path: string[]) => {
    if (asksBeforeTrash()) setAsking({ root, path });
    else bin(root, path);
  };

  // Undo, which here means the last press of the bin and nothing else. It is the OS's own key rather
  // than one Amenbo invented, and it is heard on the panel rather than on the window: the terminal
  // beside it has its own idea of what the key means, and the boundary between the two is which of
  // them the reader is in (`AMB-D-780`).
  const undo = () => {
    setStopped(null);
    void folderUntrash()
      .then((done) => {
        if (done === null) return;
        setActed((n) => n + 1);
        setStopped(done.stopped === null ? null : whyStopped(done.stopped));
      })
      .catch((e: unknown) => setStopped(errText(e)));
  };

  /**
   * The three keys the machine already has, heard on the panel rather than on the window: the
   * terminal beside it has its own idea of what each of them means, and the boundary between the two
   * is which of them the reader is in (`AMB-D-780`).
   *
   * **Copy and paste act on a row and nowhere else.** A file being read is drawn in an editor, and
   * ⌘C there is the editor's — it copies the words somebody selected. So neither key is taken unless
   * the keyboard is standing on the tree, and the press falls through untouched when it is not.
   */
  const onKey = (e: ReactKeyboardEvent) => {
    // **One press, one layer.** The file lying over the tree goes first and the panel itself after
    // it, so the two things a reader might mean by "back" are told apart by how many times they
    // press rather than by finding a different way out of each (`AMB-D-815`).
    if (e.key === "Escape") {
      // Not this panel's to take while something inside it is already answering to the same key: a
      // menu and the question before a bin both close on Escape, and a press counted twice would
      // carry the reader a layer past the one they asked for. A name being typed stops the press
      // itself, so it never arrives here (`NameBox`).
      if (asking !== null || (e.target as HTMLElement).closest('[role="menu"]') !== null) return;
      e.preventDefault();
      if (reading !== null) setReading(null);
      else onClose();
      return;
    }
    if (!(e.metaKey || e.ctrlKey) || e.shiftKey || e.altKey) return;
    const pressed = e.key.toLowerCase();
    if (pressed === "z") {
      e.preventDefault();
      undo();
      return;
    }
    if (projectId === null) return;
    const on = e.target as HTMLElement;

    // The row the keyboard is on, which is the one thing a copy could be about.
    if (pressed === "c") {
      const row = on.closest<HTMLElement>('[role="treeitem"]');
      const root = row?.dataset.root;
      const path = row?.dataset.key;
      if (root === undefined || path === undefined) return;
      e.preventDefault();
      void folderClipCopy(projectId, root, [segmentsOf(path)])
        .catch((why: unknown) => pushNotice(errText(why)));
      return;
    }

    // And the folder it would land in, worked out the way a drop's landing is: a file's row belongs
    // to the folder holding it, so pasting on a name means the same as pasting beside it.
    if (pressed === "v") {
      const at = landingOf(on.closest("[data-into]"));
      if (at === null) return;
      e.preventDefault();
      void folderClipPaste(projectId, at.root, segmentsOf(at.into))
        .then((carried) => {
          const line = stoppedLine(carried);
          if (line !== null) pushNotice(line);
        })
        .catch((why: unknown) => pushNotice(errText(why)));
    }
  };

  // The question, and the line the last refusal left. Both are drawn in whichever state the panel is
  // in: a file can be sent to the bin from the list and from the reader, so neither of them is the
  // one place an answer about it belongs.
  const aside = (
    <>
      {stopped !== null && <p className="files__stopped">{stopped}</p>}
      {asking !== null && (
        <TrashAsk
          name={asking.path[asking.path.length - 1] ?? ""}
          onGo={() => { const one = asking; setAsking(null); bin(one.root, one.path); }}
          onCancel={() => setAsking(null)}
        />
      )}
    </>
  );

  // The way to put the panel away, and the whole of the row it sits on. It is drawn in every state
  // the panel can be in — reading a file included — because a panel that could only be closed from
  // one of its states is one a reader has to find their way back out of.
  const close = (
    <button className="files__close" title={t("pane.close")} onClick={onClose}>
      <Icon name="close" />
    </button>
  );

  // The draft page is the project's, and a project has one whether or not it is bound to a folder
  // (`./MemoPage`). So the half that is up is answered first, and only the files half goes on to ask
  // where it is rooted — a reader with nowhere to read files still has somewhere to write
  // (`AMB-T-3690`).
  if (projectId !== null && tab === "memo") {
    return (
      <div className="files">
        <div className="files__top">{close}</div>
        <MemoPage projectId={projectId} />
      </div>
    );
  }

  if (projectId === null || sections.length === 0) {
    // A read that has not come back draws nothing at all: a flash of "no folder" on a project that
    // has one reads as a broken binding (`core/boundFolders`).
    return folders.answered
      ? (
        <div className="files files--empty">
          <div className="files__top">{close}</div>
          <p className="files__none">{t("files.noFolder")}</p>
        </div>
      )
      : <div className="files"><div className="files__top">{close}</div></div>;
  }

  const top = <div className="files__top">{close}</div>;

  return (
    // Focusable so the panel can hold the key it hears, and taken off the tab order so that being
    // able to hold it costs nobody a stop on the way past (`AMB-D-780`).
    <div className="files" ref={box} tabIndex={-1} onKeyDown={onKey}>
      {/* The row and the question are the tree's while the tree is what is on top. A file being
          read draws both itself, and two of each in the document is two crosses to close and two
          questions to answer — one of them under a panel nobody can reach it through. */}
      {reading === null && top}
      {reading === null && aside}
      {sections.map((one) => (
        <FolderSection
          key={one.path}
          projectId={projectId}
          root={one.path}
          // The only folder there is needs no heading: a name is what tells two of them apart.
          label={sections.length > 1 ? one.label : null}
          bound={one.exists}
          landing={landing}
          scroller={box}
          opened={opened[one.path] ?? AT_FIRST}
          onOpened={(change) => setOpened((was) => ({
            ...was,
            [one.path]: change(was[one.path] ?? AT_FIRST),
          }))}
          edit={edit?.root === one.path ? edit : null}
          onEdit={setEdit}
          onRead={(path) => { setEdit(null); setReading({ root: one.path, path }); }}
          onMenu={(path, dir, x, y) => setMenu({ root: one.path, path, dir, x, y })}
          onTrash={(path) => askTrash(one.path, path)}
          chosen={reading !== null && reading.root === one.path ? reading.path.join("/") : null}
        />
      ))}
      {menu !== null && (
        <FileMenu
          projectId={projectId}
          root={menu.root}
          path={menu.path}
          dir={menu.dir}
          at={{ x: menu.x, y: menu.y }}
          naming={{
            // Into the row that was right-clicked, which the menu only offers over a folder: what
            // can hold a new name is a folder, and pointing at one is how a person says which.
            onMake: (dir) => setEdit({ kind: "make", root: menu.root, into: menu.path, dir }),
            // The bound folder itself is no row anybody may rename here: it is the binding, and
            // where a binding is changed is the project's own settings.
            onRename: menu.path.length === 0
              ? null
              : () => setEdit({ kind: "rename", root: menu.root, path: menu.path }),
          }}
          onClose={() => setMenu(null)}
          onTrash={() => askTrash(menu.root, menu.path)}
          onHandOver={onHandOver}
        />
      )}
      {/* The file being read, lying over the tree rather than in place of it (`AMB-D-815`). The
          tree stays mounted underneath — which folders are open, and which row the keyboard is on,
          are what a reader comes back to — and a band of it is left showing on the left, so what
          the panel is covering is on the screen and not only in the reader's memory. */}
      {reading !== null && (
        <FileReader
          projectId={projectId}
          root={reading.root}
          path={reading.path}
          onBack={() => setReading(null)}
          onOpenLedger={onOpenLedger}
          onTrash={() => askTrash(reading.root, reading.path)}
          onKey={onKey}
          close={close}
          aside={aside}
          onHandOver={onHandOver}
        />
      )}
    </div>
  );
}

/**
 * One bound folder: its tree, wearing what git says about it.
 *
 * **The watch is the section's own.** Each folder is watched separately and each answer names the
 * folder it is about (`./folder`), so a section takes the news addressed to it and leaves the rest.
 * Holding one watch for the panel would mean the panel deciding which folder each answer belonged
 * to, which is the same work done once further from where it is used.
 *
 * **What is watched is the section's, how it is opened is the panel's.** The two are held apart on
 * one line: a watch has to stop when the section does, and how a reader opened the tree has to
 * outlive it — the section is unmounted for as long as a file is being read (`Opened`).
 *
 * **The news is a number, not a payload.** What the host says when the folder moves carries no
 * rows (`AMB-D-785`), so what a section does with it is count: `moved` goes up, and everything read
 * off the disk — git's answer here, the names of each open level in `Tree` — is asked for again
 * because it changed. One counter rather than a refresh per reader, because a folder moving is one
 * fact and they would all be reacting to it.
 *
 * A folder that is gone draws its heading and the reason, and nothing else. There is nothing to
 * watch and no tree to open, and the two states it could be confused with — a binding somebody
 * removed, and a folder with nothing in it — both look like an empty section.
 *
 * **A half-watched folder says which half-watched it is.** Too big to walk to the end of and out of
 * watches are separate answers from the host and are drawn as separate lines (`AMB-D-778`): one is
 * about this folder and is answered by pointing the app at less of it, the other is about the
 * machine and is answered by giving it more watches. They are said of the folder rather than of the
 * tree below, so they stand beside the heading, above it.
 */
function FolderSection({
  projectId, root, label, bound, landing, scroller, opened, onOpened, edit, onEdit, onRead, onMenu,
  onTrash, chosen,
}: {
  projectId: number;
  root: string;
  /** The heading, or nothing where this is the only folder. */
  label: string | null;
  /** Whether the store's own read found the folder. The watch answers the same question later. */
  bound: boolean;
  /** Where a dragged file would land, anywhere on the panel — a section draws the highlight only
   *  where the landing is one of its own (`landed`). */
  landing: Landing | null;
  /** The box the panel scrolls in — the panel's own, because every section is drawn in the one box
   *  and what is in view of it is what each tree draws (`Tree`). */
  scroller: RefObject<HTMLElement | null>;
  /** How this folder's tree is opened. The panel holds it rather than the section, so that it
   *  outlives a section unmounted by anything at all — a folder unbound and bound back, a project
   *  switched away from and returned to (`Opened`). */
  opened: Opened;
  /** Change it — handed the way it stands, because two of these can land in one render: unfolding
   *  the way to a name being made is the tree and one folder inside it, said at once. */
  onOpened: (change: (was: Opened) => Opened) => void;
  /** The name being typed in this section, or nothing. The panel holds it, and hands each section
   *  only what is its own. */
  edit: Edit | null;
  onEdit: (edit: Edit | null) => void;
  onRead: (path: string[]) => void;
  onMenu: (path: string[], dir: boolean, x: number, y: number) => void;
  /** Put one row of this folder in the machine's bin. */
  onTrash: (path: string[]) => void;
  /** The row of this folder whose file is being read, or nothing where the file being read is in
   *  another folder — or where none is. The panel works out which section it belongs to, since it
   *  is the panel that knows what is open. */
  chosen: string | null;
}) {
  const [changes, setChanges] = useState<FolderChangesDto>(
    { root, capped: false, unwatched: false, gone: false },
  );
  // How many times the host has said this folder moved. Everything read off the disk watches it.
  const [moved, setMoved] = useState(0);
  const [git, setGit] = useState<GitEntryDto[]>([]);
  // How this tree stands: the panel's answer, because a section that was unmounted would lose it
  // (`Opened`). Nothing until a reader has been on a row is what puts the tab stop on the first one
  // the moment the panel is drawn, rather than leaving a reader to Tab through the tree to find out
  // where they are.
  const { treeOpen, open, cursor } = opened;
  const gone = !bound || changes.gone;

  const naming: Naming = {
    edit,
    // A name written into the folder moves it as surely as an agent does, and the host says so 400ms
    // later (`crate::folder_watch`). A row somebody has just made belongs on the list before that.
    end: (made) => {
      if (made) setMoved((n) => n + 1);
      onEdit(null);
    },
    // The bound folder itself is no row anybody may rename here — it is the binding, and where a
    // binding is changed is the project's own settings. Said again on this road because the menu's
    // road says it too, and the keyboard reaches rows the menu was never opened on.
    rename: (path) => { if (path.length > 0) onEdit({ kind: "rename", root, path }); },
  };

  // The panel says which folder a name is being made in; unfolding the way to it is this section's
  // own answer, said in one change because the tree and the folder inside it open together. The
  // first level is open from the start and everything under it is not, so a box typed into a folder
  // nobody can see is the ordinary case rather than a corner.
  const making = edit?.kind === "make" ? edit.into.join("/") : null;
  useEffect(() => {
    if (making === null) return;
    onOpened((was) => ({
      ...was,
      treeOpen: true,
      open: making === "" || was.open.includes(making) ? was.open : [...was.open, making],
    }));
  }, [making]);

  useEffect(() => {
    if (!bound) return;
    let alive = true;
    // Subscribed before the watch is asked for: the first thing the folder does could happen while
    // the host is still walking it, and a listener set up afterwards would miss exactly that.
    // Every watched folder is told about through the one listener, so an answer about another
    // folder is not this section's news.
    const listening = onFolderChanged((fresh) => {
      if (!alive || fresh.root !== root) return;
      setChanges(fresh);
      setMoved((n) => n + 1);
    });
    void folderWatch(projectId, root)
      .then((now) => { if (alive) setChanges(now); })
      .catch(() => {
        if (alive) setChanges({ root, capped: false, unwatched: false, gone: false });
      });
    return () => {
      alive = false;
      void listening.then((stop) => stop());
      void folderUnwatch(root);
    };
  }, [projectId, root, bound]);

  // What git says, asked for again every time the folder moves — staging included, which moves not
  // one byte of the working tree and every colour in the tree (`AMB-D-774`). A folder that is no
  // repository answers with nothing, which is a tree with no colours and not a failure to draw.
  //
  // **Only while the tree is open**, which is the same rule the levels themselves are read under: a
  // colour nobody is looking at is a process started for nothing, and a folder someone folds the
  // panel shut on is one an agent may be writing in all afternoon. The tree starts open, so this is
  // asked once when the files half is drawn — the same one process the first level's own read is.
  useEffect(() => {
    if (!bound || !treeOpen) return;
    let alive = true;
    void folderGitStatus(projectId, root)
      .then((rows) => { if (alive) setGit(rows); })
      .catch(() => { if (alive) setGit([]); });
    return () => { alive = false; };
  }, [projectId, root, bound, treeOpen, moved]);

  const marks = useMemo(() => gitMarks(git), [git]);

  // The whole of one folder is one box, so the space between two folders is wider than the space
  // inside either — two stacks of rows with the same gap everywhere read as one long stack.
  const heading = label !== null && (
    <h3 className="files__foldername" title={root}>{label}</h3>
  );

  if (gone) {
    return (
      <div className="files__folder">
        {heading}
        <p className="files__none">{t("files.folderGone")}</p>
      </div>
    );
  }

  return (
    <div className="files__folder">
      {heading}
      {/* Said out loud rather than left to be assumed: a folder only half watched goes on looking
          like one where nothing is happening. Both reasons stand in one stack, tight enough to read
          as one thing said about the folder rather than as two of the panel's rows. */}
      {(changes.capped || changes.unwatched) && (
        <div className="files__row">
          {changes.capped && <p className="files__none">{t("files.capped")}</p>}
          {/* The way out is said with the fact, because the fact alone reads as something the
              reader did — and it is not: the supply is the machine's and their editor is already
              spending it (`AMB-T-3753`). Which folders went unwatched is not said: they are
              whichever the walk reached last, so naming them would describe the walk rather than
              the project. */}
          {changes.unwatched && (
            <>
              <p className="files__none">{t("files.unwatched")}</p>
              <p className="files__none">{t("files.unwatchedHow")}</p>
            </>
          )}
        </div>
      )}
      {/* The root is a folder like any other in the tree, and the one a drop that fell on no row
          lands in. It is marked on the section rather than on the heading so that the whole of the
          tree — the gaps between its rows included — answers for it. */}
      <section
        className={`files__row${landed(landing, root, "") ? " files__row--into" : ""}`}
        data-root={root}
        data-into=""
      >
        {/* The heading is the folder's own row: right-clicking it is how a name is made at the top
            of the tree, where there is no row inside to point at. It carries the menu rather than the
            whole section so that a right-click on a row inside is that row's and not this one's. */}
        <button
          className="files__head files__head--button"
          aria-expanded={treeOpen}
          onClick={() => onOpened((was) => ({ ...was, treeOpen: !was.treeOpen }))}
          onContextMenu={(e) => {
            e.preventDefault();
            onMenu([], true, e.clientX, e.clientY);
          }}
        >
          {/* The same box the rows carry their chevron in, so the mark over the tree is the mark
              inside it — one drawing of open and shut, at one size (`AMB-D-686`). */}
          <span className="files__twisty">
            <Icon name={treeOpen ? "chevronDown" : "chevronRight"} />
          </span>
          {t("files.tree")}
        </button>
        {/* The first level from the start, and each level under it only when it is opened: the
            names of the bound folder are what a reader opens this half for, and everything deeper
            is a repository read to draw rows nobody has asked for. Folding the whole tree away is
            still a press, and it stops the reads the same way. */}
        {treeOpen && (
          <Tree
            projectId={projectId}
            root={root}
            landing={landing}
            scroller={scroller}
            marks={marks}
            moved={moved}
            open={open}
            onOpen={(key) => onOpened((was) => ({
              ...was,
              open: was.open.includes(key)
                ? was.open.filter((one) => one !== key)
                : [...was.open, key],
            }))}
            naming={naming}
            onRead={onRead}
            onMenu={onMenu}
            onTrash={onTrash}
            cursor={cursor}
            onCursor={(key) => onOpened((was) => ({ ...was, cursor: key }))}
            chosen={chosen}
          />
        )}
      </section>
    </div>
  );
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
function FileMenu({ projectId, root, path, dir, at, naming, onClose, onTrash, onHandOver }: {
  projectId: number;
  root: string;
  path: string[];
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
  onHandOver?: (whole: string) => void;
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
              <MenuItem onClick={() => pick(() => naming.onMake(false))}>
                {t("files.newFile")}
              </MenuItem>
              <MenuItem onClick={() => pick(() => naming.onMake(true))}>
                {t("files.newFolder")}
              </MenuItem>
            </>
          )}
          {rename !== null && <MenuItem onClick={() => pick(rename)}>{t("files.rename")}</MenuItem>}
          {!dir && (
            <>
              {/* The one door that goes the other way: everything under it hands the file out to
                  the machine, and this hands it to what is running in the pane — which is the
                  reverse of a path drawn in a pane opening the file here (`../shell/TerminalFace`).
                  It is first because it is the one whose answer stays inside the app. */}
              {onHandOver !== undefined && (
                <MenuItem onClick={() => { onClose(); onHandOver(fileAt(root, path)); }}>
                  {t("files.handOver")}
                </MenuItem>
              )}
              <MenuItem onClick={() => act(() => folderOpenFile(projectId, root, path))}>
                {t("files.openWith")}
              </MenuItem>
              <MenuItem onClick={choose}>{t("files.chooseApp")}</MenuItem>
              <MenuItem onClick={() => act(() => folderRevealFile(projectId, root, path))}>
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
            onClick={() => act(() => folderOpenFileWith(projectId, root, path, app.path))}
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

/**
 * Following something from this face means leaving it: what a record opens on is the ledger, and a
 * click that selected it behind this face would look like a link that did nothing (`AMB-D-747`).
 */
function useLedgerNav(onOpenLedger?: () => void): RefNav {
  const outer = useRefNav();
  return useMemo(() => ({
    selectTask: (id: number) => { onOpenLedger?.(); outer.selectTask?.(id); },
    selectDecision: (id: number | null) => { onOpenLedger?.(); outer.selectDecision?.(id); },
  }), [outer, onOpenLedger]);
}

/**
 * A row's classes: the faint one for a name the repository ignores, and the colour for what git
 * says about it.
 *
 * The row is drawn either way: `.gitignore` says what git does not record, not what a reader may
 * not look at, and the files a person goes looking for after an agent has been at work — `.env`,
 * `.amenbo` — are ignored in most repositories (`AMB-D-786`). What is left out of the tree is the
 * floor the host prunes, and that never reaches here.
 *
 * **The two can land on the same row and say different things.** Faint is "git does not record
 * this"; the colour is "git says this moved". A row is rarely both — what is ignored has no status
 * to show — but nothing here stops them, because nothing about either one depends on the other.
 */
function rowClass(
  base: string,
  ignored: boolean,
  mark: GitMark | null,
  chosen: boolean,
): string {
  let all = base;
  if (ignored) all += ` ${base}--ignored`;
  if (mark !== null) all += ` ${base}--git ${base}--git-${mark}`;
  if (chosen) all += ` ${base}--chosen`;
  return all;
}

/**
 * How tall one row of the tree is drawn, in pixels.
 *
 * **The stylesheet's figure, written here as well** (`../styles/global.css`). Every row is the same
 * height and none of them wraps, so where a row sits is a multiplication rather than something to
 * be measured — which is what lets the rows nobody is looking at be left out of the document and
 * still be stood in for by a box of the right size.
 */
const ROW = 22;

/**
 * How many rows above and below the window are drawn anyway.
 *
 * The window, the two boxes standing in for what is outside it and the scroll a key asks for are
 * all this file's own — no library holds them (`AMB-D-817`).
 *
 * A scroll is told about after it has happened, so a window drawn exactly to the edges shows a band
 * of nothing until the next drawing catches up. Six rows either way is a fifth of a panel's worth,
 * which is more than one wheel notch moves.
 */
const SPARE = 6;

/**
 * One row of the tree.
 *
 * **A row is a line of one list and not a box holding the lines under it.** What is open is a flat
 * run of rows, in the order a reader goes down them, and how far down a row stands is a number on
 * the row rather than the depth of the box it sits in. That is what lets a row in the middle be
 * left out of the document without the ones below it going with it (`AMB-T-4108`).
 *
 * It is also the order the keys walk, which is why the walk holds up when a row on the screen and
 * a row in the document stop being the same thing.
 */
type Row = {
  /** The row's path from the bound folder, joined — what everything about it is named by. */
  key: string;
  path: string[];
  name: string;
  isDir: boolean;
  ignored: boolean;
  /** The folder holding it, joined — what a drop or a paste on this row lands in. */
  in: string;
  /** How many folders down from the bound folder it stands, the first level being none. */
  depth: number;
  /** How many names the folder it came from holds, and which of them this is. */
  setsize: number;
  posinset: number;
  /** Whether it is a folder standing open. */
  unfolded: boolean;
};

/** One line of the tree as it is drawn: a row, or the box a name is being typed into. */
type Line =
  /** A name being typed, at the top of the folder it is being made in. */
  | { kind: "make"; at: string; depth: number; dir: boolean }
  | ({ kind: "row" } & Row);

/**
 * The folders whose names are on the screen: the bound folder, and every open one the way down to
 * which is open too.
 *
 * A folder left open inside one that was folded shut is not read — it is not drawn, and the reader
 * gets it back the way they left it when they open the way to it again.
 */
function shownFolders(open: string[]): string[] {
  const reachable = (key: string): boolean => {
    const at = key.split("/");
    for (let n = 1; n < at.length; n += 1) {
      if (!open.includes(at.slice(0, n).join("/"))) return false;
    }
    return true;
  };
  return ["", ...open.filter(reachable)];
}

/**
 * Every row on the screen, in the order they are read in — each folder's names in place of the row
 * that opened it.
 *
 * The walk is the same one `shownFolders` makes, so what is drawn and what is read off the disk
 * cannot drift apart.
 */
function linesOf(
  levels: Record<string, { rows: FolderEntryDto[] }>,
  open: string[],
  making: string | null,
  makingDir: boolean,
  at: string[],
): Line[] {
  const from = at.join("/");
  // Above the names rather than among them: where a new name would sort is decided by what is
  // typed, and a box that moved as the letters arrived would be a box nobody could read.
  const out: Line[] = making === from
    ? [{ kind: "make", at: from, depth: at.length, dir: makingDir }]
    : [];
  const names = levels[from]?.rows ?? [];
  names.forEach((one, i) => {
    const path = [...at, one.name];
    const key = path.join("/");
    const unfolded = one.isDir && open.includes(key);
    out.push({
      kind: "row",
      key,
      path,
      name: one.name,
      isDir: one.isDir,
      ignored: one.ignored,
      in: from,
      depth: at.length,
      setsize: names.length,
      posinset: i + 1,
      unfolded,
    });
    if (unfolded) out.push(...linesOf(levels, open, making, makingDir, path));
  });
  return out;
}

/** One bound folder's tree: every open row of it, drawn as one list. */
function Tree({
  projectId, root, landing, scroller, marks, moved, open, onOpen, naming, onRead, onMenu,
  onTrash, cursor, onCursor, chosen,
}: {
  projectId: number;
  root: string;
  /**
   * Where a file being dragged in would land — the whole panel's, because a drag hangs over one
   * folder of one section and every other row has to be able to stop drawing the highlight it was
   * drawing a moment ago.
   */
  landing: Landing | null;
  /**
   * The box the panel scrolls in.
   *
   * **The panel's and not the tree's**: every bound folder is drawn in the one box, one after
   * another, so what is in view of a tree is a question about a box further out than any of them.
   */
  scroller: RefObject<HTMLElement | null>;
  /** What git says about a row, asked by its segments from the bound folder, and by whether it is a
   *  folder standing folded — which is the one case that answers for what is under it (`./gitMark`). */
  marks: (path: string[], folded?: boolean) => GitMark | null;
  /** How many times the folder has moved. Every level is read again on each — a file the agent just
   *  wrote is a row that has to appear without anybody folding the tree and opening it again, and a
   *  row that just went to the bin is one that has to stop being drawn. */
  moved: number;
  /** Which folders of the whole section are unfolded, as their paths joined. The panel holds it
   *  (`Opened`) because opening one is also something the section does on a reader's behalf, and
   *  because it has to outlive the section being unmounted. */
  open: string[];
  onOpen: (key: string) => void;
  /** The name being typed in this section, or nothing. */
  naming: Naming;
  onRead: (path: string[]) => void;
  onMenu: (path: string[], dir: boolean, x: number, y: number) => void;
  /** Put one row in the machine's bin — what Delete means on a row. */
  onTrash: (path: string[]) => void;
  /**
   * Which row holds the tree's one stop in the tab order, as its path joined — or nothing, before a
   * reader has been on any of them, when the stop is the first row of the whole tree.
   *
   * **One stop for the tree, not one per row.** Every row used to be a button, so a folder of a
   * thousand names was a thousand presses of Tab on the way past it — and nothing caps what a level
   * answers with (`crate::folder`). What a reader wants of a tree is to reach it once and then walk
   * it with the arrows, which is the pattern this is half of (roving tabindex).
   */
  cursor: string | null;
  onCursor: (key: string) => void;
  /**
   * The row of this section whose file is being read, as its path joined — or nothing.
   *
   * It is a mark on the tree and not a place in it: the file panel lies over the tree rather than
   * replacing it (`AMB-D-815`), so the row a reader opened is on the screen the whole time they are
   * reading it, and without a mark it is one name among the rest.
   */
  chosen: string | null;
}) {
  /**
   * The names of every folder on the screen, each with the reading of the section they were taken
   * at.
   *
   * **One holding for the tree, not one per level.** The rows are drawn as one list, so what they
   * are made of has to be in one place; the reading each level was taken at is what keeps a folder
   * that has just been opened from asking again for the levels that were already read, and what
   * makes every one of them ask again when the folder moves.
   */
  const [levels, setLevels] = useState<Record<string, { at: number; rows: FolderEntryDto[] }>>({});
  // The name being made in this section, and the folder it is being made in.
  const make = naming.edit?.kind === "make" && naming.edit.root === root ? naming.edit : null;
  const making = make === null ? null : make.into.join("/");
  const makingDir = make?.dir ?? false;
  // Held across renders, because it is what the read below watches: rebuilt every time, it would
  // run that effect on every render the panel makes for any reason at all.
  const shown = useMemo(() => shownFolders(open), [open]);

  useEffect(() => {
    let alive = true;
    for (const key of shown) {
      if (levels[key]?.at === moved) continue;
      void folderEntries(projectId, root, segmentsOf(key))
        .then((rows) => {
          if (alive) setLevels((was) => ({ ...was, [key]: { at: moved, rows } }));
        })
        .catch(() => {
          if (alive) setLevels((was) => ({ ...was, [key]: { at: moved, rows: [] } }));
        });
    }
    // A folder nobody is looking at any more is let go of, so that opening it again reads it: what
    // is held here is what is on the screen, and a level kept for a folder that is shut is a list
    // of names growing with every folder the reader has ever opened.
    if (Object.keys(levels).some((key) => !shown.includes(key))) {
      setLevels((was) => Object.fromEntries(
        Object.entries(was).filter(([key]) => shown.includes(key)),
      ));
    }
    return () => { alive = false; };
    // `levels` is what the effect writes, and reading it here is only to skip what is already in
    // hand — watching it would run this again for every level that came back.
  }, [projectId, root, shown, moved]);

  const lines = useMemo(
    () => linesOf(levels, open, making, makingDir, []),
    [levels, open, making, makingDir],
  );
  /**
   * The rows alone, in the order a reader goes down them — what the keys walk.
   *
   * **The order the walk asks is this one and not the document's.** A row on the screen and a row
   * in the document are about to stop being the same set of rows (`AMB-T-4108`), and a walk read
   * off the document would then send End to the last row that happens to be drawn and leave a
   * letter's match unfound because it is above or below the window.
   */
  const rows = useMemo(
    () => lines.flatMap((line) => (line.kind === "row" ? [line] : [])),
    [lines],
  );

  /**
   * The row a press named, as one answer per press.
   *
   * **The press names a row; standing on it is what happens next.** They are two things because
   * the row a press names need not be in the document at the moment it is pressed
   * (`AMB-T-4108`) — the walk is over the rows, and the document is caught up with afterwards.
   *
   * **An answer and not a name**, because the same row can be named twice with something else in
   * between: a reader who clicks one row and presses End again is asking to be taken back to the
   * last row, and a bare key would read as the answer that was already given.
   */
  const [named, setNamed] = useState<{ key: string } | null>(null);
  const tree = useRef<HTMLUListElement | null>(null);

  /**
   * Which run of the lines is in the document, or nothing for all of them.
   *
   * **Nothing until the box has been laid out**, and nothing again wherever it has no height to
   * answer with: a box that has not been measured says nothing about what is in view, and drawing
   * the whole tree is the answer that is never wrong.
   */
  const [win, setWin] = useState<{ from: number; to: number } | null>(null);

  // What is in view, read off the box the panel scrolls in. Before the paint rather than after it,
  // so that the first drawing of a tree is already the run that is on the screen.
  useLayoutEffect(() => {
    const box = scroller.current;
    if (box === null) return;
    const look = () => {
      const ul = tree.current;
      const tall = box.clientHeight;
      if (ul === null || tall === 0) { setWin(null); return; }
      // How far the list's top stands above the box's — negative while it is still below it, which
      // is a first row of nought once it is floored.
      const above = box.getBoundingClientRect().top - ul.getBoundingClientRect().top;
      const first = Math.floor(above / ROW);
      const from = Math.max(0, first - SPARE);
      const to = Math.min(lines.length, first + Math.ceil(tall / ROW) + SPARE);
      setWin((was) => (was !== null && was.from === from && was.to === to ? was : { from, to }));
    };
    look();
    box.addEventListener("scroll", look, { passive: true });
    if (typeof ResizeObserver === "undefined") return () => box.removeEventListener("scroll", look);
    const sized = new ResizeObserver(look);
    sized.observe(box);
    return () => {
      box.removeEventListener("scroll", look);
      sized.disconnect();
    };
  }, [scroller, lines.length]);

  /**
   * The box a name is being typed into, as its place among the lines — or nothing.
   *
   * **It is drawn wherever the reader has scrolled to.** What is typed into the box lives in the
   * box, so a window that dropped it would take a half-written name with it. Reaching it costs the
   * rows between only for as long as a name is open, and a reader who has just begun typing one is
   * looking at it.
   */
  const typing = useMemo(() => {
    const edit = naming.edit;
    if (edit === null || edit.root !== root) return -1;
    const key = edit.kind === "rename" ? edit.path.join("/") : null;
    return lines.findIndex((line) => (key === null
      ? line.kind === "make"
      : line.kind === "row" && line.key === key));
  }, [lines, naming.edit, root]);

  const from = Math.min(win?.from ?? 0, typing < 0 ? Infinity : typing);
  const to = Math.max(win?.to ?? lines.length, typing + 1);
  const drawn = useMemo(() => lines.slice(from, to), [lines, from, to]);

  /**
   * Which of the drawn rows holds the tree's one stop in the tab order.
   *
   * The row a reader was last on where it is drawn, and the first row on the screen where it is
   * not: a tree whose only stop has been scrolled out of the document is one Tab walks straight
   * past, and the reader has no way back into it.
   */
  const stop = useMemo(() => {
    const keys = drawn.flatMap((line) => (line.kind === "row" ? [line.key] : []));
    return keys.find((key) => key === cursor) ?? keys[0] ?? null;
  }, [drawn, cursor]);

  /**
   * Stand on the row that was named.
   *
   * **Two turns where the row is not drawn.** The keys walk the rows and the window holds only
   * some of them, so a row named from the far side of the tree has to be scrolled to before there
   * is anything to stand on: the box is moved here, the drawing that follows puts the row in, and
   * this runs again with it in hand. `focus` is what scrolls it the rest of the way where it is
   * only half on the screen, which is the browser's own answer and the one Tab gives too.
   */
  useEffect(() => {
    if (named === null) return;
    const ul = tree.current;
    const row = ul?.querySelector<HTMLElement>(`[data-key="${CSS.escape(named.key)}"]`) ?? null;
    if (row !== null) { row.focus(); return; }
    const box = scroller.current;
    const at = lines.findIndex((line) => line.kind === "row" && line.key === named.key);
    if (ul === null || box === null || at < 0) return;
    const y = ul.getBoundingClientRect().top - box.getBoundingClientRect().top + at * ROW;
    if (y < 0) box.scrollTop += y;
    else if (y + ROW > box.clientHeight) box.scrollTop += y + ROW - box.clientHeight;
  }, [named, from, to]);

  /**
   * Walking the tree with the keys the tree pattern names, and no others (`AMB-D-780`).
   *
   * **Read off the drawn rows.** They are one list in the order a reader sees, so what is next below
   * an open folder is the next row and nothing has to be rebuilt to find it. The folder holding a
   * row is no box around it, so the way out of one is found by the row's path.
   *
   * A key with a modifier on it is not this tree's: the panel around it hears undo (`FilesPanel`),
   * and what the reader means by ⌘ or Ctrl is the machine's word, never a row's.
   */
  const onKey = (e: ReactKeyboardEvent<HTMLUListElement>) => {
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    const on = (e.target as HTMLElement).closest<HTMLElement>('[role="treeitem"]');
    if (on === null || !e.currentTarget.contains(on)) return;
    // The row the press was on, found in the order the rows are drawn in. The press itself is the
    // one thing the document has to answer for: whatever else is on the screen, a key arrives on a
    // row that is.
    const at = rows.findIndex((one) => one.key === (on.dataset.key ?? ""));
    const here = rows[at];
    if (here === undefined) return;
    const go = (to: Row | undefined) => { if (to !== undefined) setNamed({ key: to.key }); };
    switch (e.key) {
      case "ArrowDown": e.preventDefault(); go(rows[at + 1]); break;
      case "ArrowUp": e.preventDefault(); go(rows[at - 1]); break;
      case "Home": e.preventDefault(); go(rows[0]); break;
      case "End": e.preventDefault(); go(rows[rows.length - 1]); break;
      // Into the folder: open it where it is shut, and step to what is inside where it is already
      // open. A shut one does not also move — what was asked for is to see inside, and the rows to
      // move onto are not read off the disk yet.
      case "ArrowRight":
        e.preventDefault();
        if (here.isDir && !here.unfolded) onOpen(here.key);
        else if (here.unfolded) go(rows[at + 1]);
        break;
      // And out of it: shut an open folder, or leave a row for the folder holding it — which is the
      // row whose path is this one's without its last name. A row at the top of the tree has none.
      case "ArrowLeft": {
        e.preventDefault();
        if (here.unfolded) { onOpen(here.key); break; }
        if (here.in === "") break;
        go(rows.find((one) => one.key === here.in));
        break;
      }
      case "Enter":
        e.preventDefault();
        if (here.isDir) onOpen(here.key);
        else onRead(here.path);
        break;
      // Both keys, because which one a keyboard calls "delete" is the keyboard's answer: a Mac's
      // large one sends Backspace, and what it means on a row is the same thing either way. The bin
      // is what happens, and the question before it is the panel's (`FilesPanel`).
      case "Delete":
      case "Backspace":
        e.preventDefault();
        onTrash(here.path);
        break;
      // Renaming, from the keyboard. Enter cannot be the key here — on a tree row it already means
      // open, which is the thing a reader presses it for; F2 is what the editors this tree is read
      // beside all answer to, and it collides with nothing.
      case "F2":
        e.preventDefault();
        naming.rename(here.path);
        break;
      default:
        // A letter typed on the tree is a way of walking it: the next row below whose name starts
        // that way, wrapping past the end, so that pressing the same letter again goes on to the
        // one after it.
        if (e.key.length === 1 && e.key !== " ") {
          const want = e.key.toLowerCase();
          // From the row after this one, all the way round to this one — so a tree with one match
          // stays where it is rather than reading as a key that did nothing.
          for (let step = 1; step <= rows.length; step += 1) {
            const to = rows[(at + step) % rows.length];
            if (to !== undefined && to.name.toLowerCase().startsWith(want)) {
              e.preventDefault();
              go(to);
              break;
            }
          }
        }
        break;
    }
  };

  /** How far in a line is drawn. The step itself is the stylesheet's (`../styles/global.css`). */
  const step = (depth: number) => ({ "--depth": depth } as CSSProperties);

  return (
    <ul
      ref={tree}
      className="files__list files__list--tree"
      // One list, and every row of the tree a line of it. How deep a row is and how many it stands
      // among are said on the row itself, which is the shape a reader being read to hears the same
      // tree in (`AMB-D-780`).
      role="tree"
      aria-label={t("files.tree")}
      onKeyDown={onKey}
    >
      {/* What the rows nobody is looking at leave behind: their height, so that the list is as tall
          as the tree is and the scrollbar says how much of it there is. */}
      {from > 0 && <li role="none" aria-hidden="true" style={{ height: from * ROW }} />}
      {drawn.map((line) => {
        if (line.kind === "make") {
          return (
            <li key="make" role="none" style={step(line.depth)}>
              <NameBox
                initial=""
                onName={(name) => folderMake(
                  projectId, root, [...segmentsOf(line.at), name], line.dir,
                )}
                onEnd={naming.end}
              />
            </li>
          );
        }
        // The row is the box while its name is being written over. Drawn in place of the row rather
        // than beside it: what is being changed is this name, and two of them on the screen at once
        // would leave a reader wondering which one they were about to keep.
        if (renaming(naming.edit, root, line.path)) {
          return (
            <li key={line.key} role="none" style={step(line.depth)}>
              <NameBox
                initial={line.name}
                onName={(name) => folderRename(projectId, root, line.path, name)}
                onEnd={naming.end}
              />
            </li>
          );
        }
        // Folded folders answer for what is under them (`AMB-D-795`); an open one leaves that to the
        // rows it is showing, and a file is only ever itself.
        const mark = marks(line.path, line.isDir && !line.unfolded);
        // A folder answers for a drop, and so does a file's row — for the folder holding it, which
        // is why dropping on a name means the same as dropping in the space beside it. The rows are
        // siblings, so a file's row carries that folder itself rather than resolving up to it.
        const into = line.isDir ? line.key : line.in;
        const lands = landed(landing, root, line.isDir ? line.key : undefined);
        return (
          <li
            key={line.key}
            role="treeitem"
            data-key={line.key}
            data-root={root}
            data-into={into}
            style={step(line.depth)}
            // Depth, length and place. All three because the rows are one flat list: nothing about
            // where a row sits in the document says how far down it is or how many it stands among.
            aria-level={line.depth + 1}
            aria-setsize={line.setsize}
            aria-posinset={line.posinset}
            aria-expanded={line.isDir ? line.unfolded : undefined}
            tabIndex={line.key === stop ? 0 : -1}
            className={`files__item${lands ? " files__into" : ""}`}
            onClick={() => {
              if (line.isDir) onOpen(line.key);
              else onRead(line.path);
            }}
            // Stood on before the menu opens, because the row a menu is about is the row a reader
            // comes back to when it closes — and a right-click is not a press the browser moves the
            // focus for.
            onContextMenu={(e) => {
              e.preventDefault();
              e.stopPropagation();
              e.currentTarget.focus();
              onMenu(line.path, line.isDir, e.clientX, e.clientY);
            }}
            // Where the tab stop follows to, however the row was reached — the arrows move the
            // focus and this is what moves the stop after it, so tabbing away and back returns to
            // the row a reader was on rather than to the top of the tree.
            onFocus={(e) => { if (e.target === e.currentTarget) onCursor(line.key); }}
          >
            {line.isDir
              ? (
                <span className={rowClass("files__dir", line.ignored, mark, chosen === line.key)}>
                  <span className="files__twisty">
                    <Icon name={line.unfolded ? "chevronDown" : "chevronRight"} />
                  </span>
                  <span className="files__kind"><Icon name="folder" /></span>
                  <span className="files__name">{line.name}</span>
                </span>
              )
              : (
                <span className={rowClass("files__file", line.ignored, mark, chosen === line.key)}>
                  {/* Empty, and there anyway: it is what puts the name at the same place as the
                      name of the folder above it (`../styles/global.css`). */}
                  <span className="files__twisty" />
                  {/* One mark for every file and no second one for what kind it is. A row says
                      whether it is a folder or a file, which is the whole of what has to be read
                      without reading the name; an icon per extension is a legend to learn. */}
                  <span className="files__kind"><Icon name="document" /></span>
                  <span className="files__name">{line.name}</span>
                </span>
              )}
          </li>
        );
      })}
      {to < lines.length && (
        <li role="none" aria-hidden="true" style={{ height: (lines.length - to) * ROW }} />
      )}
    </ul>
  );
}

/**
 * How a file's lines end, once it is a thing a save can be asked for.
 *
 * `null` is the file that has both kinds and has not been asked about yet — the one state where a
 * save is refused for a reason that is not about the file being unsavable (`AMB-D-773`).
 */
type Newline = "lf" | "crlf" | null;

/**
 * A name being typed, in the row it is about.
 *
 * **The refusal is drawn here and nowhere else.** Which names a machine will hold is the one thing a
 * person cannot work out for themselves — a name already taken, a name Windows reads as syntax, a
 * disk that is full — and the place to say so is where they are still typing, with what they wrote
 * still in front of them. So a refusal keeps the box open and takes the focus back, and only an
 * answer that went through ends it.
 *
 * Leaving the box is keeping what is in it, which is what a file manager does and what a person
 * clicking away plainly means. Escape is the way to mean the other thing, and so is leaving a refusal
 * standing: a name the machine has already said no to is not one to ask about twice.
 *
 * ⚠ **Nothing here may be disabled while the answer is on its way.** Disabling the box a person is
 * typing in blurs it, and blur is one of the two ways of keeping a name — so the refusal that was
 * about to arrive would land on a box that had already closed itself, and the reader would see their
 * name vanish with nothing said. What stops a second ask is the flag, not the disabling.
 */
function NameBox({ initial, onName, onEnd }: {
  /** What the box starts with: the name being written over, or nothing for one being made. */
  initial: string;
  onName: (name: string) => Promise<void>;
  /** `true` where the folder now holds something it did not, which is what sends the levels back to
   *  read their names again. */
  onEnd: (made: boolean) => void;
}) {
  const [name, setName] = useState(initial);
  const [refused, setRefused] = useState<string | null>(null);
  const [asking, setAsking] = useState(false);
  const box = useRef<HTMLInputElement | null>(null);
  // Ending is once. Enter is followed by the box leaving the screen, and a browser blurs what it
  // takes away — so without this the same name would be asked for twice.
  const ended = useRef(false);
  const end = (made: boolean) => {
    if (ended.current) return;
    ended.current = true;
    onEnd(made);
  };

  // The box is the reader's the moment it is drawn — it took the place of a row they were standing
  // on, and a box they had to click into first would be one they typed past.
  //
  // **Standing on the stem, not on the whole name.** Nearly every rename keeps the extension, and
  // with it selected the first keystroke takes it away — so a reader who meant to retype four
  // letters has to put `.md` back by hand. A name that is all stem is selected whole, which is what
  // the box did for every name before.
  useEffect(() => {
    box.current?.focus();
    box.current?.setSelectionRange(0, stemEnd(initial));
  }, [initial]);

  const keep = () => {
    // An answer already on its way, or a box already closed. Neither is a second name to ask for.
    if (asking || ended.current) return;
    // A refusal still standing is the same name the machine has just said no to. Leaving is giving
    // up on it — typing changes it, and typing is what clears the refusal.
    // Nothing typed or nothing changed is the same answer for the other reason: there is no name to
    // ask for, and asking for the one it already has would come back as taken.
    if (refused !== null || name === "" || name === initial) {
      end(false);
      return;
    }
    setAsking(true);
    onName(name)
      .then(() => end(true))
      .catch((e: unknown) => {
        setRefused(errText(e));
        setAsking(false);
        box.current?.focus();
      });
  };

  return (
    <>
      <input
        {...asTyped}
        ref={box}
        className="files__namebox"
        aria-label={t("files.name")}
        value={name}
        onChange={(e) => { setName(e.target.value); setRefused(null); }}
        onKeyDown={(e) => {
          if (e.key === "Enter") keep();
          // Kept off the face around it: Escape closes the panel's other states as well, and one
          // press should undo one thing.
          if (e.key === "Escape") { e.stopPropagation(); end(false); }
        }}
        onBlur={keep}
      />
      {refused !== null && <p className="files__none">{refused}</p>}
    </>
  );
}

/**
 * Whether a refusal is the host saying the file moved under the reader (`crate::folder_save`).
 *
 * It is the one save refusal the panel acts on rather than prints: every other is a sentence and a
 * reader who can try again, where this one has an answer of its own to offer (`AMB-D-784`).
 */
function changedUnderneath(e: unknown): boolean {
  return typeof e === "object" && e !== null
    && (e as { code?: unknown }).code === "folder_changed_underneath";
}

/** One file, as far as a panel can show it. */
function FileReader({
  projectId, root, path, onBack, onOpenLedger, onTrash, onKey, close, aside, onHandOver,
}: {
  projectId: number;
  root: string;
  path: string[];
  onBack: () => void;
  onOpenLedger?: () => void;
  /** Send the file being read to the machine's bin. The panel takes it off the screen from there. */
  onTrash: () => void;
  /** Undo, heard here for the same reason it is heard on the list: a file can go to the bin from
   *  this state too (`./FilesPanel`). */
  onKey: (e: ReactKeyboardEvent) => void;
  /** The panel's own way out, drawn on this row: reading a file is not a state a reader should have
   *  to leave before they can close the panel (`./FilesPanel`). */
  close: ReactNode;
  /** The question about the bin and the last refusal, both of which outlive this state. */
  aside: ReactNode;
  /** Hand this file to the pane being worked in, where there is one (`./FilesPanel`). */
  onHandOver?: (whole: string) => void;
}) {
  const [file, setFile] = useState<FolderFileDto | null>(null);
  // Why the file did not open, in the reader's own language. A link is not a broken file: the host
  // refuses one on purpose (`AMB-D-782`), and a person sharing a `CLAUDE.md` between projects that
  // way is the first to meet it — so that refusal is drawn in its own words and everything else
  // keeps the one sentence there is nothing finer to say than.
  const [failed, setFailed] = useState<string | null>(null);
  // The encoding the reader named, once they have. Nothing until then: the host's guess is right
  // for 644 files in 645, and asking for one up front would be putting the question to everybody
  // to catch the one (`AMB-D-773`).
  const [asked, setAsked] = useState<string | undefined>(undefined);
  // Where the list of encodings was opened from, drawn like the file menu because it is the same
  // kind of thing: a short list of answers to one question, at the control that asked it.
  const [picking, setPicking] = useState<{ x: number; y: number } | null>(null);
  // The way to read what is in the editor, handed over once it is up. Nothing is saved before that:
  // the editor is where the text is (`./FileEditor`).
  const typed = useRef<(() => string) | null>(null);
  // Whether there is anything to save. It is set by the editor telling this side that a person
  // typed, rather than by comparing texts — the comparison would mean holding a second copy of the
  // document up here and reading it on every keystroke.
  const [edited, setEdited] = useState(false);
  const [keeping, setKeeping] = useState(false);
  // Why the last save did not happen, in the reader's own language. Cleared when another is tried.
  const [refused, setRefused] = useState<string | null>(null);
  // Which newline to write. A file with one kind keeps it; a file with both has none until the
  // reader picks, and the save waits for that rather than guessing.
  const [newline, setNewline] = useState<Newline>(null);
  // Whether the file moved under a reader who has typed. Nothing of theirs is taken away by it —
  // reading the file again is a thing they ask for, and this is the asking (`AMB-D-784`).
  const [stale, setStale] = useState(false);
  // Where a picture too large to draw was handed on to the machine from. The same menu the list
  // rows open, opened here because this is the one state a reader reaches it from with no row under
  // the pointer.
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  // Whether a Markdown file is being shown as the text it is rather than as what that text draws.
  // **It goes back with every file opened**, deliberately: what a person opens a Markdown file for
  // is to read it, and a choice that outlived the file would be a setting nobody set — one edit and
  // every Markdown file afterwards opens as source, the ones they only wanted to read included.
  const [asText, setAsText] = useState(false);
  const name = path[path.length - 1];
  // The one thing the name decides, and the only file there are two ways to show (`MARKDOWN`).
  const markdown = MARKDOWN.some((ext) => name.toLowerCase().endsWith(ext));

  // A different file is a different question: what the reader named was this file's encoding, and
  // carrying it to the next one would open that one in an encoding nobody chose for it.
  useEffect(() => setAsked(undefined), [projectId, root, path.join("/")]);

  // What the file was as it was last read, and whether there is anything of the reader's to lose by
  // replacing it. Held in a ref rather than read out of the effect below: that effect is subscribed
  // once per file, and taking these as reasons to re-subscribe would install a fresh watch over the
  // folder the first time somebody typed.
  const held = useRef({ edited, digest: file?.digest });
  held.current = { edited, digest: file?.digest };

  // One file as it has just been read. The newline travels with the text because a file read again
  // is a file whose lines may end differently than they did — and both kinds in one file is the one
  // answer this side cannot act on by itself.
  const take = (one: FolderFileDto) => {
    setFile(one);
    setNewline(one.lineEnding === "mixed" ? null : one.lineEnding);
  };

  useEffect(() => {
    let alive = true;
    setFile(null);
    setFailed(null);
    setAsText(false);
    setEdited(false);
    setRefused(null);
    setNewline(null);
    setStale(false);
    void folderRead(projectId, root, path, asked)
      .then((one) => { if (alive) take(one); })
      .catch((e) => {
        if (alive) setFailed(isErr(e, "folder_link") ? errText(e) : t("files.unreadable"));
      });
    return () => { alive = false; };
  }, [projectId, root, path.join("/"), asked]);

  // The file moving under the reader while they have it open.
  //
  // **This face watches the folder itself**, because the tree that was watching it is not on the
  // page while a file is being read — the panel draws one or the other. What arrives says only that
  // the folder moved (`AMB-D-785`), so the answer is to read the file again and compare the mark:
  // the same mark is this file standing still while something else in the folder changed, which is
  // most of what arrives here and draws nothing at all.
  //
  // **A reader who has typed nothing is simply shown what the file says now.** This panel sits
  // beside an agent that edits the same files, and a reader looking at what it changed an hour ago
  // reads it as the agent having done nothing (`AMB-D-784`).
  //
  // **A picture travels this road too and needs nothing of its own** (`AMB-D-797`). It has a mark
  // like any other file, nobody can have typed into it, and what redraws it is the address it is
  // fetched from carrying that mark. What is not watched is what is not drawn: a picture refused
  // for its size, and a binary.
  const tracked = file?.digest !== undefined;
  useEffect(() => {
    if (!tracked) return;
    let alive = true;
    const look = () => {
      // In the encoding the reader named, where they named one: a file read again in a guess they
      // had already overruled would put the panel back where they started (`AMB-D-773`).
      void folderRead(projectId, root, path, asked)
        .then((fresh) => {
          if (!alive || fresh.digest === undefined || fresh.digest === held.current.digest) return;
          if (held.current.edited) setStale(true);
          else take(fresh);
        })
        // A read that did not answer leaves what is drawn where it is. The file may be being
        // written this very instant, and taking a reader's text off the screen for a moment of the
        // disk's is worse than being a moment out of date — a file that has really gone is what the
        // save then says, to somebody who asked for it.
        .catch(() => {});
    };
    // Subscribed before the watch is asked for, the same order the tree takes: the first thing the
    // folder does could happen while the host is still walking it.
    const listening = onFolderChanged((changes) => { if (alive && changes.root === root) look(); });
    void folderWatch(projectId, root).catch(() => {});
    return () => {
      alive = false;
      void listening.then((stop) => stop());
      void folderUnwatch(root);
    };
  }, [projectId, root, path.join("/"), tracked, asked]);

  // Taking what is on the disk now, over what the reader has typed. It is the one thing here that
  // loses somebody's work, which is why nothing does it on their behalf (`AMB-D-784`).
  const readAgain = () => {
    void folderRead(projectId, root, path, asked)
      .then((fresh) => { take(fresh); setEdited(false); setStale(false); setRefused(null); })
      .catch((e) => setFailed(isErr(e, "folder_link") ? errText(e) : t("files.unreadable")));
  };

  // Whether this file is one the panel can write back at all. The host says so before a reader has
  // typed a character: a file cut at the read cap, or one whose bytes and text do not round-trip,
  // is drawn read-only from the start (`AMB-D-773`).
  //
  // **A Markdown file being drawn is not one of them.** There is no editor on the rendering, so
  // there is no text to write and nothing a save could mean — the switch beside the name is what
  // makes it savable, by putting the text on the screen.
  const savable = file?.text !== undefined
    && file.encoding !== undefined
    && !file.truncated
    && file.clean
    && (!markdown || asText);

  const save = async () => {
    const read = typed.current;
    if (!savable || keeping || file?.encoding === undefined || file.digest === undefined
      || read === null || newline === null) return;
    setKeeping(true);
    setRefused(null);
    try {
      const kept = await folderSave(
        projectId, root, path, read(), file.encoding, file.bom, newline, file.digest,
      );
      setEdited(false);
      // What is on the disk now has one kind of newline, so the question is not asked again, and it
      // is the mark this save came back with — without taking that, the panel's next look at the
      // folder would find its own writing and read it as somebody else's (`AMB-D-784`). The text is
      // left where it is: replacing it would be handing the editor its own document back and moving
      // the caret to the top for the trouble.
      setFile({ ...file, lineEnding: newline, digest: kept });
    } catch (e) {
      // The one refusal that is not a sentence to read and be done with: the file moved under the
      // reader, and what that wants is the offer below rather than a line of prose.
      if (changedUnderneath(e)) setStale(true);
      else setRefused(errText(e));
    } finally {
      setKeeping(false);
    }
  };

  // The keystroke everything else in the world saves with. It is taken on the window rather than
  // inside the editor because the reader may have clicked away from it — and it is taken only
  // while there is something to save, so nothing is swallowed on a file that cannot be.
  useEffect(() => {
    if (!savable) return;
    const key = (e: KeyboardEvent) => {
      if (e.key !== "s" || !(e.metaKey || e.ctrlKey) || e.altKey) return;
      e.preventDefault();
      void save();
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  });

  // A reference in a file is a live link or it is nothing at all (`AMB-D-747`), and following one
  // leaves this face: what a record opens on is the ledger.
  const nav = useLedgerNav(onOpenLedger);

  // What the row under the name has on it. Named here rather than asked three times in the markup,
  // because whether the row exists at all is the same question as whether anything would be on it.
  const switchable = file?.text !== undefined && markdown;
  const readAs = file?.text !== undefined && file.encoding !== undefined;

  return (
    <div className="files files--reading" tabIndex={-1} onKeyDown={onKey}>
      {/* **The name has a row to itself, and what to do with the file has another.** The two used to
          share one, and the name was the only thing on it that could give way — every control beside
          it is as wide as its own words — so the name is what disappeared: `run.sh` came up as
          `r...` on a panel of ordinary width, which leaves a reader unable to say which file they
          are looking at. It got that way one control at a time, and no one of them was the mistake
          (`AMB-T-3866` measured the state the three arrived at).

          The split is by what a control is for, not by what fits: leaving and closing are the frame's
          and stay with the name, and the ones that act on the file stand together under it. The
          second row is drawn only where there is something to put on it, so a picture — which has
          nothing to switch, nothing to reopen and nothing to save — costs no line at all.

          The bin stays up here though it acts on the file, because it is a mark and not a word: an
          icon is the same narrow width whatever the reader's language, which is exactly what the
          three that moved were not. */}
      <div className="files__bar">
        <button className="files__back" onClick={onBack}>{t("files.back")}</button>
        <span className="files__name" title={path.join("/")}>{name}</span>
        <button className="files__trash" title={t("files.trash")} onClick={onTrash}>
          <Icon name="trash" />
        </button>
        {close}
      </div>
      {(switchable || readAs || savable) && (
        <div className="files__tools">
          {/* Drawn for a Markdown file and for nothing else: every other file has one way to be
              shown, and a switch with nowhere to switch to is a control that answers nothing. What
              it says is where it goes rather than where it is — the reader can see where they
              are. */}
          {switchable && (
            <button className="files__view" onClick={() => setAsText((was) => !was)}>
              {t(asText ? "files.read" : "files.edit")}
            </button>
          )}
          {/* What the bytes were read as. The guess reports no confidence and breaks nothing visible
              when it is wrong, so the reader is the only one who can catch it — and they can only
              catch it if they are told what was guessed (`AMB-D-773`). Text only: a picture has no
              encoding to be wrong about. */}
          {file?.text !== undefined && file.encoding !== undefined && (
            <button
              className="files__encoding"
              title={t("files.reopenWith")}
              onClick={(e) => setPicking({ x: e.clientX, y: e.clientY })}
            >
              {file.encoding}
              {" · "}
              {file.lineEnding === "mixed" ? t("files.lineEndingMixed") : file.lineEnding.toUpperCase()}
            </button>
          )}
          {/* One control saying which of three things is true, rather than a button and a word
              somewhere else for a reader to find the answer in. */}
          {savable && (
            <button
              className="files__keep"
              disabled={!edited || keeping || newline === null}
              onClick={() => { void save(); }}
            >
              {keeping ? t("files.saving") : edited ? t("files.save") : t("files.saved")}
            </button>
          )}
        </div>
      )}
      {aside}
      <div className="files__body">
        {failed !== null && <p className="files__none">{failed}</p>}
        {/* The picture is fetched rather than carried: `folderRead` says only that there is one
            and what type it is, and the door that hands out a file by its path is addressed with
            the same project, folder and path this reader was opened on (`AMB-D-783`). It draws
            top to bottom as it arrives, where a `data:` URL drew all at once or not at all.

            The mark goes on the address so that the picture is fetched again when — and only
            when — the file behind it moved (`AMB-D-797`). Without it the address of a rewritten
            picture is the address of the old one, and the reader watches an agent redraw a diagram
            that never changes on screen. */}
        {file?.image !== undefined && (
          <img
            className="files__image"
            alt={name}
            src={fileUrl(projectId, root, path, file.image.mime, file.digest)}
          />
        )}
        {/* The text is what the file holds and the rendering is a view of it (`AMB-D-41`), so the
            editor is reachable for a Markdown file too — otherwise the one kind of file an agent
            writes most is the one kind nobody could correct. */}
        {file?.text !== undefined && (
          markdown && !asText
            ? <RefNavProvider value={nav}><Markdown>{file.text}</Markdown></RefNavProvider>
            : (
              <FileEditor
                text={file.text}
                editable={!file.truncated && file.clean}
                name={name}
                onEdit={() => setEdited(true)}
                hold={(read) => { typed.current = read; }}
              />
            )
        )}
        {/* Said before the reader types rather than after they press save: a file with both kinds
            of newline comes out of a save with one, and that is a change to every line of the other
            kind (`AMB-D-773`).
            The choice sits with the sentence that explains it rather than up in the bar — the bar
            is as wide as the panel, and a control there would push the file's own name off it. */}
        {savable && file?.lineEnding === "mixed" && (
          <div className="files__newlines">
            <p className="files__none">{t("files.newlinesMixed")}</p>
            <select
              className="files__newline"
              aria-label={t("files.newlineChoose")}
              value={newline ?? ""}
              onChange={(e) => setNewline(e.target.value === "crlf" ? "crlf" : "lf")}
            >
              <option value="" disabled>{t("files.newlineChoose")}</option>
              <option value="lf">{t("files.newlineLf")}</option>
              <option value="crlf">{t("files.newlineCrlf")}</option>
            </select>
          </div>
        )}
        {/* The file moved under the reader while they were typing in it. What is said is the fact
            and nothing else, and what is offered is the one thing this panel can do about it:
            lining the two texts up is the work of the agent in the pane (`AMB-D-784`). */}
        {stale && (
          <div className="files__changed">
            <p className="files__none">{t("files.changedUnderneath")}</p>
            <button className="files__reread" onClick={readAgain}>{t("files.readAgain")}</button>
          </div>
        )}
        {refused !== null && <p className="files__none">{refused}</p>}
        {/* A picture refused is not a picture missing. Drawn as nothing at all it reads as a
            damaged file, so the refusal says what it measured and hands the file on to something
            built to open it (`AMB-D-783`). */}
        {file?.oversize !== undefined && (
          <>
            <p className="files__none">{t("files.tooBig")}</p>
            <p className="files__none">{measured(file.oversize)}</p>
            <button
              className="files__hand"
              onClick={(e) => setMenu({ x: e.clientX, y: e.clientY })}
            >
              {t("files.tooBigOpen")}
            </button>
          </>
        )}
        {file !== null && file.text === undefined && file.image === undefined
          && file.oversize === undefined && (
          <p className="files__none">{t("files.notText")}</p>
        )}
        {file?.truncated === true && <p className="files__none">{t("files.cut")}</p>}
      </div>
      {menu !== null && (
        <FileMenu
          projectId={projectId}
          root={root}
          path={path}
          dir={false}
          at={menu}
          onClose={() => setMenu(null)}
          onTrash={onTrash}
          onHandOver={onHandOver}
        />
      )}
      {picking !== null && (
        <EncodingMenu
          at={picking}
          onPick={(one) => { setPicking(null); setAsked(one); }}
          onClose={() => setPicking(null)}
        />
      )}
    </div>
  );
}

/**
 * The encodings a file can be reopened in, as a list to pick from.
 *
 * **This is the items, not the box** — the same shell the file rows' menu wears
 * (`../components/Menu`). Written on its own it closed on every key, which is the bug `AMB-D-780`
 * took out of the other one and left standing here: a reader walking the list with the arrows shut
 * it on the way past.
 *
 * **The list comes from the host.** Which encodings may be offered is which ones can be written
 * back, and that is `crate::encoding`'s to say — a copy kept here would go on offering one the day
 * it stopped being written (`AMB-D-773`). It arrives after the box is drawn, so the names are what
 * the shell is told its face is: the item the reader was standing on is gone the moment they land.
 *
 * A file that is not clean is still on this road, and is the road's whole point: a guess that went
 * wrong is exactly the file whose bytes and text no longer say the same thing.
 */
function EncodingMenu({ at, onPick, onClose }: {
  at: { x: number; y: number };
  onPick: (encoding: string) => void;
  onClose: () => void;
}) {
  const [names, setNames] = useState<string[]>([]);

  useEffect(() => {
    let alive = true;
    void folderEncodings()
      .then((found) => { if (alive) setNames(found); })
      .catch(() => { if (alive) onClose(); });
    return () => { alive = false; };
  }, []);

  return (
    <Menu at={at} face={names} onClose={onClose}>
      {names.map((one) => (
        <MenuItem key={one} onClick={() => onPick(one)}>{one}</MenuItem>
      ))}
    </Menu>
  );
}

/**
 * What a refused picture is refused for, in the two numbers that were measured.
 *
 * The pixels are absent where the front of the file did not say — a picture that would not say its
 * size is refused on its bytes alone (`crate::folder`), and printing a size nobody read would be
 * inventing one.
 */
function measured(oversize: NonNullable<FolderFileDto["oversize"]>): string {
  const size = fileSize(oversize.bytes);
  if (oversize.width === undefined || oversize.height === undefined) return size;
  return `${size} · ${tf("files.tooBigPixels", {
    width: formatNumber(oversize.width),
    height: formatNumber(oversize.height),
  })}`;
}

/**
 * A file's size, in the unit that says something about it.
 *
 * **Megabytes alone would print "0 MB" for the case this exists to explain.** A picture is refused
 * on pixels as well as bytes, and the pictures that cost the most to draw are the ones that
 * compress best — ten kilobytes of lossless WebP decodes to over a gigabyte (`AMB-D-783`), and a
 * header claiming thirty thousand square costs less than a kilobyte to write. A refusal that reads
 * "0 MB" tells the reader the file is empty, which is the opposite of true, so the unit goes down
 * as far as bytes rather than ever rounding to nothing.
 *
 * The unit's own name comes from `Intl` rather than the dictionary: it is one of the things a
 * locale already knows how to write, down to `Mo` in French.
 */
function fileSize(bytes: number): string {
  const mib = 1024 * 1024;
  if (bytes >= mib) return formatNumber(bytes / mib, unit("megabyte", 1));
  if (bytes >= 1024) return formatNumber(bytes / 1024, unit("kilobyte", 0));
  return formatNumber(bytes, unit("byte", 0));
}

function unit(name: string, maximumFractionDigits: number): Intl.NumberFormatOptions {
  return { style: "unit", unit: name, unitDisplay: "short", maximumFractionDigits };
}
