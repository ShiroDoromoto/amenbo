// What the folder the window is on has been through, read in the column across the panes
// (`AMB-D-905`, `AMB-D-906`).
//
// **Three layers in one face, not three faces side by side.** A reader goes list → one commit →
// one of its files, and each press goes a layer deeper into the same tab. Drawn as siblings they
// would have to be closed one at a time, and a tab holding one file's diff would be named after the
// file — which says nothing about which commit it came out of.
//
// **It is read here and pressed over there** (`AMB-D-835`). A patch wraps and is read, so it belongs
// in the widest part of the window; the branch, the counts and the changed paths fit on a line and
// are pressed, so they stay in the rail (`./GitPanel`).
//
// **Nothing here is asked for until the face is up.** Putting the history on the call the tree's
// colours come from doubles that call for every reader, whether or not they are looking at it — one
// trigger is 20.0ms and the pair is 43.2ms (`AMB-T-4899`). So it is asked when the face opens, and
// again while it is open and the folder moves.
import { useEffect, useState } from "react";
import type { GitCommitDto, GitFileDto } from "../bindings/bindings";
import { formatNumber, t } from "../core/i18n";
import { Icon } from "../components/Icon";
import { folderGitDiff, folderGitLog, folderGitShow, onFolderChanged } from "./folder";
import { FileMenu } from "./FileMenu";

/** Which layer the reader is on: the list, one commit, or one file of it. */
export type At = { sha: string; path: string | null } | null;

/**
 * The history of the folder `root` names, as far in as `at` says.
 *
 * `at` is held by the column around this rather than here, because the key that goes back a layer
 * is that column's: one press is one layer, and the layers below this one are its width and itself
 * (`AMB-D-815`, `./FilesPanel`).
 */
export function GitHistory({ projectId, root, prefix, at, onAt, only, onOnly, onFileHistory }: {
  projectId: number | null;
  /** The folder the window is on, as its path. */
  root: string | null;
  /** The front that folder sits at inside its repository, ending in `/` and empty at the root. A
   *  commit names its paths from the root, and this is what turns one of those into the folder's
   *  own spelling — which is what the history of one path takes (`crate::folder_git`). */
  prefix: string;
  at: At;
  onAt: (at: At) => void;
  /**
   * One path the list is narrowed to, as the repository spells it — nothing for the whole of it.
   *
   * **It is not one of the layers.** Narrowing is a different list of the same kind, where a layer
   * is a different kind of thing altogether; so the key that goes back a layer does not widen this,
   * and the control that widens it does not close anything (`AMB-D-815`).
   */
  only: string | null;
  onOnly: (only: string | null) => void;
  /** Narrow the list to one of a commit's paths, handed it in the folder's own spelling. */
  onFileHistory: (path: string) => void;
}) {
  const [commits, setCommits] = useState<GitCommitDto[]>([]);
  const [answered, setAnswered] = useState(false);
  // How many times the host has said the folder moved — a commit made in a pane is one of those.
  const [moved, setMoved] = useState(0);

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
    if (projectId === null || root === null) return;
    let alive = true;
    void folderGitLog(projectId, root, only ?? undefined)
      .then((now) => { if (alive) { setCommits(now); setAnswered(true); } })
      .catch(() => { if (alive) { setCommits([]); setAnswered(true); } });
    return () => { alive = false; };
  }, [projectId, root, only, moved]);

  if (projectId === null || root === null) return <div className="githist" />;
  if (at !== null) {
    const on = commits.find((one) => one.sha === at.sha);
    return (
      <Opened
        projectId={projectId}
        root={root}
        sha={at.sha}
        // The subject of the commit being read, where the list still holds it. A reader who arrived
        // at a file through a filter that has since moved is looking at the right commit under a
        // name that is only missing.
        subject={on?.subject ?? at.sha}
        path={at.path}
        prefix={prefix}
        onAt={onAt}
        onFileHistory={onFileHistory}
      />
    );
  }
  // What the list is of, drawn only where it is of one path: the whole of the folder's history is
  // what this face is, and a control saying so where there is nothing else it could be would be a
  // line every reader has to read past.
  const narrowed = only === null ? null : (
    <div className="githist__only">
      <span className="githist__onlyname" title={only}>{name(only)}</span>
      <button className="githist__all" onClick={() => onOnly(null)}>{t("git.wholeHistory")}</button>
    </div>
  );

  if (!answered) return <div className="githist" />;
  if (commits.length === 0) {
    return (
      <div className="githist">
        {narrowed}
        <p className="files__none">{only === null ? t("git.noHistory") : t("git.noFileHistory")}</p>
      </div>
    );
  }
  return (
    <div className="githist">
      {narrowed}
      <ul className="githist__list">
      {commits.map((one) => (
        <li key={one.sha}>
          <button className="githist__row" onClick={() => onAt({ sha: one.sha, path: null })}>
            <span className="githist__subject">{one.subject}</span>
            <span className="githist__who">
              {/* A merge is told by what it was made on top of rather than by its words: the
                  subject of one is written by whoever merged it, and reads like any other. */}
              {one.parents.length > 1 && <span className="githist__merge">{t("git.merge")}</span>}
              <span className="githist__sha">{one.short}</span>
              {one.author}
            </span>
          </button>
        </li>
      ))}
      </ul>
    </div>
  );
}

