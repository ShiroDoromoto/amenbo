// The rail's other half: what git says about the folder the window is on (`AMB-D-905`, `AMB-D-906`).
//
// **It is the half a person presses, and what they read opens on the other side of the panes**
// (`AMB-D-835`). Two questions decide which side a thing goes to: whether the person presses it or
// reads it, and whether it fits on one line. A branch name, a count and a changed path all fit and
// are all pressed; a diff and a history are read and wrap, so they belong in the column across the
// panes and not here.
//
// **What it draws is one folder's**, the same one the tree draws (`./RootPick`). A window with two
// answers to which repository it is on is a window where a commit can land in the wrong one, which
// is the whole reason the rail names one folder at the top.
//
// **A folder that is no repository draws the sentence and nothing else.** That is one project in
// twenty-one on this machine, and what the half would otherwise show is a list of nothing, which
// reads as a repository where nothing has happened.
//
// **The two lists are git's own two answers**, not two things a reader has sorted. Every path git
// names carries a letter for what the index says about it and a letter for what the working tree
// says, and a path can have something in both — a file changed, staged, and then changed again is
// in both lists because that is what git will do with it.
import { useEffect, useState } from "react";
import type { FolderGitDto, GitEntryDto } from "../bindings/bindings";
import { t, tf } from "../core/i18n";
import { Icon } from "../components/Icon";
import { errText } from "../core/i18n";
import { pushNotice } from "../core/notice";
import { folderGitIgnore, folderGitStatus, folderGitUntrack, onFolderChanged } from "./folder";
import { FileMenu } from "./FileMenu";
import { useRestore } from "./restore";
import { type GitMark, markOf } from "./gitMark";

/** Nothing read yet, and what a folder that is no repository answers with. */
const NOTHING: FolderGitDto = { prefix: "", branch: null, rows: [] };

/**
 * Where the branch stands and what has changed under it, for the folder `root` names.
 *
 * **It asks the same call the tree's colours come from** (`./folder`), and asks it again on the same
 * word: the host says when the folder moved, and that is the moment to look again. Staging moves not
 * one byte of the working tree and every line of this list, which is why the watch behind that word
 * covers the repository's own directory too (`crate::folder_watch`).
 */
