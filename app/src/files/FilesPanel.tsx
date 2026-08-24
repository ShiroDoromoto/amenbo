// The file face: the right side of the terminal face, where what the agent in the pane is doing to
// the folder can be seen without leaving the window (`AMB-T-3602`).
//
// **Two of the rows belong to the project and one to the pane.** What changed lately and the folder
// itself are rooted at a folder the project is bound to, so switching panes does not move them — what
// changed in the repository is the same question whichever terminal is in front of it. The top row is
// the other way round: it is what the focused pane's agent pointed at, and it follows the focus
// (`AMB-T-3603`). Mixing the two would leave a reader unable to say what happened where.
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
import { useEffect, useMemo, useRef, useState } from "react";
import type { ReactNode } from "react";
import type { FolderAppDto, FolderChangesDto } from "../bindings/bindings";
import { Markdown } from "../components/Markdown";
import { useBoundFolders } from "../core/boundFolders";
import { t, tf, whenLabel } from "../core/i18n";
import { openExternalUrl } from "../core/mutations";
import { resolveRef } from "../core/reads";
import { RefNavProvider, useRefNav, type RefNav } from "../core/refNav";
import {
  folderEntries, folderOpenFile, folderOpenFileWith, folderOpenWith, folderRead, folderRevealFile,
  folderUnwatch, folderWatch, onFolderChanged,
} from "./folder";
import { MemoPage } from "./MemoPage";
import { fileUnder, isRef, isUrl, unread, type Pointed } from "./pointed";

/** The names a file's text is drawn as Markdown under. The one thing here the name decides. */
const MARKDOWN = [".md", ".markdown"];

