// The rail's other half: what git says about the folder the window is on, and what a reader does
// about it (`AMB-D-905`, `AMB-D-906`).
//
// **It is the half a person presses, and what they read opens on the other side of the panes**
// (`AMB-D-835`). Two questions decide which side a thing goes to: whether the person presses it or
// reads it, and whether it fits on one line. A branch name, a count, a changed path and a commit
// message are all pressed or typed and all fit; a diff and a history are read and wrap, so they
// belong in the column across the panes and not here.
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
//
// **The agent in the pane is not guarded against** (`AMB-D-906`). Nothing here reads whether
// something is running beside it, refuses a press on that ground, or puts a question in the way.
// What closes the hole instead is the shape of the call: every commit names its paths, so the
// pane's half-staged work is not taken along with the reader's — measured at 0 of 3,855 against
// every single time without it (`AMB-T-4901`).
//
// **A refusal is git's own sentence, printed as git wrote it** (`AMB-D-906`, 3-4). It is not
// rewritten, not said again in Amenbo's words, and not turned into a code with a template behind
// it, because a template is a rewriting. What a reader gets here is what they would have got in a
// terminal.
import { useEffect, useState } from "react";
import type { FolderGitDto, GitEntryDto, GitStashDto } from "../bindings/bindings";
import { Menu, MenuItem } from "../components/Menu";
import { errText, t, tf } from "../core/i18n";
import {
  folderGitCommit, folderGitStage, folderGitStash, folderGitStashes, folderGitStashPop,
  folderGitStatus, folderGitUnstage, folderUnwatch, folderWatch, nextWatchTag, onFolderChanged,
} from "./folder";
import { type GitMark, markOf } from "./gitMark";

/** Nothing read yet, and what a folder that is no repository answers with. */
const NOTHING: FolderGitDto = { prefix: "", branch: null, rows: [] };

/**
 * Where the branch stands, what has changed under it, and the doors to do something about it — for
 * the folder `root` names.
 *
 * **It asks the same call the tree's colours come from** (`./folder`), and asks it again on the same
 * word: the host says when the folder moved, and that is the moment to look again. Staging moves not
 * one byte of the working tree and every line of this list, which is why the watch behind that word
 * covers the repository's own directory too (`crate::folder_watch`).
 *
 * **A write asks again itself as well as waiting for that word.** The word comes through a watch
 * that gathers 400ms of them into one (`crate::folder_watch`), which is right for a folder somebody
 * else is writing to and too slow for a box the reader has just ticked.
 */