/** One commit: the paths it touched, and — a layer deeper — the patch for one of them. */
function Opened({ projectId, root, sha, subject, path, prefix, onAt, onFileHistory }: {
  projectId: number;
  root: string;
  sha: string;
  subject: string;
  path: string | null;
  prefix: string;
  onAt: (at: At) => void;
  onFileHistory: (path: string) => void;
}) {
  const [files, setFiles] = useState<GitFileDto[]>([]);
  // The path a right-click was on, and where the pointer was. One menu for the layer rather than
  // one per row, for the reason every other list here has one.
  const [menu, setMenu] = useState<{ path: string; x: number; y: number } | null>(null);
  // A commit reaches the whole repository, and the menu speaks about the folder the window is on.
  // A path above that folder is one this face has no spelling for, so it carries no menu.
  const held = menu !== null && menu.path.startsWith(prefix)
    ? menu.path.slice(prefix.length).split("/")
    : null;

  useEffect(() => {
    let alive = true;
    void folderGitShow(projectId, root, sha)
      .then((now) => { if (alive) setFiles(now); })
      .catch(() => { if (alive) setFiles([]); });
    return () => { alive = false; };
  }, [projectId, root, sha]);

  return (
    <div className="githist">
      {/* The way back, said as where it goes. It is the same layer the key takes off, so a reader
          who reaches for one and a reader who reaches for the other end up in the same place
          (`AMB-D-815`). */}
      <button
        className="githist__back"
        onClick={() => onAt(path === null ? null : { sha, path: null })}
      >
        <Icon name="chevronLeft" />
        {path === null ? t("git.backToHistory") : subject}
      </button>
      {path === null
        ? (
          <Touched
            files={files}
            onPath={(one) => onAt({ sha, path: one })}
            onHistory={(one, x, y) => setMenu({ path: one, x, y })}
          />
        )
        : <Patch projectId={projectId} root={root} sha={sha} path={path} />}
      {menu !== null && held !== null && (
        <FileMenu
          projectId={projectId}
          root={root}
          path={held}
          about={[held]}
          dir={false}
          at={{ x: menu.x, y: menu.y }}
          onClose={() => setMenu(null)}
          // The bin is about a file on the disk, and this row is about what one commit wrote down.
          onTrash={() => setMenu(null)}
          git={{
            // The one thing to ask of a record of what was written down: what else has happened to
            // this path. The rest of the menu changes the working tree, which is a thing to ask of
            // a row of the tree rather than of a commit's own list.
            onHistory: onFileHistory,
          }}
        />
      )}
    </div>
  );
}

/** The paths one commit touched, with what each gained and lost. */
function Touched({ files, onPath, onHistory }: {
  files: GitFileDto[];
  onPath: (path: string) => void;
  /** Open the menu on one of them, at the point the pointer was. */
  onHistory: (path: string, x: number, y: number) => void;
}) {
  if (files.length === 0) return <p className="files__none">{t("git.touchedNothing")}</p>;
  return (
    <ul className="githist__list">
      {files.map((one) => (
        <li key={one.path}>
          <button
            className="githist__row"
            onClick={() => onPath(one.path)}
            title={one.path}
            onContextMenu={(e) => { e.preventDefault(); onHistory(one.path, e.clientX, e.clientY); }}
          >
            <span className="githist__subject">{name(one.path)}</span>
            <span className="githist__who">
              {/* git counts no lines in a file it read as bytes, and says so with a dash rather
                  than with a nought — which would be a file it read and found unchanged. */}
              {one.added === null || one.removed === null
                ? <span className="githist__sha">{t("git.bytes")}</span>
                : (
                  <>
                    <span className="githist__added">+{formatNumber(one.added)}</span>
                    <span className="githist__removed">−{formatNumber(one.removed)}</span>
                  </>
                )}
              <span className="githist__where">{holding(one.path)}</span>
            </span>
          </button>
        </li>
      ))}
    </ul>
  );
}

/**
 * The patch for one path, as git wrote it.
 *
 * **git's own text, coloured by the character it begins each line with.** What a line means is
 * decided where the diff is made, and reading it back into anything else here would be this side
 * deciding it a second time.
 */
function Patch({ projectId, root, sha, path }: {
  projectId: number;
  root: string;
  sha: string;
  path: string;
}) {
  const [patch, setPatch] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    void folderGitDiff(projectId, root, sha, path)
      .then((now) => { if (alive) setPatch(now); })
      .catch(() => { if (alive) setPatch(""); });
    return () => { alive = false; };
  }, [projectId, root, sha, path]);

  if (patch === null) return <div className="githist__patch" />;
  if (patch === "") return <p className="files__none">{t("git.noPatch")}</p>;
  return (
    <pre className="githist__patch">
      {patch.split("\n").map((line, at) => (
        // The line's place in the patch, which is the only thing that tells two identical lines
        // apart — a patch is full of them.
        // eslint-disable-next-line react/no-array-index-key
        <span key={at} className={`githist__line githist__line--${kindOf(line)}`}>{line}{"\n"}</span>
      ))}
    </pre>
  );
}

/** What one line of a patch is, read off the character git begins it with. */
function kindOf(line: string): "hunk" | "added" | "removed" | "same" {
  // The heads of the file's own two names begin with the same characters a changed line does, and
  // they are three of them rather than one — so they are told apart before the single characters.
  if (line.startsWith("@@")) return "hunk";
  if (line.startsWith("+++") || line.startsWith("---")) return "hunk";
  if (line.startsWith("+")) return "added";
  if (line.startsWith("-")) return "removed";
  return "same";
}

/** The last segment of a path, which is what a row is read by. */
function name(path: string): string {
  return path.slice(path.lastIndexOf("/") + 1);
}

/** The folder holding it, drawn faintly beside the name — empty at the repository's own root. */
function holding(path: string): string {
  const cut = path.lastIndexOf("/");
  return cut < 0 ? "" : path.slice(0, cut);
}
