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
import { useEffect, useMemo, useRef, useState } from "react";
import type { ReactNode } from "react";
import type {
  FolderAppDto, FolderCarriedDto, FolderChangesDto, FolderEntryDto, FolderFileDto, GitEntryDto,
} from "../bindings/bindings";
import { Markdown } from "../components/Markdown";
import { useBoundFolders } from "../core/boundFolders";
import { watchHostDrop } from "../core/hostDrop";
import { fileUrl } from "../core/fileUrl";
import { errText, formatNumber, t, tf } from "../core/i18n";
import { pushNotice } from "../core/notice";
import { RefNavProvider, useRefNav, type RefNav } from "../core/refNav";
import {
  folderEntries, folderGitStatus, folderImport, folderMake, folderOpenFile, folderOpenFileWith,
  folderOpenWith, folderRead, folderRename, folderRevealFile, folderSave, folderUnwatch,
  folderWatch,
  onFolderChanged,
} from "./folder";
import { FileEditor } from "./FileEditor";
import { MemoPage } from "./MemoPage";
import { fileUnderAny } from "./fileUnder";
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

/** The name being typed in one section, and the end of typing it. */
type Naming = {
  /** The one being typed here, or nothing — a section is handed only its own. */
  edit: Edit | null;
  /** End it. `made` says whether the folder now holds something it did not, which is what tells the
   *  levels under it to look again rather than wait for the host to say the folder moved. */
  end: (made: boolean) => void;
};