export function GitPanel({ projectId, root }: {
  /** The project the folder is bound to; nothing is drawn without one. */
  projectId: number | null;
  /** The folder the window is on, as its path. */
  root: string | null;
}) {
  const [git, setGit] = useState<FolderGitDto>(NOTHING);
  /** False until the first read comes back. Nothing is drawn before it. */
  const [answered, setAnswered] = useState(false);
  // How many times there is a reason to look again: the host saying the folder moved, and a write
  // of this half's own coming back. The read below watches it.
  const [moved, setMoved] = useState(0);
  // What is to be written down, until it is. It is the one thing here a reader has typed rather
  // than read, so it is kept until the commit it belongs to is made — a commit git refused leaves
  // the words in the box, where the reader can press again.
  const [message, setMessage] = useState("");
  // A write on its way. What is drawn stands for what git said before it, so nothing is pressed
  // twice while it is out.
  const [writing, setWriting] = useState(false);
  // What git said in refusing, word for word — nothing where the last thing asked of it was done.
  const [refused, setRefused] = useState<string | null>(null);
  // Where the stash list was opened, or nothing while it is shut.
  const [stashAt, setStashAt] = useState<{ x: number; y: number } | null>(null);
  // What is put aside, read when that list opens and not before: it is another call out to git, and
  // a reader who never opens the list never pays for it.
  const [stashes, setStashes] = useState<GitStashDto[]>([]);
  const stashOpen = stashAt !== null;

  // **This half watches the folder itself, for as long as it is drawn.**
  //
  // The other readers of the same folder — the tree, and the column reading a file — each lay their
  // own watch while they are drawn and take it down as they go (`./FolderTree`, `./FilesPanel`), and
  // every watcher hears the same word. This half is drawn exactly where the tree is not, so leaning
  // on the tree's watch would leave it hearing nothing at all: a list that went stale the moment a
  // reader chose to look at it, which is also a list of paths they are about to stage and write down
  // by name.
  //
  // Only while it is drawn. A folder nobody is looking at is a watch held for nothing, and the half
  // reads the whole of what it draws when it comes back.
  useEffect(() => {
    if (projectId === null || root === null) return;
    let alive = true;
    // Which mount of this half it is. The column reading a file watches the same folder and lets go
    // of it at its own moment, so a watch taken down by name alone would leave it with nothing to
    // wake it (`./folder`, `AMB-T-4823`).
    const tag = nextWatchTag();
    // Subscribed before the watch is asked for: the first thing the folder does could happen while
    // the host is still walking it, and a listener set up afterwards would miss exactly that.
    const listening = onFolderChanged((fresh) => {
      if (alive && fresh.root === root) setMoved((n) => n + 1);
    });
    // A folder that cannot be watched is one the read below says the rest about — a folder that has
    // gone answers with nothing either way, and a sentence about the watch would be this half
    // talking about its own machinery.
    void folderWatch(projectId, root, "git", tag).catch(() => {});
    return () => {
      alive = false;
      void listening.then((stop) => stop());
      void folderUnwatch(root, "git", tag);
    };
  }, [projectId, root]);

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

  // The window moving to another folder is the window being about something else: a message meant
  // for this repository is not one to leave sitting over another's list of changes, and a refusal
  // git wrote about this folder says nothing about the next.
  useEffect(() => {
    setMessage("");
    setRefused(null);
    setStashAt(null);
  }, [projectId, root]);

  // Asked each time the list opens, because what is put aside is what is put aside now — and asked
  // again after a write, since popping one is what the list was opened to do.
  useEffect(() => {
    if (!stashOpen || projectId === null || root === null) return;
    let alive = true;
    void folderGitStashes(projectId, root)
      .then((now) => { if (alive) setStashes(now); })
      .catch(() => { if (alive) setStashes([]); });
    return () => { alive = false; };
  }, [stashOpen, projectId, root, moved]);

  /**
   * Ask git for one thing, and hold what it answers.
   *
   * Every door goes through here, so that all of them keep the same three: nothing is pressed twice
   * while one is out, a refusal is kept as git's own words, and the list is read again either way —
   * a call that failed may still have moved something, and a call that worked has certainly moved
   * everything this half draws.
   */
  const ask = (run: () => Promise<void>) => {
    if (writing) return;
    setWriting(true);
    setRefused(null);
    void run()
      .catch((e: unknown) => setRefused(errText(e)))
      .finally(() => {
        setWriting(false);
        setMoved((n) => n + 1);
      });
  };

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
  // What may be put aside: the paths git follows. Naming an untracked one in a stash's pathspec is
  // refused outright — `did not match any file(s) known to git`, with nothing put aside — and
  // handing over no paths at all would take in the pane's working tree along with the reader's.
  const followed = git.rows.filter((row) => row.index !== "?").map((row) => row.path);

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
      <div className="gitpanel__acts">
        <button
          className="btn"
          type="button"
          aria-haspopup="menu"
          aria-expanded={stashOpen}
          disabled={writing}
          // Opened and never toggled here: the list closes itself on the pointer going down
          // anywhere outside it, and this press is one of those (`../components/Menu`).
          onClick={(e) => {
            const box = e.currentTarget.getBoundingClientRect();
            setStashAt({ x: box.left, y: box.bottom });
          }}
        >
          {t("git.stash")}
        </button>
      </div>
      {/* git's own words, as git wrote them: several lines where it wrote several, since the account
          of which files stand in the way of a checkout is the useful half of it. */}
      {refused !== null && <p className="gitpanel__refused">{refused}</p>}
      <div className="gitpanel__commit">
        <textarea
          className="gitpanel__message"
          aria-label={t("git.commitMessage")}
          placeholder={t("git.commitMessage")}
          rows={2}
          value={message}
          onChange={(e) => setMessage(e.target.value)}
        />
        <div className="gitpanel__commitfoot">
          {/* How many paths the commit will name. It is the hinge of this screen, so it is said
              rather than left to be worked out from the list above (`AMB-D-906`, 3-2). */}
          {staged.length > 0 && (
            <span className="gitpanel__hint">{tf("git.commitNaming", { n: staged.length })}</span>
          )}
          <button
            className="btn btn--primary gitpanel__do"
            type="button"
            disabled={writing || staged.length === 0 || message.trim() === ""}
            onClick={() => ask(async () => {
              await folderGitCommit(projectId, root, message, staged.map((row) => row.path));
              setMessage("");
            })}
          >
            {t("git.commit")}
          </button>
        </div>
      </div>
      <Changes
        what={t("git.staged")}
        none={t("git.nothingStaged")}
        rows={staged}
        staged
        writing={writing}
        onToggle={(row) => ask(() => folderGitUnstage(projectId, root, [row.path]))}
      />
      <Changes
        what={t("git.changes")}
        none={t("git.nothingChanged")}
        rows={changed}
        staged={false}
        writing={writing}
        onToggle={(row) => ask(() => folderGitStage(projectId, root, [row.path]))}
      />
      {stashAt !== null && (
        <Menu at={stashAt} onClose={() => setStashAt(null)}>
          {/* Nothing git follows is nothing to put aside, and a door that would only ever come back
              with git's refusal is one to leave out rather than to offer. */}
          {followed.length > 0 && (
            <MenuItem
              onClick={() => {
                setStashAt(null);
                // No message of its own: the line the list is read by is git's, and what it says —
                // the branch it was made on and the commit it was made over — is the sentence a
                // reader is choosing between. A stash named by hand is a second box to type in, and
                // this half has one.
                ask(() => folderGitStash(projectId, root, "", followed));
              }}
            >
              {t("git.stashPush")}
            </MenuItem>
          )}
          {stashes.length === 0
            ? <p className="gitpanel__stashnone">{t("git.stashEmpty")}</p>
            : stashes.map((one, i) => (
              <MenuItem
                key={one.name}
                apart={i === 0 && followed.length > 0}
                onClick={() => {
                  setStashAt(null);
                  // By the name this list was just read with. `stash@{0}` is where a stash sits and
                  // not what it is, so one kept from an earlier read names whatever has since moved
                  // into that place.
                  ask(() => folderGitStashPop(projectId, root, one.name));
                }}
              >
                <span className="gitpanel__stashrow">
                  <span className="gitpanel__stashline">{one.message}</span>
                  <span className="gitpanel__stashdo">{t("git.stashRestore")}</span>
                </span>
              </MenuItem>
            ))}
        </Menu>
      )}
    </div>
  );
}

