// The folder tree: the rail's other half, where what the agent in the pane is doing to the folder
// can be seen without leaving the window (`AMB-T-3602`).
//
// **It stands on the rail's side of the panes, and the file being read on the other** (`AMB-D-835`).
// The two shared one column until now, and a file being read was drawn over the tree — so a reader
// who opened one could not reach for the next, which is what made picking several rows out a thing
// that was there and could not be used. Apart, the list is on the screen for as long as the reading
// is.
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
  CSSProperties, KeyboardEvent as ReactKeyboardEvent, PointerEvent as RowPress, RefObject,
} from "react";
import type { FolderChangesDto, FolderEntryDto, GitEntryDto } from "../bindings/bindings";
import { useBoundFolders } from "../core/boundFolders";
import { watchHostDrop } from "../core/hostDrop";
import { errText, t } from "../core/i18n";
import { asTyped } from "../core/keys";
import { pushNotice } from "../core/notice";
import { hostOs } from "../core/platform";
import {
  folderClipCopy, folderClipPaste, folderEntries, folderGitStatus, folderImport,
  folderMake, folderRename, folderUnwatch, folderWatch, onFolderChanged,
} from "./folder";
import { stoppedLine } from "./stopped";
import { FileMenu } from "./FileMenu";
import { useTrash } from "./trash";
import { fileAt } from "./fileUnder";
import { gitMarks, type GitMark } from "./gitMark";
import { sectionsOf } from "./sections";
import { Icon } from "../components/Icon";


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
 * **The panel holds it, not the tree.** The tree is unmounted the moment it is folded shut, and the
 * section holding it whenever the folder is unbound or the project switched away from — and what a
 * component holds goes with it. A reader who folds a tree away and opens it again is asking for it
 * back the way they left it, which is only possible if it was never theirs to lose.
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
  /**
   * The rows a reader has picked out, as their paths joined.
   *
   * **Not the row the keyboard is on, and not the file being read.** The cursor is where the next
   * key lands and `chosen` is what the panel is showing; this is the answer to "which ones", and it
   * is the only one of the three that can be more than one row (`AMB-T-4229`).
   */
  picked: string[];
  /**
   * The end a range is measured from, as its path joined — the last row picked without Shift.
   *
   * Held rather than read back off `picked`, because a range runs both ways: the rows in it say
   * which they are and not which end the reader started at, so a range pulled back past its own
   * start would grow the other way instead.
   */
  anchor: string | null;
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
const AT_FIRST: Opened = { treeOpen: true, open: [], cursor: null, picked: [], anchor: null };

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

/**
 * The rows an act aimed at one row is about: the ones picked out, where that row is among them —
 * and the row alone, where it is not.
 *
 * **One rule, read in two places.** The panel answers for the doors it draws — the menu, the keys —
 * and the tree answers for a row taken hold of, because the gesture starts before the panel hears
 * anything about it. Written twice they would drift, and a menu acting on five rows beside a drag
 * carrying one is exactly the kind of drift nobody would notice until it mattered.
 */
function rowsAbout(picked: string[], path: string[]): string[][] {
  return picked.includes(path.join("/")) ? picked.map(segmentsOf) : [path];
}

/**
 * Every folder this project is bound to, drawn as one tree per binding, and the acts that reach the
 * rows in them.
 *
 * **What a row opens is not drawn here.** A row pressed twice asks for the file, and the file is
 * drawn in the column on the other side of the panes (`AMB-D-835`). So what this side keeps is how
 * the trees are opened, which rows are picked out, and which name is being typed; the file being
 * read arrives as a prop, and all it does here is put the mark on the row it was opened from.
 */
