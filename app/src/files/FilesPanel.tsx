// The file face: the right side of the terminal face, where what the agent in the pane is doing to
// the folder can be seen without leaving the window (`AMB-T-3602`).
//
// **Both rows belong to the project.** What changed lately and the folder itself are rooted at a
// folder the project is bound to, so switching panes does not move them — what changed in the
// repository is the same question whichever terminal is in front of it.
//
// **What changed lately is watched, not asked for.** The host lays a watch over the folder and
// says what is in it as it moves (`crate::folder_watch`), so the row is right while a person is
// looking at it rather than as of whenever this side last thought to ask. What it cannot watch —
// a folder too large to walk, a watch the kernel refused — is drawn as the row saying so, because
// an unwatched half looks exactly like a half where nothing happened (`AMB-T-3604`).
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
import type { ReactNode } from "react";
import type {
  FolderAppDto, FolderChangesDto, FolderEntryDto, FolderFileDto,
} from "../bindings/bindings";
import { Markdown } from "../components/Markdown";
import { useBoundFolders } from "../core/boundFolders";
import { watchHostDrop } from "../core/hostDrop";
import { formatNumber, t, tf, whenLabel } from "../core/i18n";
import { RefNavProvider, useRefNav, type RefNav } from "../core/refNav";
import {
  folderEntries, folderOpenFile, folderOpenFileWith, folderOpenWith, folderRead, folderRevealFile,
  folderUnwatch, folderWatch, onFolderChanged,
} from "./folder";
import { MemoPage } from "./MemoPage";
import { fileUnder } from "./fileUnder";
import { Icon } from "../components/Icon";