export function GitPanel({ projectId, root, onHistory, onPrefix, onHandOver }: {
  /** The project the folder is bound to; nothing is drawn without one. */
  projectId: number | null;
  /** The folder the window is on, as its path. */
  root: string | null;
  /** Open the history in the column across the panes. Where nothing is handed down there is
   *  nowhere for it to open, and the press is not offered. A path narrows it to that path alone,
   *  spelled from this folder (`./GitHistory`). */
  onHistory?: (path?: string) => void;
  /** The front this folder sits at inside its repository, said as git answers it (`./FolderTree`
   *  hands up the same thing from the other half). */
  onPrefix?: (prefix: string) => void;
  /** Hand a changed path to the pane the reader is working in (`../shell/TerminalFace`). */
  onHandOver?: (wholes: string[]) => void;
}) {
  const [git, setGit] = useState<FolderGitDto>(NOTHING);
  // The row a right-click was on, and where the pointer was. One menu for the half rather than one
  // per row: only one can be open, and a row that held its own would keep it after the list moved
  // under it — which is what every word from the host does to this list.
  const [menu, setMenu] = useState<{ path: string[]; x: number; y: number } | null>(null);
  // Throwing away what git has not recorded, which the menu offers over a row that has some.
  const restore = useRestore(projectId);
  /** False until the first read comes back. Nothing is drawn before it. */
  const [answered, setAnswered] = useState(false);
  // How many times the host has said the folder moved. The read below watches it.
  const [moved, setMoved] = useState(0);

  // The folder is watched by whatever else is drawing it — the tree, and the column reading a file —
  // and every watcher hears the same word. This half takes the news addressed to its own folder and
  // lays no watch of its own: what a watch costs is paid per folder and not per reader, and this
  // half is drawn where the tree is not.
  useEffect(() => {
    if (root === null) return;
    let alive = true;
    const listening = onFolderChanged((fresh) => {
      if (alive && fresh.root === root) setMoved((n) => n + 1);
    });
    return () => {
      alive = false;
      void listening.then((stop) => stop());
    };
  }, [root]);

  useEffect(() => {
    if (projectId === null || root === null) {
      setGit(NOTHING);
      setAnswered(true);
      return;
    }
    let alive = true;
    void folderGitStatus(projectId, root)
      .then((now) => { if (alive) { setGit(now); setAnswered(true); onPrefix?.(now.prefix); } })
      // A folder that went while this was out answers with nothing, which is the same hand as a
      // folder that is no repository — and the half beside this one says which of the two it was.
      .catch(() => { if (alive) { setGit(NOTHING); setAnswered(true); } });
    return () => { alive = false; };
  }, [projectId, root, moved]);

  // Nothing is drawn where there is nothing to draw it about, and where the answer is still out.
  //
  // **No folder** is a project nobody has bound one to, and the face before it has been told which
  // project it is on. The folder half says what to do about that (`./FolderTree`), and the same
  // sentence twice in one column is one column saying it twice.
  //
  // **No answer yet** is the read still on its way. A flash of "this folder is not a repository" on
  // one that is reads as an answer that was looked up and came back no.
  if (projectId === null || root === null || !answered) return <div className="gitpanel" />;
  if (git.branch === null) {
    return (
      <div className="gitpanel">
        <p className="files__none">{t("git.noRepo")}</p>
      </div>
    );
  }

  // git's `X` is what the index says and its `Y` what the working tree says, and a space in either
  // is git saying nothing about that half. `?` is not an index letter — it is git saying it has
  // never seen the path at all — so an untracked file is something to stage and nothing staged.
  const staged = git.rows.filter((row) => row.index !== " " && row.index !== "?");
  const changed = git.rows.filter((row) => row.worktree !== " ");

  return (
    <div className="gitpanel">
      <div className="gitpanel__branch">
        {/* A checkout made at a commit rather than at a branch. git writes no name there, so the
            words say what it is rather than leaving the line empty. */}
        <span className="gitpanel__branchname" title={git.branch.name ?? t("git.detached")}>
          {git.branch.name ?? t("git.detached")}
        </span>
        {/* Only the count there is one of. A pair of zeroes is a branch level with the one it is
            measured by, and drawing them says "nothing to do" in two numbers instead of none. */}
        {git.branch.ahead > 0 && (
          <span className="gitpanel__count" title={tf("git.ahead", { n: git.branch.ahead })}>
            ↑{git.branch.ahead}
          </span>
        )}
        {git.branch.behind > 0 && (
          <span className="gitpanel__count" title={tf("git.behind", { n: git.branch.behind })}>
            ↓{git.branch.behind}
          </span>
        )}
      </div>
      {/* The one press here that opens the other column. The history is there from the moment the
          repository has one, and it is not drawn until somebody asks: what it costs is a call of
          its own, paid by the reader who wants it rather than by everyone (`AMB-T-4899`). The mark
          says where it goes, which is out of this column and across the panes. */}
      {onHistory !== undefined && (
        <button className="gitpanel__open" onClick={() => onHistory()}>
          {t("git.history")}
          <Icon name="foldRight" />
        </button>
      )}
      <Changes
        what={t("git.staged")}
        none={t("git.nothingStaged")}
        rows={staged}
        onMenu={(path, x, y) => setMenu({ path, x, y })}
      />
      <Changes
        what={t("git.changes")}
        none={t("git.nothingChanged")}
        rows={changed}
        onMenu={(path, x, y) => setMenu({ path, x, y })}
      />
      {restore.aside}
      {menu !== null && projectId !== null && root !== null && (
        <FileMenu
          projectId={projectId}
          root={root}
          path={menu.path}
          about={[menu.path]}
          // Every row here is a path git named, and git names files and the folders it answers for
          // whole. What is under a folder it named is not on this list, so nothing here is a folder
          // anything could be written into.
          dir={false}
          at={{ x: menu.x, y: menu.y }}
          onClose={() => setMenu(null)}
          // The bin is the machine's own, and a changed path is a file like any other: what it
          // would take is the file, which is not what a reader pressing here means. So the row is
          // offered git's doors and not that one.
          onTrash={() => setMenu(null)}
          onHandOver={onHandOver}
          git={{
            onHistory: (path) => onHistory?.(path),
            onIgnore: () => {
              void folderGitIgnore(projectId, root, [menu.path])
                .catch((why: unknown) => pushNotice(errText(why)));
            },
            onUntrack: () => {
              void folderGitUntrack(projectId, root, [menu.path])
                .catch((why: unknown) => pushNotice(errText(why)));
            },
            // Only where git says the working tree has done something to it. A path that is only
            // staged has nothing in the working tree to throw away.
            onRestore: changed.some((row) => row.path.join("/") === menu.path.join("/"))
              ? () => restore.askRestore(root, [menu.path])
              : undefined,
          }}
        />
      )}
    </div>
  );
}