export function FolderTree({
  projectId, reading, onRead, onGone, onHandOver, onCarry,
}: {
  /** The project whose folders the trees are rooted at; nothing is drawn without one. */
  projectId: number | null;
  /** The file the other column is reading, so the row it was opened from can say so. */
  reading: { root: string; path: string[] } | null;
  /** Ask for a file to be read. What answers is the column on the other side of the panes. */
  onRead: (at: { root: string; path: string[] }) => void;
  /** The rows that have gone to the bin, so a file being read that was one of them can be put away.
   *  The paths are joined, as they are drawn on the rows. */
  onGone?: (root: string, went: string[]) => void;
  /**
   * Hand rows to the pane the reader is working in, as the whole paths they are at — the reverse of
   * a path drawn in a pane opening the file beside them (`../shell/TerminalFace`).
   *
   * Which pane that is, and whether there is one at all, is the terminal face's own answer: with
   * none, none of this is handed down and the row's menu draws no item for it.
   */
  onHandOver?: (wholes: string[]) => void;
  /**
   * Take hold of a row, so it can be carried to a pane and let go there (`./handDrag`).
   *
   * The gesture belongs to the face for the same reason `onHandOver` does — where a path lands is a
   * pane's session, which the tree cannot see — so what this side does with it is hand it to every
   * row it draws. With none handed down, the rows are what they were: things to open.
   */
  onCarry?: (wholes: string[], event: RowPress<HTMLElement>) => void;
}) {
  // `0` names no project, which is what the folder read then answers with: none. A window with no
  // project on it draws the invitation, the same as one whose project has no folder.
  const folders = useBoundFolders(projectId ?? 0);
  // Every folder recorded rather than every folder that is there: one that has gone is a section
  // saying so, and it can only say so if it is still on the list.
  const sections = useMemo(() => sectionsOf(folders.all), [folders.all]);
  const live = folders.live.map((one) => one.path);
  // The file a right-click was on, and where the pointer was. One menu for the tree rather than one
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
  // the bound folder itself). Null while nothing is over the tree. The folder is half of it because
  // every section has a row for its own root, and the same path inside two of them is two places.
  const [landing, setLanding] = useState<Landing | null>(null);
  // How each bound folder's tree is opened, by the folder it is about — the folder is the key
  // because a project draws a section per binding and each of them is opened on its own (`Opened`).
  const [opened, setOpened] = useState<Record<string, Opened>>({});
  const box = useRef<HTMLDivElement | null>(null);
  // The bin, and the question before it. The reading column holds one of its own for the file it is
  // drawing: what is shared is how a press behaves, not one question for the two of them (`./trash`).
  const trash = useTrash(projectId, onGone);
  const roots = live.join("\0");

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

  /**
   * Pick rows out of one folder's tree, which puts down whatever was picked in another.
   *
   * **One selection for the rail, drawn in whichever section it is in.** The sections are how
   * several bound folders are drawn (`AMB-D-778`), not several selections to hold at once — and
   * what is done with picked rows is done to one folder's paths, so rows gathered across two of
   * them would be a selection nothing could act on.
   */
  const onPicked = (root: string, picked: string[], anchor: string | null) =>
    setOpened((was) => Object.fromEntries(sections.map((one) => {
      const now = was[one.path] ?? AT_FIRST;
      if (one.path === root) return [one.path, { ...now, picked, anchor }];
      return [one.path, now.picked.length === 0 ? now : { ...now, picked: [], anchor: null }];
    })));

  /**
   * The rows an act is about: the ones picked out, where the act was aimed at one of them — and the
   * row it was aimed at alone, where it was not.
   *
   * **Aimed at, not standing on.** A menu is opened on a row and a key is pressed on one; either
   * way the row under it decides whether the reader meant the selection or meant that row. Nothing
   * has to be put down first, because both doors put it down themselves: a right-click away from
   * the selection takes it (`Tree`), and so does a walk with the arrows (`AMB-T-4229`).
   *
   * **A row picked out and then folded away is still one of them.** What a fold takes off the
   * screen it gives back on opening, and the selection is held through it on purpose (`Tree`); the
   * count in the question before the bin is what says how many rows a press is about, not how many
   * of them happen to be drawn.
   */
  const actOn = (root: string, path: string[]): string[][] =>
    rowsAbout((opened[root] ?? AT_FIRST).picked, path);

  // Files dragged in from the desktop. The tree hears about them from the host rather than from
  // the DOM, so the highlight under the pointer — and the scroll when the pointer hangs at an edge —
  // are this side's to drive (`../core/hostDrop`).
  //
  // **The folder the highlight named is the folder the files are carried into** (`./folder`). Both
  // halves of the landing go to the host, not the path alone: every section draws a row for its own
  // root, and two projects' folders each holding a `src` are two places (`AMB-T-3781`).
  //
  // **What arrived is not said, and what did not is.** The folder is watched and the tree reads its
  // names again on every move, so a file that came in is about to be a row — a line saying so would
  // be a second, slower account of what is already drawn. A carry that stopped leaves nothing to
  // draw, and that is what the toast is for.
  useEffect(() => {
    if (projectId === null || sections.length === 0) return;
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
  }, [projectId, roots]);

  // The tree takes the focus once it has changed a folder, so that undo is the next thing a reader
  // can press. What did the changing was a menu item that is gone by the time the answer lands, and
  // a key nothing is focused on reaches nothing (`AMB-D-780`).
  useEffect(() => {
    if (trash.acted > 0) box.current?.focus();
  }, [trash.acted]);

  /**
   * The three keys the machine already has, heard on the tree rather than on the window: the
   * terminal beside it has its own idea of what each of them means, and the boundary between the two
   * is which of them the reader is in (`AMB-D-780`).
   *
   * **Copy and paste act on a row and nowhere else.** A file being read is drawn in an editor in the
   * other column, and the copy key there is the editor's — it copies the words somebody selected. So
   * neither key is taken unless the keyboard is standing on the tree, and the press falls through
   * untouched when it is not.
   */
  const onKey = (e: ReactKeyboardEvent) => {
    if (!(e.metaKey || e.ctrlKey) || e.shiftKey || e.altKey) return;
    const pressed = e.key.toLowerCase();
    if (pressed === "z") {
      e.preventDefault();
      trash.undo();
      return;
    }
    if (projectId === null) return;
    const on = e.target as HTMLElement;

    // The rows the copy is about, found from the row the keyboard is on: the ones picked out where
    // it is one of them, and that row alone where it is not (`actOn`).
    if (pressed === "c") {
      const row = on.closest<HTMLElement>('[role="treeitem"]');
      const root = row?.dataset.root;
      const path = row?.dataset.key;
      if (root === undefined || path === undefined) return;
      e.preventDefault();
      void folderClipCopy(projectId, root, actOn(root, segmentsOf(path)))
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

  // No project to root the tree in — the terminal face before it has been told which one it is on,
  // and the machine that has none at all. Nothing is said either way: what is on the screen is about
  // a project, and there is no project here for it to be about (`AMB-T-4358`).
  if (projectId === null) return <div className="files" />;

  if (sections.length === 0) {
    // A read that has not come back draws nothing at all: a flash of "no folder" on a project that
    // has one reads as a broken binding (`core/boundFolders`).
    return folders.answered
      ? <div className="files files--empty"><p className="files__none">{t("files.noFolder")}</p></div>
      : <div className="files" />;
  }

  return (
    // Focusable so the tree can hold the key it hears, and taken off the tab order so that being
    // able to hold it costs nobody a stop on the way past (`AMB-D-780`).
    <div className="files" ref={box} tabIndex={-1} onKeyDown={onKey}>
      {trash.aside}
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
          onRead={(path) => { setEdit(null); onRead({ root: one.path, path }); }}
          onMenu={(path, dir, x, y) => setMenu({ root: one.path, path, dir, x, y })}
          onTrash={(path) => trash.askTrash(one.path, actOn(one.path, path))}
          onPicked={(picked, anchor) => onPicked(one.path, picked, anchor)}
          chosen={reading !== null && reading.root === one.path ? reading.path.join("/") : null}
          onCarry={onCarry}
        />
      ))}
      {menu !== null && (
        <FileMenu
          projectId={projectId}
          root={menu.root}
          path={menu.path}
          about={actOn(menu.root, menu.path)}
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
          onTrash={() => trash.askTrash(menu.root, actOn(menu.root, menu.path))}
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
 * outlive both the tree, which goes whenever it is folded shut, and the section itself (`Opened`).
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
  onTrash, onPicked, chosen, onCarry,
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
  /** Pick rows of this folder out, and say which end a range would next be measured from. The
   *  panel takes it because picking here puts down what was picked in another section. */
  onPicked: (picked: string[], anchor: string | null) => void;
  /** The row of this folder whose file is being read, or nothing where the file being read is in
   *  another folder — or where none is. The panel works out which section it belongs to, since it
   *  is the panel that knows what is open. */
  chosen: string | null;
  /** Take hold of one of this folder's rows, to carry what the press is about to a pane
   *  (`./handDrag`). */
  onCarry?: (wholes: string[], event: RowPress<HTMLElement>) => void;
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
  const { treeOpen, open, cursor, picked, anchor } = opened;
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
            picked={picked}
            anchor={anchor}
            onPicked={onPicked}
            chosen={chosen}
            onCarry={onCarry}
          />
        )}
      </section>
    </div>
  );
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
 *
 * And the two grounds a row can stand on: picked out by the reader, and being read. A row is often
 * both — clicking a file does both at once — so which of the two is drawn is the stylesheet's
 * answer and not this function's (`../styles/global.css`).
 *
 * Named rather than counted off, because four flags in a row is four chances to hand them over in
 * the wrong order and no way to see it at the call.
 */