export function FilesPanel({ projectId, onOpenLedger, pointed, show, tab, onTab, onClose }: {
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
  /** What the focused pane's agent has pointed at, and whose pane it is (`AMB-T-3603`). */
  pointed?: {
    /** The rows, newest first. */
    points: readonly Pointed[];
    /** What that pane is called, where anybody has named it. */
    name: string | null;
    /** Whether the session that said these has ended. */
    ended: boolean;
    /** Somebody opened one of them. */
    onRead: (at: string) => void;
  };
  /**
   * Which of the two halves is up, and how to ask for one.
   *
   * They are the caller's rather than this panel's because the terminal face's top row switches
   * between them too, and it is the same control as the tabs here: pressing the half already up is
   * what closes the panel (`../shell/TerminalFace`). A file clicked in a pane asks for the files
   * half the same way, so a panel that had been closed comes back to show it.
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
  const [changes, setChanges] = useState<FolderChangesDto>({ changed: [], partial: false });
  const [treeOpen, setTreeOpen] = useState(false);
  const [reading, setReading] = useState<string[] | null>(null);
  // The file a right-click was on, and where the pointer was. One menu for the face rather than one
  // per row: only one can be open, and a row that held its own would keep it after the list moved
  // under it (`AMB-T-3605`).
  const [menu, setMenu] = useState<{ path: string[]; x: number; y: number } | null>(null);
  // A path clicked in a pane. It opens only where it lands inside the folder this face is rooted at
  // — the same rule a pointed-at file is held to, and the same fence the host applies. One that
  // lands outside opens nothing: the pane keeps the characters it drew, and no reader is shown a
  // file from somewhere this face cannot answer for (`AMB-D-747`).
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
    const listening = onFolderChanged((fresh) => { if (alive) setChanges(fresh); });
    void folderWatch(projectId, root)
      .then((now) => { if (alive) setChanges(now); })
      .catch(() => { if (alive) setChanges({ changed: [], partial: false }); });
    return () => {
      alive = false;
      void listening.then((stop) => stop());
      void folderUnwatch();
    };
  }, [projectId, root]);

  // The way to put the panel away. It is drawn on whatever row the panel has, in every state it can
  // be in: a panel that could only be closed while it happened to be showing its tabs would be one
  // a reader has to find their way back out of.
  const close = (
    <button className="files__close" title={t("pane.close")} onClick={onClose}>×</button>
  );

  if (projectId === null || root === null) {
    // A read that has not come back draws nothing at all: a flash of "no folder" on a project that
    // has one reads as a broken binding (`core/boundFolders`).
    return folders.answered
      ? (
        <div className="files files--empty">
          <div className="files__tabs">{close}</div>
          <p className="files__none">{t("files.noFolder")}</p>
        </div>
      )
      : <div className="files"><div className="files__tabs">{close}</div></div>;
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

  const tabs = (
    <div className="files__tabs" role="tablist">
      {(["files", "memo"] as const).map((one) => (
        <button
          key={one}
          className={`files__tab${tab === one ? " files__tab--on" : ""}`}
          role="tab"
          aria-selected={tab === one}
          onClick={() => onTab(one)}
        >
          {t(one === "files" ? "files.tab" : "files.memo")}
        </button>
      ))}
      {close}
    </div>
  );

  if (tab === "memo") {
    return (
      <div className="files">
        {tabs}
        <MemoPage projectId={projectId} />
      </div>
    );
  }

  return (
    <div className="files">
      {tabs}
      {pointed !== undefined && (
        <PointedRow
          root={root}
          pointed={pointed}
          onRead={(at) => pointed.onRead(at)}
          onOpenFile={setReading}
          onOpenLedger={onOpenLedger}
        />
      )}
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
      <section className="files__row">
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
 * What the focused pane's agent pointed at.
 *
 * Nothing is announced while the agent is working — the count beside the heading is the whole of
 * what this row says during a run, because an agent at work is not the moment to interrupt. When the
 * session ends, anything nobody opened is said once, and nothing is held up over it: a reader who
 * ignores the line has still finished (`AMB-T-3603`).
 */
function PointedRow({ root, pointed, onRead, onOpenFile, onOpenLedger }: {
  root: string | null;
  pointed: { points: readonly Pointed[]; name: string | null; ended: boolean };
  onRead: (at: string) => void;
  onOpenFile: (path: string[]) => void;
  onOpenLedger?: () => void;
}) {
  const nav = useLedgerNav(onOpenLedger);
  const left = unread(pointed.points);

  const open = (one: Pointed) => {
    onRead(one.at);
    if (isRef(one.target)) {
      void resolveRef(one.target).then((target) => {
        if (!target) return;
        onOpenLedger?.();
        if (target.kind === "task") nav.selectTask?.(target.id);
        else nav.selectDecision?.(target.id);
      });
      return;
    }
    if (isUrl(one.target)) {
      void openExternalUrl(one.target);
      return;
    }
    const path = root === null ? null : fileUnder(root, one.cwd, one.target);
    if (path) onOpenFile(path);
  };

  return (
    <section className="files__row">
      <h3 className="files__head">
        {t("files.pointed")}
        {/* The count, and nothing else, while the agent is at work. */}
        {left > 0 && <span className="files__when">{left}</span>}
      </h3>
      {pointed.name !== null && <span className="files__where">{pointed.name}</span>}
      {pointed.points.length === 0
        ? <p className="files__none">{t("files.nothingPointed")}</p>
        : (
          <ul className="files__list">
            {pointed.points.map((one) => {
              // A row that opens nothing is not drawn as one that does: a path outside the folder
              // the face is rooted at has nowhere here to go (`AMB-D-747`).
              const reachable = isRef(one.target) || isUrl(one.target)
                || (root !== null && fileUnder(root, one.cwd, one.target) !== null);
              return (
                <li key={one.at}>
                  {reachable
                    ? (
                      <button
                        className={`files__file${one.read ? " files__file--read" : ""}`}
                        onClick={() => open(one)}
                      >
                        <span className="files__name">{one.target}</span>
                      </button>
                    )
                    : <span className="files__name files__none">{one.target}</span>}
                  {one.why !== "" && <span className="files__where">{one.why}</span>}
                </li>
              );
            })}
          </ul>
        )}
      {/* Said once, at the end, and only about what is still unopened. */}
      {pointed.ended && left > 0 && (
        <p className="files__none">{tf("files.unopened", { n: left })}</p>
      )}
    </section>
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

/** One folder's worth of names, and whatever of it has been opened. */
function Level({ projectId, root, path, onRead, onMenu }: {
  projectId: number;
  root: string;
  path: string[];
  onRead: (path: string[]) => void;
  onMenu: (path: string[], x: number, y: number) => void;
}) {
  const [names, setNames] = useState<{ name: string; isDir: boolean }[]>([]);
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
      {names.map((one) => (
        <li key={one.name}>
          {one.isDir
            ? (
              <>
                <button
                  className="files__dir"
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
                    onRead={onRead}
                    onMenu={onMenu}
                  />
                )}
              </>
            )
            : (
              <button
                className="files__file"
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
      ))}
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
  const [file, setFile] = useState<{ text?: string; truncated: boolean; image?: { mime: string; base64: string } } | null>(null);
  const [failed, setFailed] = useState(false);
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
  // leaves this face for the same reason a pointed-at record does.
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
        {file !== null && file.text === undefined && file.image === undefined && (
          <p className="files__none">{t("files.notText")}</p>
        )}
        {file?.truncated === true && <p className="files__none">{t("files.cut")}</p>}
      </div>
    </div>
  );
}