/** One of the two lists, under its name — drawn with nothing in it as well, since which of the two
 *  a path is in is the answer, and a list that disappeared would leave the other unnamed. */
function Changes({ what, none, rows, staged, writing, onToggle }: {
  what: string;
  none: string;
  rows: GitEntryDto[];
  /** Which of git's two answers this list is, which is what a box in it does when it is pressed. */
  staged: boolean;
  /** A write is out, so nothing here is pressed until it comes back. */
  writing: boolean;
  onToggle: (row: GitEntryDto) => void;
}) {
  return (
    <section className="gitpanel__section">
      <h3 className="gitpanel__head">{what} {rows.length > 0 && <span>{rows.length}</span>}</h3>
      {rows.length === 0
        ? <p className="files__none">{none}</p>
        : (
          <ul className="gitpanel__list">
            {rows.map((row) => (
              <ChangedRow
                key={row.path.join("/")}
                row={row}
                staged={staged}
                writing={writing}
                onToggle={onToggle}
              />
            ))}
          </ul>
        )}
    </section>
  );
}

/**
 * One path git named, drawn the way the rail draws a row: the box, the mark, the name, and where it
 * is.
 *
 * **The box is what the row's list does to it, not a state of the path.** A row of the staged list
 * is staged and its box is ticked; a row of the changes list is a change that is not staged, and its
 * box is empty. A path that is in both — changed, staged, and changed again — is a ticked row and an
 * empty row, which is exactly what git will do with it: part of it is written down and part of it is
 * not.
 *
 * **The mark is git's own letters and the colour is the tree's.** A reader who has seen a row of the
 * tree go that colour is reading the same answer here, so the two wear one set of colours
 * (`./gitMark`, `AMB-D-785`).
 *
 * **The folder holding it is drawn faintly after the name**, not above it. A list of twenty changes
 * broken into a heading per folder is a page rather than a list, and the names are what a reader
 * runs their eye down.
 */
function ChangedRow({ row, staged, writing, onToggle }: {
  row: GitEntryDto;
  staged: boolean;
  writing: boolean;
  onToggle: (row: GitEntryDto) => void;
}) {
  const name = row.path[row.path.length - 1] ?? "";
  const holding = row.path.slice(0, -1).join("/");
  const mark: GitMark = markOf(row);
  const whole = row.path.join("/");
  return (
    <li className={`gitpanel__row gitpanel__row--${mark}`} title={whole}>
      {/* The path is said in full, because the box stands away from the name in the reading order
          of anything that reads the row out. */}
      <input
        className="gitpanel__check"
        type="checkbox"
        checked={staged}
        disabled={writing}
        aria-label={tf(staged ? "git.unstageOne" : "git.stageOne", { path: whole })}
        onChange={() => onToggle(row)}
      />
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
  return `${row.index}${row.worktree}`.replaceAll(" ", " ");
}
