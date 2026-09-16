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
// **The three that go out to the remote are run by the window itself**, not written into a pane for
// the agent to run. What that buys is measured (`AMB-T-4900`): the ssh agent reaches a window opened
// from the Dock, and HTTPS goes through the credential helper (`./folder`). What git says on the way
// back is drawn as git wrote it, refusal and all (`AMB-D-906`, 3-4) — which is why the three go down
// together while one of them is out and come back up on git's answer, whenever that is.
//
// **The two lists are git's own two answers**, not two things a reader has sorted. Every path git
// names carries a letter for what the index says about it and a letter for what the working tree
// says, and a path can have something in both — a file changed, staged, and then changed again is
// in both lists because that is what git will do with it.
import { useEffect, useState } from "react";
import type { FolderGitDto, GitEntryDto } from "../bindings/bindings";
import { Icon } from "../components/Icon";
import { errText, t, tf } from "../core/i18n";
import {
  folderGitFetch,
  folderGitPull,
  folderGitPush,
  folderGitStatus,
  onFolderChanged,
} from "./folder";
import { type GitMark, markOf } from "./gitMark";

/** What git wrote on the way back from the remote, and whether it was a refusal. */
type Said = { text: string; refused: boolean };

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
export function GitPanel({ projectId, root, onHistory }: {
  /** The project the folder is bound to; nothing is drawn without one. */
  projectId: number | null;
  /** The folder the window is on, as its path. */
  root: string | null;
  /** Open the history in the column across the panes. Where nothing is handed down there is
   *  nowhere for it to open, and the press is not offered. */
  onHistory?: () => void;
}) {
  const [git, setGit] = useState<FolderGitDto>(NOTHING);
  /** False until the first read comes back. Nothing is drawn before it. */
  const [answered, setAnswered] = useState(false);
  // How many times the host has said the folder moved. The read below watches it.
  const [moved, setMoved] = useState(0);
  // The three buttons are down while one of them is out, and what git wrote comes back to `said`.
  const [running, setRunning] = useState(false);
  const [said, setSaid] = useState<Said | null>(null);

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
      .then((now) => { if (alive) { setGit(now); setAnswered(true); } })
      // A folder that went while this was out answers with nothing, which is the same hand as a
      // folder that is no repository — and the half beside this one says which of the two it was.
      .catch(() => { if (alive) { setGit(NOTHING); setAnswered(true); } });
    return () => { alive = false; };
  }, [projectId, root, moved]);

  // What git said was about the folder it was said of. A reader who moved the window on is owed the
  // new folder's answers and not the last one's, so the line goes with the folder.
  useEffect(() => { setSaid(null); }, [projectId, root]);

  /**
   * Run one of the three and draw what git wrote.
   *
   * **The read is asked for again either way.** A fetch moves where the branch stands against the
   * one it is measured by, and a push moves it back level; both are lines of the answer above. The
   * watch would say so too, four hundred milliseconds later and only for the ones that wrote to
   * `.git` — asking outright is one call and says it now.
   */
  async function reach(call: (projectId: number, root: string) => Promise<string>): Promise<void> {
    if (projectId === null || root === null || running) return;
    setRunning(true);
    setSaid(null);
    try {
      setSaid({ text: await call(projectId, root), refused: false });
    } catch (e) {
      // git's own sentence, in git's own words. It is the only account of why it stopped, and
      // rewriting it into this app's vocabulary would cost the reader the one thing it carries.
      setSaid({ text: errText(e), refused: true });
    } finally {
      setRunning(false);
      setMoved((n) => n + 1);
    }
  }

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
      {/* The remote, in the order a person works it: read it, bring it in, send it. Push carries the
          count of what it would send, which is the one of the two the button is about. */}
      <div className="gitpanel__net">
        <button className="btn" disabled={running} onClick={() => void reach(folderGitFetch)}>
          {t("git.fetch")}
        </button>
        <button className="btn" disabled={running} onClick={() => void reach(folderGitPull)}>
          {t("git.pull")}
        </button>
        <button
          className="btn btn--primary"
          disabled={running}
          onClick={() => void reach(folderGitPush)}
        >
          {t("git.push")}{git.branch.ahead > 0 && ` ↑${git.branch.ahead}`}
        </button>
      </div>
      {running && <p className="gitpanel__said">{t("git.running")}</p>}
      {/* A push that went through says what it sent; a fetch that found nothing says nothing at all,
          and the sentence there is this app's own, since silence on its own reads as a button that
          did not work. */}
      {!running && said !== null && (
        <p className={`gitpanel__said${said.refused ? " gitpanel__said--refused" : ""}`}>
          {said.text === "" ? t("git.quiet") : said.text}
        </p>
      )}
      {/* The one press here that opens the other column. The history is there from the moment the
          repository has one, and it is not drawn until somebody asks: what it costs is a call of
          its own, paid by the reader who wants it rather than by everyone (`AMB-T-4899`). The mark
          says where it goes, which is out of this column and across the panes. */}
      {onHistory !== undefined && (
        <button className="gitpanel__open" onClick={onHistory}>
          {t("git.history")}
          <Icon name="foldRight" />
        </button>
      )}
      <Changes what={t("git.staged")} none={t("git.nothingStaged")} rows={staged} />
      <Changes what={t("git.changes")} none={t("git.nothingChanged")} rows={changed} />
    </div>
  );
}

/** One of the two lists, under its name — drawn with nothing in it as well, since which of the two
 *  a path is in is the answer, and a list that disappeared would leave the other unnamed. */
function Changes({ what, none, rows }: { what: string; none: string; rows: GitEntryDto[] }) {
  return (
    <section className="gitpanel__section">
      <h3 className="gitpanel__head">{what} {rows.length > 0 && <span>{rows.length}</span>}</h3>
      {rows.length === 0
        ? <p className="files__none">{none}</p>
        : (
          <ul className="gitpanel__list">
            {rows.map((row) => <ChangedRow key={row.path.join("/")} row={row} />)}
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
function ChangedRow({ row }: { row: GitEntryDto }) {
  const name = row.path[row.path.length - 1] ?? "";
  const holding = row.path.slice(0, -1).join("/");
  const mark: GitMark = markOf(row);
  return (
    <li className={`gitpanel__row gitpanel__row--${mark}`} title={row.path.join("/")}>
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