/** One of the two lists, under its name — drawn with nothing in it as well, since which of the two
 *  a path is in is the answer, and a list that disappeared would leave the other unnamed. */
function Changes({ what, none, rows, onMenu }: {
  what: string;
  none: string;
  rows: GitEntryDto[];
  onMenu: (path: string[], x: number, y: number) => void;
}) {
  return (
    <section className="gitpanel__section">
      <h3 className="gitpanel__head">{what} {rows.length > 0 && <span>{rows.length}</span>}</h3>
      {rows.length === 0
        ? <p className="files__none">{none}</p>
        : (
          <ul className="gitpanel__list">
            {rows.map((row) => (
              <ChangedRow key={row.path.join("/")} row={row} onMenu={onMenu} />
            ))}
          </ul>
        )}
    </section>
  );
}

/**
 * One path git named, drawn the way the rail draws a row: the mark, the name, and where it is.
 *
 * **The mark is git's own letters and the colour is the tree's.** A reader who has seen a row of the
 * tree go that colour is reading the same answer here, so the two wear one set of colours
 * (`./gitMark`, `AMB-D-785`).
 *
 * **The folder holding it is drawn faintly after the name**, not above it. A list of twenty changes
 * broken into a heading per folder is a page rather than a list, and the names are what a reader
 * runs their eye down.
 */
function ChangedRow({ row, onMenu }: {
  row: GitEntryDto;
  onMenu: (path: string[], x: number, y: number) => void;
}) {
  const name = row.path[row.path.length - 1] ?? "";
  const holding = row.path.slice(0, -1).join("/");
  const mark: GitMark = markOf(row);
  return (
    <li
      className={`gitpanel__row gitpanel__row--${mark}`}
      title={row.path.join("/")}
      onContextMenu={(e) => { e.preventDefault(); onMenu(row.path, e.clientX, e.clientY); }}
    >
      <span className="gitpanel__mark">{letters(row)}</span>
      {/* A folder git named as a whole rather than naming what is inside it, which is what it does
          with an untracked one. The slash is how git writes that, and how the tree reads it. */}
      <span className="gitpanel__name">{name}{row.isDir ? "/" : ""}</span>
      {holding !== "" && <span className="gitpanel__where">{holding}</span>}
    </li>
  );
}

/** The two letters git wrote. The space it writes for "nothing to say" is kept as one that does not
 *  collapse, so the pair stands in one column however much of it git filled in. */
function letters(row: GitEntryDto): string {
  return `${row.index}${row.worktree}`.replaceAll(" ", " ");
}