function rowClass(base: string, on: {
  ignored: boolean;
  mark: GitMark | null;
  picked: boolean;
  chosen: boolean;
}): string {
  let all = base;
  if (on.ignored) all += ` ${base}--ignored`;
  if (on.mark !== null) all += ` ${base}--git ${base}--git-${on.mark}`;
  if (on.picked) all += ` ${base}--picked`;
  if (on.chosen) all += ` ${base}--chosen`;
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
  onTrash, cursor, onCursor, picked, anchor, onPicked, chosen, onCarry,
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
   * The rows a reader has picked out, as their paths joined (`Opened`).
   *
   * **Picking is not standing.** One row carries the tab stop and one file is being read; what is
   * picked is a set, and it is what an act on "the files" is about (`AMB-T-4230`).
   */
  picked: string[];
  /** The end a range is measured from, or nothing before a reader has picked anything (`Opened`). */
  anchor: string | null;
  /** Pick rows out, and say which end a range would next be measured from. */
  onPicked: (picked: string[], anchor: string | null) => void;
  /**
   * The row of this section whose file is being read, as its path joined — or nothing.
   *
   * It is a mark on the tree and not a place in it: the file panel lies over the tree rather than
   * replacing it (`AMB-D-815`), so the row a reader opened is on the screen the whole time they are
   * reading it, and without a mark it is one name among the rest.
   */
  chosen: string | null;
  /** Take hold of a row, to carry what the press is about to a pane (`./handDrag`). */
  onCarry?: (wholes: string[], event: RowPress<HTMLElement>) => void;
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
   * The rows from one to another, both ends included, in the order they are drawn.
   *
   * Over the rows and not over the document, for the reason every walk here is: a range whose far
   * end has been scrolled out of the window would otherwise stop at the edge of what happens to be
   * drawn (`AMB-T-4108`). A row that is no longer there at all names no range — the tree moved
   * under the reader, and the press was about a row that has gone.
   */
  const between = (from: string, to: string): string[] => {
    const end = rows.findIndex((one) => one.key === to);
    if (end < 0) return [];
    const at = rows.findIndex((one) => one.key === from);
    const start = at < 0 ? end : at;
    return rows.slice(Math.min(start, end), Math.max(start, end) + 1).map((one) => one.key);
  };

  // A name the folder no longer holds is picked no longer. What decides is the level's own answer
  // and never the rows on the screen: a folder somebody folded shut is let go of here (`levels`),
  // and a row whose folder is not in hand stays picked — so folding one and opening it again gives
  // the reader back what they had, while a file that went to the bin is out of the selection before
  // anything can be asked of it (`AMB-T-4230`).
  useEffect(() => {
    const held = (key: string): boolean => {
      const at = key.split("/");
      const level = levels[at.slice(0, -1).join("/")];
      return level === undefined || level.rows.some((one) => one.name === at[at.length - 1]);
    };
    if (picked.every(held)) return;
    onPicked(picked.filter(held), anchor);
  }, [levels, picked]);

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
   * and what the reader means by ⌘ or Ctrl is the machine's word, never a row's. Shift is the one
   * exception, and it is no word of the machine's: on a walk it reaches from the end a range is
   * measured from to where the reader has arrived (`AMB-T-4229`).
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
    /**
     * Stand on a row, and pick out what the walk arrived at.
     *
     * Moving without Shift picks one row: what was picked is put down, and where the reader lands
     * is the end the next range is measured from. With it, the range runs from that end to here —
     * from the row they were standing on where nothing has been picked yet, which is what a reader
     * pressing Shift on a tree nobody has touched means by it.
     */
    const go = (to: Row | undefined, spread = false) => {
      if (to === undefined) return;
      setNamed({ key: to.key });
      if (!spread) { onPicked([to.key], to.key); return; }
      const from = anchor ?? here.key;
      onPicked(between(from, to.key), from);
    };
    switch (e.key) {
      case "ArrowDown": e.preventDefault(); go(rows[at + 1], e.shiftKey); break;
      case "ArrowUp": e.preventDefault(); go(rows[at - 1], e.shiftKey); break;
      // The two ends of the tree are reached the way the steps are: a reader holding Shift is
      // asking for everything between, however far away the end is.
      case "Home": e.preventDefault(); go(rows[0], e.shiftKey); break;
      case "End": e.preventDefault(); go(rows[rows.length - 1], e.shiftKey); break;
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
      // Said on the list, because it is a fact about the tree and not about any one row: a reader
      // being read to is told the rows can be picked out several at a time before they meet one.
      aria-multiselectable
      onKeyDown={onKey}
    >
      {/* What the rows nobody is looking at leave behind: their height, so that the list is as tall
          as the tree is and the scrollbar says how much of it there is. */}
      {from > 0 && <li role="none" aria-hidden="true" style={{ height: from * ROW }} />}
      {drawn.map((line) => {
        // 🚨 **The box is a row of the tree, not a line standing beside the rows.** What a tree may
        // hold is named, and a line drawn as anything else is thrown out of the tree the machine
        // hands a reader being read to — so the box was on the screen and nowhere in what the
        // machine said was there, which is a rename a screen reader cannot find (`AMB-T-4396`). It
        // holds no name of its own to be picked out or stood on: how deep it is is what it says.
        if (line.kind === "make") {
          return (
            <li key="make" role="treeitem" aria-level={line.depth + 1} style={step(line.depth)}>
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
        // would leave a reader wondering which one they were about to keep. It keeps the row's own
        // place in the tree — how deep it is, how many it stands among and which of them it is —
        // because it is still that row, and a line the tree may not hold is a line nobody being
        // read to is told about (the box above).
        if (renaming(naming.edit, root, line.path)) {
          return (
            <li
              key={line.key}
              role="treeitem"
              aria-level={line.depth + 1}
              aria-setsize={line.setsize}
              aria-posinset={line.posinset}
              style={step(line.depth)}
            >
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
            // On every row and not only on the picked ones: what a reader is told about a row they
            // have arrived at is whether it is in the selection, and a row that said nothing would
            // be read as one that cannot be picked at all.
            aria-selected={picked.includes(line.key)}
            tabIndex={line.key === stop ? 0 : -1}
            className={`files__item${lands ? " files__into" : ""}`}
            // A press picks the row out; which keys are down says what else it is. The machine's
            // own key takes one row into the selection or back out of it, Shift reaches from the
            // end the range is measured from to here, and a press with neither is this one row.
            // None of the three reads a file: a reader gathering rows is not asking to be shown
            // each one on the way (`AMB-T-4229`), and what is inside a row is asked for by the
            // second press (`AMB-D-835`).
            //
            // **Which key that is, is the machine's answer and not one key that will do.** Ctrl
            // and a press is how a Mac asks for the menu, so a Mac reading it as "add this row"
            // would answer one press with a menu and a row taken in at once (`../core/platform`).
            // Whether a click arrives beside that menu is the webview's own answer, so the press is
            // let go of here rather than read as the plain one it is not.
            onClick={(e) => {
              const mac = hostOs() === "macos";
              if (mac && e.ctrlKey) return;
              if (e.shiftKey) {
                const from = anchor ?? cursor ?? line.key;
                onPicked(between(from, line.key), from);
                return;
              }
              if (mac ? e.metaKey : e.ctrlKey) {
                onPicked(
                  picked.includes(line.key)
                    ? picked.filter((one) => one !== line.key)
                    : [...picked, line.key],
                  line.key,
                );
                return;
              }
              onPicked([line.key], line.key);
              // A folder answers the first press: what is under it is more of the tree, and
              // showing it takes nothing away from what a reader can still reach. A file waits for
              // the second one — the file lies over the tree while it is being read, so a row
              // opened on the way to the next one is the row that hides it (`AMB-D-835`).
              if (line.isDir) onOpen(line.key);
            }}
            // The second press is what asks for what is inside. Keys held down mean here what they
            // mean on the first press: a reader reaching for a run of rows, or taking one in and
            // out of the selection, is not asking to be shown any of them — and pressing twice to
            // gather two rows is not a request to read either.
            onDoubleClick={(e) => {
              const mac = hostOs() === "macos";
              if (mac && e.ctrlKey) return;
              if (e.shiftKey || (mac ? e.metaKey : e.ctrlKey)) return;
              if (!line.isDir) onRead(line.path);
            }}
            // A row is a thing to open and a thing to carry, and which one a press turns out to be
            // is decided by how far it travels (`./handDrag`). A folder is carried the same way a
            // file is: what is handed over is a path, and the tree under it costs nothing to name.
            //
            // What is taken hold of is what the press is about (`rowsAbout`): the rows picked out
            // where this is one of them, and this row alone where it is not. The ghost that follows
            // the pointer is still this row — the one under the hand is what a person is carrying,
            // however many are coming with it.
            //
            // **The row takes the keyboard here, rather than leaving it to the press's default.**
            // The band and the keyboard are two different things on this tree — one is the rows
            // picked out, the other the row `⌘C` and the arrows are standing on — and the press was
            // only ever moving the first. In the webview it moved neither: the gesture raises its
            // fence against the browser's own text selection the moment the press lands
            // (`./handDrag`), and the default that would have stood the keyboard on the row does
            // not survive it. What a reader saw was the band on the row they pressed and the copy
            // taking the row before it, or nothing at all (`AMB-T-4368`). Stood on explicitly, the
            // way the menu already stands on the row it is about.
            onPointerDown={(e) => {
              e.currentTarget.focus();
              onCarry?.(rowsAbout(picked, line.path).map((one) => fileAt(root, one)), e);
            }}
            // Stood on before the menu opens, because the row a menu is about is the row a reader
            // comes back to when it closes — and a right-click is not a press the browser moves the
            // focus for.
            //
            // A menu opened away from what is picked is a menu about this row: the rows a reader
            // gathered are put down, because the alternative is a menu standing over one row and
            // acting on others (`AMB-T-4230`). Opened on a row that is already in the selection it
            // changes nothing — that is the press a reader makes to act on what they gathered.
            onContextMenu={(e) => {
              e.preventDefault();
              e.stopPropagation();
              e.currentTarget.focus();
              if (!picked.includes(line.key)) onPicked([line.key], line.key);
              onMenu(line.path, line.isDir, e.clientX, e.clientY);
            }}
            // Where the tab stop follows to, however the row was reached — the arrows move the
            // focus and this is what moves the stop after it, so tabbing away and back returns to
            // the row a reader was on rather than to the top of the tree.
            onFocus={(e) => { if (e.target === e.currentTarget) onCursor(line.key); }}
          >
            {line.isDir
              ? (
                <span className={rowClass("files__dir", {
                  ignored: line.ignored,
                  mark,
                  picked: picked.includes(line.key),
                  chosen: chosen === line.key,
                })}
                >
                  <span className="files__twisty">
                    <Icon name={line.unfolded ? "chevronDown" : "chevronRight"} />
                  </span>
                  <span className="files__kind"><Icon name="folder" /></span>
                  <span className="files__name">{line.name}</span>
                </span>
              )
              : (
                <span className={rowClass("files__file", {
                  ignored: line.ignored,
                  mark,
                  picked: picked.includes(line.key),
                  chosen: chosen === line.key,
                })}
                >
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