/** The names a file's text is drawn as Markdown under. The one thing here the name decides. */
const MARKDOWN = [".md", ".markdown"];

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
  const root = folders.live[0]?.path ?? null;
  const [changes, setChanges] = useState<FolderChangesDto>(
    { root: "", changed: [], partial: false, gone: false },
  );
  const [treeOpen, setTreeOpen] = useState(false);
  const [reading, setReading] = useState<string[] | null>(null);
  // The file a right-click was on, and where the pointer was. One menu for the face rather than one
  // per row: only one can be open, and a row that held its own would keep it after the list moved
  // under it (`AMB-T-3605`).
  const [menu, setMenu] = useState<{ path: string[]; x: number; y: number } | null>(null);
  // The folder a file being dragged in would land in, spelled as its segments from the root ("" is
  // the root itself), or null while nothing is over the panel.
  const [landing, setLanding] = useState<string | null>(null);
  const box = useRef<HTMLDivElement | null>(null);
  // A path clicked in a pane. It opens only where it lands inside the folder this face is rooted at
  // — the same fence the host applies. One that lands outside opens nothing: the pane keeps the
  // characters it drew, and no reader is shown a file from somewhere this face cannot answer for
  // (`AMB-D-747`).
  useEffect(() => {
    if (show === undefined || show === null || root === null) return;
    const path = fileUnder(root, show.cwd, show.target);
    // A file asked for is a file to be looked at: the panel comes back off the page to show it.
    if (path) {
      setReading(path);
      onTab("files");
    }
    // `nth` is what makes the same file asked for twice two answers.
  }, [show?.nth, root]);

  useEffect(() => {
    if (projectId === null || root === null) return;
    let alive = true;
    // Subscribed before the watch is asked for: the first thing the folder does could happen while
    // the host is still walking it, and a listener set up afterwards would miss exactly that.
    // Every watched folder is told about through the one listener, and this face is rooted at one of
    // them — so an answer about another folder is not this row's news.
    const listening = onFolderChanged((fresh) => {
      if (alive && fresh.root === root) setChanges(fresh);
    });
    void folderWatch(projectId, root)
      .then((now) => { if (alive) setChanges(now); })
      .catch(() => {
        if (alive) setChanges({ root, changed: [], partial: false, gone: false });
      });
    return () => {
      alive = false;
      void listening.then((stop) => stop());
      void folderUnwatch(root);
    };
  }, [projectId, root]);

  // Files dragged in from the desktop. The panel hears about them from the host rather than from
  // the DOM, so the highlight under the pointer — and the scroll when the pointer hangs at an edge —
  // are this side's to drive (`../core/hostDrop`).
  //
  // **Where a drop lands is worked out and goes no further.** Bringing the file in wants a door that
  // writes into the folder, and the file face has none yet (`AMB-T-3791`); until it does, what the
  // highlight says is which folder the paths would be handed to, which is the half of this that has
  // to be right on all three operating systems.
  useEffect(() => {
    if (projectId === null || root === null || tab !== "files") return;
    let alive = true;
    let stop: (() => void) | null = null;
    void watchHostDrop({
      select: "[data-into]",
      scroller: () => box.current,
      over: ({ el }) => { if (alive) setLanding(el?.getAttribute("data-into") ?? null); },
      leave: () => { if (alive) setLanding(null); },
      drop: () => { if (alive) setLanding(null); },
    }).then((off) => { if (alive) stop = off; else off(); });
    return () => {
      alive = false;
      stop?.();
      setLanding(null);
    };
  }, [projectId, root, tab]);

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

  if (projectId === null || root === null) {
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
        root={root}
        path={reading}
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
      <section className="files__row">
        <h3 className="files__head">{t("files.changed")}</h3>
        {changes.changed.length === 0
          ? <p className="files__none">{t("files.nothingChanged")}</p>
          : (
            <ul className="files__list">
              {changes.changed.map((one) => (
                <li key={one.path.join("/")}>
                  <button
                    className="files__file"
                    onClick={() => setReading(one.path)}
                    onContextMenu={(e) => {
                      e.preventDefault();
                      setMenu({ path: one.path, x: e.clientX, y: e.clientY });
                    }}
                  >
                    <span className="files__name">{one.path[one.path.length - 1]}</span>
                    <span className="files__when">{whenLabel(one.modified)}</span>
                  </button>
                  <span className="files__where">{one.path.slice(0, -1).join("/")}</span>
                </li>
              ))}
            </ul>
          )}
        {/* Said out loud rather than left to be assumed: a folder only half watched goes on looking
            like one where nothing is happening. */}
        {changes.partial && <p className="files__none">{t("files.partial")}</p>}
      </section>
      {/* The root is a folder like any other in the tree, and the one a drop that fell on no row
          lands in. It is marked on the section rather than on the heading so that the whole of the
          tree — the gaps between its rows included — answers for it. */}
      <section
        className={`files__row${landing === "" ? " files__row--into" : ""}`}
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
            onRead={setReading}
            onMenu={(path, x, y) => setMenu({ path, x, y })}
          />
        )}
      </section>
      {menu !== null && (
        <FileMenu
          projectId={projectId}
          root={root}
          path={menu.path}
          at={{ x: menu.x, y: menu.y }}
          onClose={() => setMenu(null)}
        />
      )}
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
function FileMenu({ projectId, root, path, at, onClose }: {
  projectId: number;
  root: string;
  path: string[];
  at: { x: number; y: number };
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
 * A row's classes, with the faint one added for a name the repository ignores.
 *
 * The row is drawn either way: `.gitignore` says what git does not record, not what a reader may
 * not look at, and the files a person goes looking for after an agent has been at work — `.env`,
 * `.amenbo` — are ignored in most repositories (`AMB-D-786`). What is left out of the tree is the
 * floor the host prunes, and that never reaches here.
 */
function faint(base: string, ignored: boolean): string {
  return ignored ? `${base} ${base}--ignored` : base;
}

/** One folder's worth of names, and whatever of it has been opened. */
function Level({ projectId, root, path, landing, onRead, onMenu }: {
  projectId: number;
  root: string;
  path: string[];
  /**
   * The folder a file being dragged in would land in, as its segments joined — passed down whole
   * rather than resolved per level, because a drag hangs over one folder in the whole tree and every
   * level has to be able to stop drawing the highlight it was drawing a moment ago.
   */
  landing: string | null;
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
  }, [projectId, root, path.join("/")]);

  return (
    <ul className="files__list files__list--tree">
      {names.map((one) => {
        // A folder answers for a drop, and what it answers for is everything drawn under it — which
        // is the row itself and, once it is open, the level inside it. That is why the mark sits on
        // the item and not on the button: a file row inside a folder resolves upwards to that
        // folder, so dropping on a name means the same as dropping in the space beside it.
        const into = one.isDir ? [...path, one.name].join("/") : undefined;
        return (
          <li
            key={one.name}
            data-into={into}
            className={into !== undefined && into === landing ? "files__into" : undefined}
          >
            {one.isDir
              ? (
                <>
                  <button
                    className={faint("files__dir", one.ignored)}
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
                      onRead={onRead}
                      onMenu={onMenu}
                    />
                  )}
                </>
              )
              : (
                <button
                  className={faint("files__file", one.ignored)}
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
    <div className="files files--reading">
      <div className="files__bar">
        <button className="files__back" onClick={onBack}>{t("files.back")}</button>
        <span className="files__name" title={path.join("/")}>{name}</span>
        {close}
      </div>
      <div className="files__body">
        {failed && <p className="files__none">{t("files.unreadable")}</p>}
        {file?.image !== undefined && (
          <img className="files__image" alt={name} src={`data:${file.image.mime};base64,${file.image.base64}`} />
        )}
        {file?.text !== undefined && (
          MARKDOWN.some((ext) => name.toLowerCase().endsWith(ext))
            ? <RefNavProvider value={nav}><Markdown>{file.text}</Markdown></RefNavProvider>
            : <pre className="files__text">{file.text}</pre>
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