/** Whether an edit is the making of a name in this folder of this section. */
function makingIn(edit: Edit | null, root: string, path: string[]): Edit & { kind: "make" } | null {
  return edit?.kind === "make" && edit.root === root && edit.into.join("/") === path.join("/")
    ? edit
    : null;
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
 * What to say about a carry that stopped, or nothing where the whole of it arrived.
 *
 * The count is in the sentence because a carry is not one act: stopping on the second of three
 * leaves one file in the folder, and a line that named only the failure would have the reader
 * looking for the one that did arrive.
 */
function stoppedLine(carried: FolderCarriedDto): string | null {
  const stopped = carried.stopped;
  if (stopped === null) return null;
  const about = { name: stopped.name, why: stopped.why };
  return carried.arrived.length === 0
    ? tf("files.dropStopped", about)
    : tf("files.dropPartly", { ...about, count: formatNumber(carried.arrived.length) });
}

export function FilesPanel({ projectId, onOpenLedger, show, tab, onTab, onClose }: {
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

  if (reading !== null) {
    return (
      <FileReader
        projectId={projectId}
        root={reading.root}
        path={reading.path}
        onBack={() => setReading(null)}
        onOpenLedger={onOpenLedger}
        close={close}
      />
    );
  }

  const top = <div className="files__top">{close}</div>;

  return (
    <div className="files" ref={box}>
      {top}
      {sections.map((one) => (
        <FolderSection
          key={one.path}
          projectId={projectId}
          root={one.path}
          // The only folder there is needs no heading: a name is what tells two of them apart.
          label={sections.length > 1 ? one.label : null}
          bound={one.exists}
          landing={landing}
          edit={edit?.root === one.path ? edit : null}
          onEdit={setEdit}
          onRead={(path) => { setEdit(null); setReading({ root: one.path, path }); }}
          onMenu={(path, dir, x, y) => setMenu({ root: one.path, path, dir, x, y })}
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
 * **The news is a number, not a payload.** What the host says when the folder moves carries no
 * rows (`AMB-D-785`), so what a section does with it is count: `moved` goes up, and everything read
 * off the disk — git's answer here, the names of each open level in `Level` — is asked for again
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
  projectId, root, label, bound, landing, edit, onEdit, onRead, onMenu,
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
  /** The name being typed in this section, or nothing. The panel holds it, and hands each section
   *  only what is its own. */
  edit: Edit | null;
  onEdit: (edit: Edit | null) => void;
  onRead: (path: string[]) => void;
  onMenu: (path: string[], dir: boolean, x: number, y: number) => void;
}) {
  const [changes, setChanges] = useState<FolderChangesDto>(
    { root, capped: false, unwatched: false, gone: false },
  );
  // How many times the host has said this folder moved. Everything read off the disk watches it.
  const [moved, setMoved] = useState(0);
  const [git, setGit] = useState<GitEntryDto[]>([]);
  const [treeOpen, setTreeOpen] = useState(false);
  // Which folders of the tree are unfolded, as their paths joined. Held for the whole section rather
  // than per level, because opening one is also something the section does on a reader's behalf: a
  // name being made in a folder that is folded shut would be typed where nobody could see it.
  const [open, setOpen] = useState<string[]>([]);
  const gone = !bound || changes.gone;

  const naming: Naming = {
    edit,
    // A name written into the folder moves it as surely as an agent does, and the host says so 400ms
    // later (`crate::folder_watch`). A row somebody has just made belongs on the list before that.
    end: (made) => {
      if (made) setMoved((n) => n + 1);
      onEdit(null);
    },
  };

  // The panel says which folder a name is being made in; unfolding the way to it is this section's
  // own answer. The tree starts folded, so a box typed into a folder nobody can see is the ordinary
  // case rather than a corner.
  const making = edit?.kind === "make" ? edit.into.join("/") : null;
  useEffect(() => {
    if (making === null) return;
    setTreeOpen(true);
    if (making !== "") setOpen((was) => (was.includes(making) ? was : [...was, making]));
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
  // colour nobody is looking at is a process started for nothing, and a folder someone leaves the
  // panel folded on is one an agent may be writing in all afternoon.
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
          onClick={() => setTreeOpen((was) => !was)}
          onContextMenu={(e) => {
            e.preventDefault();
            onMenu([], true, e.clientX, e.clientY);
          }}
        >
          {t("files.tree")}
        </button>
        {/* Folded until it is asked for, and each level asked for only when it is opened: a tree is
            not the point of this face, and an unfolded one would read the whole repository to draw
            a panel nobody was looking at. */}
        {treeOpen && (
          <Level
            projectId={projectId}
            root={root}
            path={[]}
            landing={landing}
            marks={marks}
            moved={moved}
            open={open}
            onOpen={(key) => setOpen((was) =>
              was.includes(key) ? was.filter((one) => one !== key) : [...was, key]
            )}
            naming={naming}
            onRead={onRead}
            onMenu={onMenu}
          />
        )}
      </section>
    </div>
  );
}

/**
 * What can be done with a file that is not reading it here: hand it to the machine.
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
function FileMenu({ projectId, root, path, dir, at, naming, onClose }: {
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
}) {
  // The applications to pick from, once they have been asked for and there are any — the second
  // face of this one menu, drawn where the OS has no chooser to draw it for us.
  const [apps, setApps] = useState<FolderAppDto[] | null>(null);
  const box = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    // Anything the person does **outside** the menu closes it: one that outlived the next click
    // would sit over rows it is no longer about. Inside is the opposite — a press on an item is the
    // first half of choosing it, and closing there unmounts the button before the click can land on
    // it, so the item never fires at all.
    const close = (event: Event) => {
      if (event.target instanceof Node && box.current?.contains(event.target)) return;
      onClose();
    };
    document.addEventListener("pointerdown", close);
    document.addEventListener("keydown", close);
    window.addEventListener("blur", close);
    return () => {
      document.removeEventListener("pointerdown", close);
      document.removeEventListener("keydown", close);
      window.removeEventListener("blur", close);
    };
  }, [onClose]);

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
    <div className="files__menu" style={{ left: at.x, top: at.y }} role="menu" ref={box}>
      {apps === null ? (
        <>
          {naming !== undefined && dir && (
            <>
              <button
                className="files__menuitem"
                role="menuitem"
                onClick={() => pick(() => naming.onMake(false))}
              >
                {t("files.newFile")}
              </button>
              <button
                className="files__menuitem"
                role="menuitem"
                onClick={() => pick(() => naming.onMake(true))}
              >
                {t("files.newFolder")}
              </button>
            </>
          )}
          {rename !== null && (
            <button className="files__menuitem" role="menuitem" onClick={() => pick(rename)}>
              {t("files.rename")}
            </button>
          )}
          {!dir && (
            <>
              <button
                className="files__menuitem"
                role="menuitem"
                onClick={() => act(() => folderOpenFile(projectId, root, path))}
              >
                {t("files.openWith")}
              </button>
              <button className="files__menuitem" role="menuitem" onClick={choose}>
                {t("files.chooseApp")}
              </button>
              <button
                className="files__menuitem"
                role="menuitem"
                onClick={() => act(() => folderRevealFile(projectId, root, path))}
              >
                {t("files.reveal")}
              </button>
            </>
          )}
        </>
      ) : (
        apps.map((app) => (
          <button
            key={app.path}
            className="files__menuitem"
            role="menuitem"
            onClick={() => act(() => folderOpenFileWith(projectId, root, path, app.path))}
          >
            {/* The one the file would have opened with anyway is said to be that, not just put
                first: a list whose order carries the meaning loses it the moment somebody reads
                from the middle. */}
            {app.usual ? tf("files.appUsual", { name: app.name }) : app.name}
          </button>
        ))
      )}
    </div>
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
function rowClass(base: string, ignored: boolean, mark: GitMark | null): string {
  let all = base;
  if (ignored) all += ` ${base}--ignored`;
  if (mark !== null) all += ` ${base}--git ${base}--git-${mark}`;
  return all;
}

/** One folder's worth of names, and whatever of it has been opened. */
function Level({
  projectId, root, path, landing, marks, moved, open, onOpen, naming, onRead, onMenu,
}: {
  projectId: number;
  root: string;
  path: string[];
  /**
   * Where a file being dragged in would land — passed down whole rather than resolved per level,
   * because a drag hangs over one folder on the whole panel and every level has to be able to stop
   * drawing the highlight it was drawing a moment ago.
   */
  landing: Landing | null;
  /** What git says about a row, asked by its segments from the bound folder (`./gitMark`). */
  marks: (path: string[]) => GitMark | null;
  /** How many times the folder has moved. The names are read again on each — a file the agent just
   *  wrote is a row that has to appear without anybody folding the tree and opening it again. */
  moved: number;
  /** Which folders of the whole section are unfolded, as their paths joined. The section holds it
   *  (`FolderSection`) because opening one is also something it does on a reader's behalf. */
  open: string[];
  onOpen: (key: string) => void;
  /** The name being typed anywhere in this section, passed down for the same reason `landing` is:
   *  every level has to be able to stop drawing the box it was drawing a moment ago. */
  naming: Naming;
  onRead: (path: string[]) => void;
  onMenu: (path: string[], dir: boolean, x: number, y: number) => void;
}) {
  const [names, setNames] = useState<FolderEntryDto[]>([]);
  const making = makingIn(naming.edit, root, path);

  useEffect(() => {
    let alive = true;
    void folderEntries(projectId, root, path)
      .then((rows) => { if (alive) setNames(rows); })
      .catch(() => { if (alive) setNames([]); });
    return () => { alive = false; };
    // `path` is rebuilt by the parent on every render, so the array itself is not what to watch.
  }, [projectId, root, path.join("/"), moved]);

  return (
    <ul className="files__list files__list--tree">
      {/* The name being made, at the top of the folder it is being made in. Above the names rather
          than among them: where it would sort is decided by what is typed, and a box that moved as
          the letters arrived would be a box nobody could read what they had written in. */}
      {making !== null && (
        <li>
          <NameBox
            initial=""
            onName={(name) => folderMake(projectId, root, [...path, name], making.dir)}
            onEnd={naming.end}
          />
        </li>
      )}
      {names.map((one) => {
        // A folder answers for a drop, and what it answers for is everything drawn under it — which
        // is the row itself and, once it is open, the level inside it. That is why the mark sits on
        // the item and not on the button: a file row inside a folder resolves upwards to that
        // folder, so dropping on a name means the same as dropping in the space beside it.
        const here = [...path, one.name];
        const key = here.join("/");
        const into = one.isDir ? key : undefined;
        const mark = marks(here);
        // The row is the box while its name is being written over. Drawn in place of the row rather
        // than beside it: what is being changed is this name, and two of them on the screen at once
        // would leave a reader wondering which one they were about to keep.
        if (renaming(naming.edit, root, here)) {
          return (
            <li key={one.name}>
              <NameBox
                initial={one.name}
                onName={(name) => folderRename(projectId, root, here, name)}
                onEnd={naming.end}
              />
            </li>
          );
        }
        return (
          <li
            key={one.name}
            data-root={root}
            data-into={into}
            className={landed(landing, root, into) ? "files__into" : undefined}
          >
            {one.isDir
              ? (
                <>
                  <button
                    className={rowClass("files__dir", one.ignored, mark)}
                    aria-expanded={open.includes(key)}
                    onClick={() => onOpen(key)}
                    onContextMenu={(e) => {
                      e.preventDefault();
                      onMenu(here, true, e.clientX, e.clientY);
                    }}
                  >
                    {one.name}
                  </button>
                  {open.includes(key) && (
                    <Level
                      projectId={projectId}
                      root={root}
                      path={here}
                      landing={landing}
                      marks={marks}
                      moved={moved}
                      open={open}
                      onOpen={onOpen}
                      naming={naming}
                      onRead={onRead}
                      onMenu={onMenu}
                    />
                  )}
                </>
              )
              : (
                <button
                  className={rowClass("files__file", one.ignored, mark)}
                  onClick={() => onRead(here)}
                  onContextMenu={(e) => {
                    e.preventDefault();
                    onMenu(here, false, e.clientX, e.clientY);
                  }}
                >
                  <span className="files__name">{one.name}</span>
                </button>
              )}
          </li>
        );
      })}
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

  // The box is the reader's the moment it is drawn — it took the place of a row they right-clicked,
  // and a box they had to click into first would be one they typed past.
  useEffect(() => { box.current?.focus(); box.current?.select(); }, []);

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

/** One file, as far as a panel can show it. */
function FileReader({ projectId, root, path, onBack, onOpenLedger, close }: {
  projectId: number;
  root: string;
  path: string[];
  onBack: () => void;
  onOpenLedger?: () => void;
  /** The panel's own way out, drawn on this row: reading a file is not a state a reader should have
   *  to leave before they can close the panel (`./FilesPanel`). */
  close: ReactNode;
}) {
  const [file, setFile] = useState<FolderFileDto | null>(null);
  const [failed, setFailed] = useState(false);
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
  // Where a picture too large to draw was handed on to the machine from. The same menu the list
  // rows open, opened here because this is the one state a reader reaches it from with no row under
  // the pointer.
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const name = path[path.length - 1];

  useEffect(() => {
    let alive = true;
    setFile(null);
    setFailed(false);
    setEdited(false);
    setRefused(null);
    setNewline(null);
    void folderRead(projectId, root, path)
      .then((one) => {
        if (!alive) return;
        setFile(one);
        // Both kinds in one file is the only answer this side cannot act on by itself.
        setNewline(one.lineEnding === "mixed" ? null : one.lineEnding);
      })
      .catch(() => { if (alive) setFailed(true); });
    return () => { alive = false; };
  }, [projectId, root, path.join("/")]);

  // Whether this file is one the panel can write back at all. The host says so before a reader has
  // typed a character: a file cut at the read cap, or one whose bytes and text do not round-trip,
  // is drawn read-only from the start (`AMB-D-773`). Markdown is drawn rather than edited, so there
  // is nothing to save there either (`AMB-T-3807` is where that changes).
  const savable = file?.text !== undefined
    && file.encoding !== undefined
    && !file.truncated
    && file.clean
    && !MARKDOWN.some((ext) => name.toLowerCase().endsWith(ext));

  const save = async () => {
    const read = typed.current;
    if (!savable || keeping || file?.encoding === undefined || read === null || newline === null) return;
    setKeeping(true);
    setRefused(null);
    try {
      await folderSave(projectId, root, path, read(), file.encoding, file.bom, newline);
      setEdited(false);
      // What is on the disk now has one kind of newline, so the question is not asked again. The
      // text is left where it is: replacing it would be handing the editor its own document back
      // and moving the caret to the top for the trouble.
      setFile({ ...file, lineEnding: newline });
    } catch (e) {
      setRefused(errText(e));
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

  return (
    <div className="files files--reading">
      <div className="files__bar">
        <button className="files__back" onClick={onBack}>{t("files.back")}</button>
        <span className="files__name" title={path.join("/")}>{name}</span>
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
        {close}
      </div>
      <div className="files__body">
        {failed && <p className="files__none">{t("files.unreadable")}</p>}
        {/* The picture is fetched rather than carried: `folderRead` says only that there is one
            and what type it is, and the door that hands out a file by its path is addressed with
            the same project, folder and path this reader was opened on (`AMB-D-783`). It draws
            top to bottom as it arrives, where a `data:` URL drew all at once or not at all. */}
        {file?.image !== undefined && (
          <img
            className="files__image"
            alt={name}
            src={fileUrl(projectId, root, path, file.image.mime)}
          />
        )}
        {file?.text !== undefined && (
          MARKDOWN.some((ext) => name.toLowerCase().endsWith(ext))
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
        />
      )}
    </div>
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
