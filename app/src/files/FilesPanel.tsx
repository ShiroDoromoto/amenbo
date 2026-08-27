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
// **A file dragged in from the desktop lands on the application, not on this page** (`AMB-D-775`).
// So the panel is not told which row is under the pointer — it is told a point, and the folder that
// point falls in is worked out here (`../core/hostDrop`). Every folder in the tree is one, and so is
// the tree itself; a file row belongs to the folder holding it, which is what makes dropping on the
// name of a file mean the same as dropping just beside it.
import { useEffect, useMemo, useRef, useState } from "react";
import type { KeyboardEvent, ReactNode } from "react";
import type {
  FolderAppDto, FolderChangesDto, FolderEntryDto, FolderFileDto, GitEntryDto,
} from "../bindings/bindings";
import { Markdown } from "../components/Markdown";
import { useBoundFolders } from "../core/boundFolders";
import { watchHostDrop } from "../core/hostDrop";
import { fileUrl } from "../core/fileUrl";
import { errText, formatNumber, t, tf } from "../core/i18n";
import { RefNavProvider, useRefNav, type RefNav } from "../core/refNav";
import {
  folderEntries, folderGitStatus, folderOpenFile, folderOpenFileWith, folderOpenWith, folderRead,
  folderRevealFile, folderTrash, folderUnwatch, folderUntrash, folderWatch, onFolderChanged,
} from "./folder";
import { asksBeforeTrash } from "./askBeforeTrash";
import { TrashAsk } from "./TrashAsk";
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
    { root: string; path: string[]; x: number; y: number } | null
  >(null);
  // Where a file being dragged in would land: which bound folder, and which folder inside it ("" is
  // the bound folder itself). Null while nothing is over the panel. The folder is half of it because
  // every section has a row for its own root, and the same path inside two of them is two places.
  const [landing, setLanding] = useState<Landing | null>(null);
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

  // Files dragged in from the desktop. The panel hears about them from the host rather than from
  // the DOM, so the highlight under the pointer — and the scroll when the pointer hangs at an edge —
  // are this side's to drive (`../core/hostDrop`).
  //
  // **Where a drop lands is worked out and goes no further.** Bringing the file in wants a door that
  // writes into the folder, and the file face has none yet (`AMB-T-3791`); until it does, what the
  // highlight says is which folder the paths would be handed to, which is the half of this that has
  // to be right on all three operating systems.
  useEffect(() => {
    if (projectId === null || sections.length === 0 || tab !== "files") return;
    let alive = true;
    let stop: (() => void) | null = null;
    void watchHostDrop({
      select: "[data-into]",
      scroller: () => box.current,
      over: ({ el }) => { if (alive) setLanding(landingOf(el)); },
      leave: () => { if (alive) setLanding(null); },
      drop: () => { if (alive) setLanding(null); },
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
        setStopped(done.stopped?.why ?? null);
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
        setStopped(done.stopped?.why ?? null);
      })
      .catch((e: unknown) => setStopped(errText(e)));
  };

  const onKey = (e: KeyboardEvent) => {
    if (!(e.metaKey || e.ctrlKey) || e.shiftKey || e.altKey) return;
    if (e.key.toLowerCase() !== "z") return;
    e.preventDefault();
    undo();
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

  if (reading !== null) {
    return (
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
      />
    );
  }

  const top = <div className="files__top">{close}</div>;

  return (
    // Focusable so the panel can hold the key it hears, and taken off the tab order so that being
    // able to hold it costs nobody a stop on the way past (`AMB-D-780`).
    <div className="files" ref={box} tabIndex={-1} onKeyDown={onKey}>
      {top}
      {aside}
      {sections.map((one) => (
        <FolderSection
          key={one.path}
          projectId={projectId}
          root={one.path}
          // The only folder there is needs no heading: a name is what tells two of them apart.
          label={sections.length > 1 ? one.label : null}
          bound={one.exists}
          landing={landing}
          onRead={(path) => setReading({ root: one.path, path })}
          onMenu={(path, x, y) => setMenu({ root: one.path, path, x, y })}
        />
      ))}
      {menu !== null && (
        <FileMenu
          projectId={projectId}
          root={menu.root}
          path={menu.path}
          at={{ x: menu.x, y: menu.y }}
          onClose={() => setMenu(null)}
          onTrash={() => askTrash(menu.root, menu.path)}
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
function FolderSection({ projectId, root, label, bound, landing, onRead, onMenu }: {
  projectId: number;
  root: string;
  /** The heading, or nothing where this is the only folder. */
  label: string | null;
  /** Whether the store's own read found the folder. The watch answers the same question later. */
  bound: boolean;
  /** Where a dragged file would land, anywhere on the panel — a section draws the highlight only
   *  where the landing is one of its own (`landed`). */
  landing: Landing | null;
  onRead: (path: string[]) => void;
  onMenu: (path: string[], x: number, y: number) => void;
}) {
  const [changes, setChanges] = useState<FolderChangesDto>(
    { root, capped: false, unwatched: false, gone: false },
  );
  // How many times the host has said this folder moved. Everything read off the disk watches it.
  const [moved, setMoved] = useState(0);
  const [git, setGit] = useState<GitEntryDto[]>([]);
  const [treeOpen, setTreeOpen] = useState(false);
  const gone = !bound || changes.gone;

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
        <button
          className="files__head files__head--button"
          aria-expanded={treeOpen}
          onClick={() => setTreeOpen((open) => !open)}
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
 */
function FileMenu({ projectId, root, path, at, onClose, onTrash }: {
  projectId: number;
  root: string;
  path: string[];
  at: { x: number; y: number };
  onClose: () => void;
  /** Send this row to the machine's bin — asked about first, unless the reader turned that off. */
  onTrash: () => void;
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
          {/* The one item here that changes the folder rather than handing it to something else, so
              it is set apart from the three that do not: a press meant for the row above it must not
              be able to land on this one by half a pixel. */}
          <button
            className="files__menuitem files__menuitem--apart"
            role="menuitem"
            onClick={() => { onClose(); onTrash(); }}
          >
            {t("files.trash")}
          </button>
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
function Level({ projectId, root, path, landing, marks, moved, onRead, onMenu }: {
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
   *  wrote is a row that has to appear without anybody folding the tree and opening it again, and a
   *  row that just went to the bin is one that has to stop being drawn. */
  moved: number;
  onRead: (path: string[]) => void;
  onMenu: (path: string[], x: number, y: number) => void;
}) {
  const [names, setNames] = useState<FolderEntryDto[]>([]);
  const [open, setOpen] = useState<string[]>([]);

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
      {names.map((one) => {
        // A folder answers for a drop, and what it answers for is everything drawn under it — which
        // is the row itself and, once it is open, the level inside it. That is why the mark sits on
        // the item and not on the button: a file row inside a folder resolves upwards to that
        // folder, so dropping on a name means the same as dropping in the space beside it.
        const into = one.isDir ? [...path, one.name].join("/") : undefined;
        const mark = marks([...path, one.name]);
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
                    aria-expanded={open.includes(one.name)}
                    onClick={() => setOpen((was) =>
                      was.includes(one.name) ? was.filter((n) => n !== one.name) : [...was, one.name]
                    )}
                  >
                    {one.name}
                  </button>
                  {open.includes(one.name) && (
                    <Level
                      projectId={projectId}
                      root={root}
                      path={[...path, one.name]}
                      landing={landing}
                      marks={marks}
                      moved={moved}
                      onRead={onRead}
                      onMenu={onMenu}
                    />
                  )}
                </>
              )
              : (
                <button
                  className={rowClass("files__file", one.ignored, mark)}
                  onClick={() => onRead([...path, one.name])}
                  onContextMenu={(e) => {
                    e.preventDefault();
                    onMenu([...path, one.name], e.clientX, e.clientY);
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

/** One file, as far as a panel can show it. */
function FileReader({ projectId, root, path, onBack, onOpenLedger, onTrash, onKey, close, aside }: {
  projectId: number;
  root: string;
  path: string[];
  onBack: () => void;
  onOpenLedger?: () => void;
  /** Send the file being read to the machine's bin. The panel takes it off the screen from there. */
  onTrash: () => void;
  /** Undo, heard here for the same reason it is heard on the list: a file can go to the bin from
   *  this state too (`./FilesPanel`). */
  onKey: (e: KeyboardEvent) => void;
  /** The panel's own way out, drawn on this row: reading a file is not a state a reader should have
   *  to leave before they can close the panel (`./FilesPanel`). */
  close: ReactNode;
  /** The question about the bin and the last refusal, both of which outlive this state. */
  aside: ReactNode;
}) {
  const [file, setFile] = useState<FolderFileDto | null>(null);
  const [failed, setFailed] = useState(false);
  // Where a picture too large to draw was handed on to the machine from. The same menu the list
  // rows open, opened here because this is the one state a reader reaches it from with no row under
  // the pointer.
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const name = path[path.length - 1];

  useEffect(() => {
    let alive = true;
    setFile(null);
    setFailed(false);
    void folderRead(projectId, root, path)
      .then((one) => { if (alive) setFile(one); })
      .catch(() => { if (alive) setFailed(true); });
    return () => { alive = false; };
  }, [projectId, root, path.join("/")]);

  // A reference in a file is a live link or it is nothing at all (`AMB-D-747`), and following one
  // leaves this face: what a record opens on is the ledger.
  const nav = useLedgerNav(onOpenLedger);

  return (
    <div className="files files--reading" tabIndex={-1} onKeyDown={onKey}>
      <div className="files__bar">
        <button className="files__back" onClick={onBack}>{t("files.back")}</button>
        <span className="files__name" title={path.join("/")}>{name}</span>
        <button className="files__trash" title={t("files.trash")} onClick={onTrash}>
          <Icon name="trash" />
        </button>
        {close}
      </div>
      {aside}
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
            : <FileEditor text={file.text} editable={!file.truncated && file.clean} name={name} />
        )}
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
          at={menu}
          onClose={() => setMenu(null)}
          onTrash={onTrash}
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
