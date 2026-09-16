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
// **A repository git would not answer about draws git's words instead of that sentence**
// (`AMB-T-4982`). Both come back with no branch, and the two were drawn the same until the sentence
// was being said of folders that are repositories — of one whose `.gitattributes` names a filter the
// window cannot reach, which is what a reader who keeps files in LFS meets every time they save one
// (`AMB-T-4979` measured it).
//
// **The three that go out to the remote are run by the window itself**, not written into a pane for
// the agent to run. What that buys is measured (`AMB-T-4900`): the ssh agent reaches a window opened
// from the Dock, and HTTPS goes through the credential helper (`./folder`). **Where the helper has
// nothing to answer with, the question is put to the reader** rather than failing on a terminal
// nobody is watching (`./GitAsk`, `AMB-D-913`) — which is the other reason a door out can be out for
// as long as it is.
//
// **Whatever git wrote is drawn as git wrote it, refusal and all** (`AMB-D-906`, 3-4) — not
// rewritten, not said again in Amenbo's words, and not turned into a code with a template behind it,
// because a template is a rewriting. What a reader gets here is what they would have got in a
// terminal. That one line is why every door on this half goes down while one of them is out and
// comes back up on git's answer, whenever that is.
//
// **The two lists are git's own two answers**, not two things a reader has sorted. Every path git
// names carries a letter for what the index says about it and a letter for what the working tree
// says, and a path can have something in both — a file changed, staged, and then changed again is
// in both lists because that is what git will do with it.
//
// **What a merge could not settle is a third list, and the one number on this half that git did not
// give it** (`AMB-D-906`, 2-7). The count of conflicts left in a file is read off the file, so it
// falls the moment the file is put right — by the reader on the other side of the panes, or by the
// agent in the pane beside it — where git would go on calling the path unmerged until somebody
// staged it. Nobody else can see that number: a pane shows one file at a time, and this half is the
// only place the question "how much of this merge is left" is asked of the whole folder.
//
// **No screen is drawn for the marks themselves.** A conflicted file is an ordinary file and opens
// in the ordinary editor; the ways to settle one are to write it, to ask the agent in the pane to,
// or to take one side whole off the row's own menu — and all three end at the same number falling.
//
// **The rows are picked out the way the tree's are, and one list at a time** (`./gitPick`). ⌘ takes
// a row in, Shift reaches from where the reader was, and the arrows walk the list — the same answer
// the other half of the rail gives, because the two are one rail. Picking in one list puts down what
// was picked in another: what is done with a set of rows here is staging or unstaging, and a set
// spanning both lists would be a press with two meanings.
//
// **What a set is for is one press and one call out to git.** The box on a row of the set takes the
// whole set, Space presses that box from the keyboard, and the box on the line a list is named on
// takes the list itself — every one of them handing git all the paths at once. Both doors have
// taken several paths all along (`./folder`); it was this side that passed them one at a time,
// which made six files six presses with the whole half down between them.
//
// **What the menu acts on is the set, where it was opened on one of them** (`AMB-D-906`, 2-8). So
// the items are drawn whatever is picked out and greyed where the set is not in a state for them —
// a menu whose items came and went with the selection would be one a reader cannot learn, and with
// rows gathered several at a time a set mixing what git follows with what it has never seen is
// ordinary.
//
// **The agent in the pane is not guarded against** (`AMB-D-906`). Nothing here reads whether
// something is running beside it, refuses a press on that ground, or puts a question in the way.
// What closes the hole instead is the shape of the call: every commit names its paths, so the
// pane's half-staged work is not taken along with the reader's — measured at 0 of 3,855 against
// every single time without it (`AMB-T-4901`).
import { useEffect, useState } from "react";
import type {
  KeyboardEvent as ReactKeyboardEvent, MouseEvent as ReactMouseEvent, ReactNode,
} from "react";
import type { FolderGitDto, GitEntryDto, GitStashDto } from "../bindings/bindings";
import { Icon } from "../components/Icon";
import { Menu, MenuItem } from "../components/Menu";
import { errText, t, tf } from "../core/i18n";
import { hostOs } from "../core/platform";
import {
  folderGitCommit, folderGitFetch, folderGitIgnore, folderGitMarks, folderGitMergeContinue,
  folderGitPull, folderGitPush, folderGitStage, folderGitStash, folderGitStashes, folderGitStashPop,
  folderGitStatus, folderGitTake, folderGitUnstage, folderGitUntrack, folderUnwatch, folderWatch,
  nextWatchTag, onFolderChanged,
} from "./folder";
import { FileMenu } from "./FileMenu";
import { GitBranch } from "./GitBranch";
import { useRestore } from "./restore";
import { type GitMark, markOf } from "./gitMark";
import { type How, kept, keysIn, PICKED_NONE, pick, type Picked, type Which } from "./gitPick";
import type { DiffPick } from "./GitDiff";

/** What git wrote on the way back from a door of this half, and whether it was a refusal. */
type Said = { text: string; refused: boolean };

/** Nothing read yet, and what a folder that is no repository answers with. */
const NOTHING: FolderGitDto = { prefix: "", branch: null, rows: [], merging: false, said: null };

/**
 * Where the branch stands, what has changed under it, and the doors to do something about it — for
 * the folder `root` names.
 *
 * **It asks the same call the tree's colours come from** (`./folder`), and asks it again on the same
 * word: the host says when the folder moved, and that is the moment to look again. Staging moves not
 * one byte of the working tree and every line of this list, which is why the watch behind that word
 * covers the repository's own directory too (`crate::folder_watch`).
 *
 * **A door asks again itself as well as waiting for that word.** The word comes through a watch that
 * gathers four hundred milliseconds of them into one, and a fetch moves where the branch stands
 * without writing a byte anybody watches — asking outright is one call and says it now.
 */
export function GitPanel({
  projectId, root, onHistory, onPrefix, onHandOver, onRead, onPicked, onDiff,
}: {
  /** The project the folder is bound to; nothing is drawn without one. */
  projectId: number | null;
  /** The folder the window is on, as its path. */
  root: string | null;
  /** Open the history in the column across the panes. Where nothing is handed down there is
   *  nowhere for it to open, and the press is not offered. A path narrows it to that path alone,
   *  spelled from this folder (`./GitHistory`). */
  onHistory?: (path?: string) => void;
  /** The front this folder sits at inside its repository, said as git answers it (`./FolderTree`
   *  hands up the same thing from the other half of the rail). */
  onPrefix?: (prefix: string) => void;
  /** Hand a changed path to the pane the reader is working in (`../shell/TerminalFace`). */
  onHandOver?: (wholes: string[]) => void;
  /**
   * Open one of this folder's files in the column across the panes, by the path this half spells.
   *
   * **It is how a conflicted row is reached at all.** This half and the tree are two tabs of one
   * rail (`./FolderRail`), so a reader standing on the list of what the merge could not settle has
   * no tree to find those files in — and what settles a conflict is the file itself
   * (`AMB-D-906`, 2-7).
   */
  onRead?: (path: string[]) => void;
  /**
   * The rows picked out, as the column across the panes reads them — and nothing where the set is
   * put down or is in the list of what a merge could not settle.
   *
   * **It is handed up on every change and not only when the diff is asked for.** What the column
   * draws is what is picked at this moment: a reader with the patches in front of them who presses
   * another row is asking to read that one, the way pressing another file in the tree is
   * (`./GitDiff`, `AMB-D-906`, 2-4).
   *
   * **A conflict is not one of them.** Its two lists are what staging and unstaging act on, and a
   * path a merge left unmerged is one git answers about with a patch of a different kind — this
   * half draws the count of what is left in it instead (`AMB-D-906`, 2-7).
   */
  onPicked?: (picked: DiffPick | null) => void;
  /** Open what the picked rows are holding, in the column across the panes. Where nothing is handed
   *  down there is nowhere for it to open, and the press does nothing. */
  onDiff?: () => void;
}) {
  const [git, setGit] = useState<FolderGitDto>(NOTHING);
  /** False until the first read comes back. Nothing is drawn before it. */
  const [answered, setAnswered] = useState(false);
  // How many times there is a reason to look again: the host saying the folder moved, and a door of
  // this half's own coming back. The read below watches it.
  const [moved, setMoved] = useState(0);
  // Every door is down while one of them is out, and what git wrote comes back to `said`.
  const [running, setRunning] = useState(false);
  const [said, setSaid] = useState<Said | null>(null);
  // What is to be written down, until it is. It is the one thing here a reader has typed rather
  // than read, so it is kept until the commit it belongs to is made — a commit git refused leaves
  // the words in the box, where the reader can press again.
  const [message, setMessage] = useState("");
  // Where the stash list was opened, or nothing while it is shut.
  const [stashAt, setStashAt] = useState<{ x: number; y: number } | null>(null);
  // What is put aside, read when that list opens and not before: it is another call out to git, and
  // a reader who never opens the list never pays for it.
  const [stashes, setStashes] = useState<GitStashDto[]>([]);
  const stashOpen = stashAt !== null;
  // The row a right-click was on, and where the pointer was. One menu for the half rather than one
  // per row: only one can be open, and a row that held its own would keep it after the list moved
  // under it — which is what every word from the host does to this list.
  const [menu, setMenu] = useState<{ which: Which; path: string[]; x: number; y: number } | null>(
    null,
  );
  // The rows a reader picked out, in the list they are in (`./gitPick`). It is what the menu acts
  // on, and it is held here rather than in a list because there is one set for the half: picking in
  // one list is putting down what was picked in another.
  const [picked, setPicked] = useState<Picked>(PICKED_NONE);
  // Throwing away what git has not recorded, which the menu offers over a row that has some.
  const restore = useRestore(projectId);
  // How many conflicts are still written into each path the merge could not settle, by the whole
  // path. A path the count has not come back for yet is not in it, which is not the same as nought.
  const [marks, setMarks] = useState<Record<string, number>>({});

  // git's three answers about a path, kept apart here because the reader does a different thing to
  // each (`rowsIn`).
  const conflicts = rowsIn("conflict", git.rows);
  const staged = rowsIn("staged", git.rows);
  const changed = rowsIn("changed", git.rows);
  const settled = git.rows.filter((row) => !unmerged(row));
  // What git has never seen. It is the one state the menu's doors are not all in: `git rm --cached`
  // refuses a pathspec naming a path git does not follow, and refuses the whole of it — so a set of
  // five rows with one untracked among them is a press that fails for all five.
  const untracked = new Set(git.rows.filter((row) => row.index === "?").map(whole));
  // What may be put aside: the paths git follows. Naming an untracked one in a stash's pathspec is
  // refused outright — `did not match any file(s) known to git`, with nothing put aside — and
  // handing over no paths at all would take in the pane's working tree along with the reader's.
  const followed = settled.filter((row) => row.index !== "?").map((row) => row.path);
  // What the count is asked about, as one word: a list rebuilt on every draw is a new array each
  // time, and the read below is about which paths are in conflict rather than about that array.
  const inConflict = conflicts.map(whole).join("\n");

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
      .then((now) => { if (alive) { setGit(now); setAnswered(true); onPrefix?.(now.prefix); } })
      // A folder that went while this was out answers with nothing, which is the same hand as a
      // folder that is no repository — and the half beside this one says which of the two it was.
      .catch(() => { if (alive) { setGit(NOTHING); setAnswered(true); } });
    return () => { alive = false; };
  }, [projectId, root, moved]);

  // The window moving to another folder is the window being about something else: what git said was
  // said of the folder it was said of, and a message meant for this repository is not one to leave
  // sitting over another's list of changes.
  useEffect(() => {
    setSaid(null);
    setMessage("");
    setStashAt(null);
    setPicked(PICKED_NONE);
  }, [projectId, root]);

  // A path git no longer names is a row nobody can see, and a set holding one is a set the next
  // press acts on silently. Staging one of five rows is the ordinary way it happens: the row leaves
  // the list it was picked in, and the four beside it stay picked.
  useEffect(() => {
    setPicked((now) => kept(now, rowsIn(now.which, git.rows).map(whole)));
  }, [git]);

  // What the column across the panes is holding, handed up as the set moves (`./GitDiff`). It is
  // built from the rows rather than from the keys alone, so what goes up is paths git is still
  // naming — a set is held to the list it was picked from, and this is read after that holding.
  const showing = diffOf(picked, git.rows);
  // The same set as one word, because the object is rebuilt on every draw and what the column is
  // being told is which paths, out of which of the two lists.
  const shown = showing === null
    ? ""
    : `${showing.staged}\n${showing.paths.map((one) => one.join("/")).join("\n")}`;
  useEffect(() => {
    onPicked?.(showing);
    // Told by `shown`, which is the set said as a word.
  }, [shown]);

  // Asked each time the list opens, because what is put aside is what is put aside now — and asked
  // again after a door comes back, since taking one out is what the list was opened to do.
  useEffect(() => {
    if (!stashOpen || projectId === null || root === null) return;
    let alive = true;
    void folderGitStashes(projectId, root)
      .then((now) => { if (alive) setStashes(now); })
      .catch(() => { if (alive) setStashes([]); });
    return () => { alive = false; };
  }, [stashOpen, projectId, root, moved]);

  // **How much of each conflict is left, read from the files and not from git.**
  //
  // git calls a path unmerged until somebody stages it, so it would go on saying "in conflict"
  // about a file that has been put right and not yet declared. The file's own bytes say it the
  // moment they are written — by the reader in the column across the panes, or by the agent in the
  // pane beside it — and `moved` is the word that says to look again (`AMB-D-906`, 2-7).
  //
  // Asked only where there is a conflict to count. A folder with none never pays for this at all.
  useEffect(() => {
    if (projectId === null || root === null || inConflict === "") {
      setMarks({});
      return;
    }
    let alive = true;
    const paths = inConflict.split("\n").map((one) => one.split("/"));
    void folderGitMarks(projectId, root, paths)
      .then((counts) => {
        if (!alive) return;
        setMarks(Object.fromEntries(paths.map((path, at) => [path.join("/"), counts[at] ?? 0])));
      })
      // A file that went while this was out is a row about to stop being drawn, and a number that
      // did not arrive is drawn as no number rather than as none left.
      .catch(() => { if (alive) setMarks({}); });
    return () => { alive = false; };
  }, [projectId, root, inConflict, moved]);

  /**
   * Ask git for one thing, and draw what it wrote.
   *
   * Every door goes through here, so that all of them keep the same three: nothing is pressed twice
   * while one is out, what git said is kept in git's own words, and the read is asked for again
   * either way — a call that failed may still have moved something, and a call that worked has
   * certainly moved what this half draws.
   *
   * `shows` is whether what the call did appears in the lists under it. A box that moved its row
   * from one list to the other has already said that it worked, so git having written nothing about
   * it is nothing to report; a fetch moves nothing on this screen, and silence on its own there
   * reads as a button that did not work.
   */
  async function ask(run: () => Promise<string>, shows = false): Promise<void> {
    if (projectId === null || root === null || running) return;
    setRunning(true);
    setSaid(null);
    try {
      const wrote = await run();
      setSaid(wrote === "" && shows ? null : { text: wrote, refused: false });
    } catch (e) {
      // git's own sentence, in git's own words. It is the only account of why it stopped, and
      // rewriting it into this app's vocabulary would cost the reader the one thing it carries.
      setSaid({ text: errText(e), refused: true });
    } finally {
      setRunning(false);
      setMoved((n) => n + 1);
    }
  }

  /** The three that need nothing but the folder they are about. */
  const reach = (call: (projectId: number, root: string) => Promise<string>): void => {
    if (projectId === null || root === null) return;
    void ask(() => call(projectId, root));
  };

  /** The rows the open menu acts on: the set picked out, where it was opened on one of them. */
  const about = menu === null
    ? []
    : rowsAbout(rowsIn(menu.which, git.rows), keysIn(menu.which, picked), menu.path);
  /** Whether git says the working tree has done something to this path — what there is to throw
   *  away. */
  const dirty = (path: string[]): boolean => changed.some((row) => whole(row) === path.join("/"));
  /** Whether this path is one the merge could not settle. */
  const conflicted = (path: string[]): boolean =>
    conflicts.some((row) => whole(row) === path.join("/"));

  // Nothing is drawn where there is nothing to draw it about, and where the answer is still out.
  //
  // **No folder** is a project nobody has bound one to, and the face before it has been told which
  // project it is on. The folder half says what to do about that (`./FolderTree`), and the same
  // sentence twice in one column is one column saying it twice.
  //
  // **No answer yet** is the read still on its way. A flash of "this folder is not a repository" on
  // one that is reads as an answer that was looked up and came back no.
  //
  // **git refusing to answer is neither of those.** It is a repository — the host asked git where
  // it sits before asking what has changed in it — and what stands in the way is a sentence of
  // git's about this machine: a filter it cannot run, an index it may not read. So git's own words
  // are drawn, under a line saying whose they are, rather than the sentence about a folder that is
  // no repository, which would be a plain falsehood and one nobody can act on (`AMB-T-4982`).
  if (projectId === null || root === null || !answered) return <div className="gitpanel" />;
  if (git.said !== null) {
    return (
      <div className="gitpanel">
        <p className="files__none">{t("git.noAnswer")}</p>
        <p className="gitpanel__said gitpanel__said--refused">
          {git.said === "" ? t("git.quiet") : git.said}
        </p>
      </div>
    );
  }
  if (git.branch === null) {
    return (
      <div className="gitpanel">
        <p className="files__none">{t("git.noRepo")}</p>
      </div>
    );
  }

  return (
    <div className="gitpanel">
      {/* **Only the lists scroll.** Where the branch stands and the three that go out to the remote
          are above them, the box a commit is written in and the press that writes it below — and
          none of the four moves, however many paths git names. They are what a reader presses, and
          that last press is what the whole half is worked towards (`AMB-D-906`, 2-2). */}
      <div className="gitpanel__top">
        {/* The branch line is its own, because it is the one part of this half whose doors are all
            about the branch: moving onto another, making one, bringing one in, and getting back out
            of a merge that is underway (`./GitBranch`). */}
        <GitBranch
          projectId={projectId}
          root={root}
          on={git.branch}
          merging={git.merging}
          onMoved={() => setMoved((n) => n + 1)}
        />
        {/* The remote, in the order a person works it: read it, bring it in, send it. Push carries the
            count of what it would send, which is the one of the two the button is about, and the
            accent is on that same count rather than on the button: filled with nothing to send, it
            recommends a press that would do nothing, next to a label that already says so by leaving
            the count off. Then what is put aside, which is the one door here that opens a list rather
            than doing a thing. */}
        <div className="gitpanel__net">
          <button className="btn" disabled={running} onClick={() => reach(folderGitFetch)}>
            {t("git.fetch")}
          </button>
          <button className="btn" disabled={running} onClick={() => reach(folderGitPull)}>
            {t("git.pull")}
          </button>
          <button
            className={`btn${git.branch.ahead > 0 ? " btn--primary" : ""}`}
            disabled={running}
            onClick={() => reach(folderGitPush)}
          >
            {t("git.push")}{git.branch.ahead > 0 && ` ↑${git.branch.ahead}`}
          </button>
          <button
            className="btn"
            type="button"
            aria-haspopup="menu"
            aria-expanded={stashOpen}
            disabled={running}
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
          <button className="gitpanel__open" onClick={() => onHistory()}>
            {t("git.history")}
            <Icon name="foldRight" />
          </button>
        )}
      </div>
      {/* The one part that scrolls. What the merge could not settle stands at its head — which is
          the order a reader works it in — and is drawn whenever git names an unmerged path, merge
          or not: a rebase leaves the same rows, and a list that appeared only for one of them would
          leave the other with rows in no list at all. */}
      <div className="gitpanel__lists">
        {conflicts.length > 0 && (
          <Conflicts
            rows={conflicts}
            marks={marks}
            running={running}
            picked={picked}
            onPicked={setPicked}
            onOpen={onRead}
            // Staging it is the whole of "I say this one is settled" — nothing does it for the
            // reader, however few marks are left in the file (`AMB-D-906`, 2-7).
            onSettle={(row) => void ask(() => folderGitStage(projectId, root, [row.path]), true)}
            onMenu={(path, x, y) => setMenu({ which: "conflict", path, x, y })}
          />
        )}
        <Changes
          what={t("git.staged")}
          none={t("git.nothingStaged")}
          rows={staged}
          staged
          running={running}
          which="staged"
          picked={picked}
          onPicked={setPicked}
          onToggle={(paths) => void ask(() => folderGitUnstage(projectId, root, paths), true)}
          onMenu={(path, x, y) => setMenu({ which: "staged", path, x, y })}
          onOpen={onDiff}
        />
        <Changes
          what={t("git.changes")}
          none={t("git.nothingChanged")}
          rows={changed}
          staged={false}
          running={running}
          which="changed"
          picked={picked}
          onPicked={setPicked}
          onToggle={(paths) => void ask(() => folderGitStage(projectId, root, paths), true)}
          onMenu={(path, x, y) => setMenu({ which: "changed", path, x, y })}
          onOpen={onDiff}
        />
      </div>
      <div className="gitpanel__commit">
        {/* **No box while a merge is under way.** git wrote the message when it began the merge and
            takes that one, so a box here would be one a reader types into and is never asked
            for. */}
        {!git.merging && (
          <textarea
            className="gitpanel__message"
            aria-label={t("git.commitMessage")}
            placeholder={t("git.commitMessage")}
            rows={2}
            value={message}
            onChange={(e) => setMessage(e.target.value)}
          />
        )}
        <div className="gitpanel__commitfoot">
          {/* While a merge is open, how much of it is left; otherwise how many paths the commit
              will name. Either is the hinge of this screen, so it is said rather than left to be
              worked out from the lists below (`AMB-D-906`, 3-2). */}
          {git.merging ? (
            <span className="gitpanel__hint">
              {conflicts.length > 0
                ? tf("git.conflictsLeft", { n: conflicts.length })
                : t("git.conflictsSettled")}
            </span>
          ) : staged.length > 0 && (
            <span className="gitpanel__hint">{tf("git.commitNaming", { n: staged.length })}</span>
          )}
          {/* The one press, under whichever name the folder's state gives it. Ending a merge is a
              commit git has already written the message for, so the two never stand together —
              and while one path is still unmerged git refuses it, which is why it is down until
              the list above is empty. */}
          {git.merging ? (
            <button
              className="btn btn--primary gitpanel__do"
              type="button"
              disabled={running || conflicts.length > 0}
              onClick={() => void ask(() => folderGitMergeContinue(projectId, root), true)}
            >
              {t("git.mergeContinue")}
            </button>
          ) : (
            <button
              className="btn btn--primary gitpanel__do"
              type="button"
              disabled={running || staged.length === 0 || message.trim() === ""}
              onClick={() => void ask(async () => {
                const wrote = await folderGitCommit(
                  projectId, root, message, staged.map((row) => row.path),
                );
                setMessage("");
                return wrote;
              }, true)}
            >
              {t("git.commit")}
            </button>
          )}
        </div>
      </div>
      {restore.aside}
      {menu !== null && (
        <FileMenu
          projectId={projectId}
          root={root}
          path={menu.path}
          about={about}
          // Every row here is a path git named, and git names files and the folders it answers for
          // whole. What is under a folder it named is not on this list, so nothing here is a folder
          // anything could be written into.
          dir={false}
          at={{ x: menu.x, y: menu.y }}
          onClose={() => setMenu(null)}
          // The bin is the machine's own, and what it would take is the file — which is not what a
          // reader pressing on a changed path means. So the row is offered git's doors and not it.
          onTrash={() => setMenu(null)}
          onHandOver={onHandOver}
          git={{
            onHistory: (path) => onHistory?.(path),
            onIgnore: () => {
              void folderGitIgnore(projectId, root, about)
                .catch((why: unknown) => setSaid({ text: errText(why), refused: true }));
            },
            onUntrack: () => {
              void ask(() => folderGitUntrack(projectId, root, about), true);
            },
            // Drawn and not pressed where git has never seen one of the rows: `git rm --cached`
            // refuses a pathspec naming such a path, and refuses the whole of it.
            followsAll: about.every((path) => !untracked.has(path.join("/"))),
            // Only where git says the working tree has done something to one of them, and then to
            // those alone. A path that is only staged has nothing in the working tree to throw
            // away, and the one act here that cannot be walked back is the last to offer idly.
            onRestore: about.some(dirty)
              ? () => restore.askRestore(root, about.filter(dirty))
              : undefined,
            // The short way out of a conflict, and only where every row is in one: over any other
            // path git refuses it with its own sentence about the path not being unmerged.
            onTake: about.length > 0 && about.every(conflicted)
              ? (mine) => void ask(() => folderGitTake(projectId, root, about, mine), true)
              : undefined,
          }}
        />
      )}
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
                void ask(() => folderGitStash(projectId, root, "", followed), true);
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
                  void ask(() => folderGitStashPop(projectId, root, one.name), true);
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

/**
 * Whether git says the merge could not settle this path.
 *
 * git writes the pair of letters for that seven ways — one side changed it while the other deleted
 * it, both added it, both deleted it — and what they have in common is a `U` on one side, or the
 * same letter on both. It is read off the row rather than asked for again, because the row is
 * already carrying git's own answer.
 */
function unmerged(row: GitEntryDto): boolean {
  if (row.index === "U" || row.worktree === "U") return true;
  return row.index === row.worktree && (row.index === "A" || row.index === "D");
}

/** A row's path as one word, which is how a picked row is named (`./gitPick`). */
function whole(row: GitEntryDto): string {
  return row.path.join("/");
}

/**
 * The rows of one of the half's three lists, read off git's answer.
 *
 * git's `X` is what the index says and its `Y` what the working tree says, and a space in either is
 * git saying nothing about that half. `?` is not an index letter — it is git saying it has never
 * seen the path at all — so an untracked file is something to stage and nothing staged. A conflict
 * is in neither of the two: what a box would do to it is stage it, and staging a conflict is the
 * one press that says it is settled (`AMB-D-906`, 2-7).
 *
 * Read in two places, and so written once: the lists are drawn from it, and a set of picked rows is
 * held to it when git is asked again (`./gitPick`).
 */
function rowsIn(which: Which | null, rows: GitEntryDto[]): GitEntryDto[] {
  if (which === "conflict") return rows.filter(unmerged);
  const settled = rows.filter((row) => !unmerged(row));
  if (which === "staged") return settled.filter((row) => row.index !== " " && row.index !== "?");
  if (which === "changed") return settled.filter((row) => row.worktree !== " ");
  return [];
}

/**
 * The rows an act aimed at one row is about: the ones picked out, where that row is among them —
 * and the row alone, where it is not.
 *
 * The same rule the tree reads a menu by (`./FolderTree`). A menu opened away from what a reader
 * gathered is a menu about the row under the pointer, because the alternative is a box standing
 * over one row and acting on others.
 */
function rowsAbout(rows: GitEntryDto[], picked: string[], path: string[]): string[][] {
  if (!picked.includes(path.join("/"))) return [path];
  return rows.filter((row) => picked.includes(whole(row))).map((row) => row.path);
}

/**
 * The picked rows as the column across the panes reads them, or nothing where there is no patch to
 * read: an empty set, or one in the list of what a merge could not settle.
 *
 * Which of git's two answers the set is in *is* the question the column asks git — what the working
 * tree holds that the index does not, or what the index holds that the last commit does not — so it
 * travels with the paths rather than being worked out again over there (`crate::folder_git`).
 */
function diffOf(picked: Picked, rows: GitEntryDto[]): DiffPick | null {
  if (picked.which !== "changed" && picked.which !== "staged") return null;
  const paths = rowsIn(picked.which, rows)
    .filter((row) => picked.keys.includes(whole(row)))
    .map((row) => row.path);
  return paths.length === 0 ? null : { paths, staged: picked.which === "staged" };
}

/** Which of the three a press is, by what the reader was holding down. Which key takes a row in is
 *  the machine's answer: Ctrl and a press is how a Mac asks for the menu (`../core/platform`). */
function howOf(e: { shiftKey: boolean; metaKey: boolean; ctrlKey: boolean }): How {
  if (e.shiftKey) return "spread";
  return (hostOs() === "macos" ? e.metaKey : e.ctrlKey) ? "add" : "one";
}

/**
 * What the merge could not settle: one row per path, with how much of it is still in conflict.
 *
 * **The number is what this window has that no pane does** (`AMB-D-906`, 2-7). It is counted off
 * the file rather than off git's index, so it drops the moment the file is put right — by the
 * reader on the other side of the panes, or by the agent in the pane beside it — and neither has to
 * tell anybody they are done.
 *
 * **Nothing here stages anything by itself.** A file with no marks left in it is a file the reader
 * is offered the one press that says so, which is what that press means: VS Code asks the same
 * question in words before it stages a conflict (`AMB-T-4919`).
 *
 * **There is no box on these rows.** A box would be the two lists' box, and what it does there is
 * stage — which on a conflict is the declaration above and not a thing to tick idly.
 */
function Conflicts({ rows, marks, running, picked, onPicked, onOpen, onSettle, onMenu }: {
  rows: GitEntryDto[];
  /** How many marks are left in each path, by the whole path. A path that is not in it is one the
   *  count has not come back for, which is drawn as no number rather than as none. */
  marks: Record<string, number>;
  /** A door is out, so nothing here is pressed until it comes back. */
  running: boolean;
  /** The half's one set of picked rows, which this list draws its own of (`./gitPick`). */
  picked: Picked;
  onPicked: (picked: Picked) => void;
  /** Open the file on the other side of the panes. Absent where there is nowhere to open it. */
  onOpen?: (path: string[]) => void;
  /** Say this one is settled, which is to stage it. */
  onSettle: (row: GitEntryDto) => void;
  onMenu: (path: string[], x: number, y: number) => void;
}) {
  const on = picking("conflict", rows, picked, onPicked, onMenu);
  return (
    <section className="gitpanel__section gitpanel__section--conflict">
      {/* No box on this line. What a box does in the two lists below is stage, and staging a
          conflict is the reader saying the merge is settled there (`AMB-D-906`, 2-7) — the last
          thing to offer over a whole list at once. */}
      <div className="gitpanel__headrow">
        <h3 className="gitpanel__head">{t("git.conflicts")} <span>{rows.length}</span></h3>
      </div>
      <RowList what={t("git.conflicts")} on={on}>
        {rows.map((row) => (
          <ConflictRow
            key={whole(row)}
            row={row}
            left={marks[whole(row)]}
            running={running}
            picked={on.has(whole(row))}
            stop={on.stop === whole(row)}
            onPress={(how) => on.press(whole(row), how)}
            onOpen={onOpen}
            onSettle={onSettle}
            onMenu={on.menu}
          />
        ))}
      </RowList>
    </section>
  );
}

/**
 * What one list answers the reader's hand with: which of its rows are picked out, where the tab
 * stop is, and what a press or a walk does to the set (`./gitPick`).
 *
 * **Written once for the three lists.** They draw different rows — a conflict carries a count and
 * no box — and a reader gathers rows in all three the same way, so the gathering is here and the
 * drawing is in each list.
 */
function picking(
  which: Which,
  rows: GitEntryDto[],
  picked: Picked,
  onPicked: (picked: Picked) => void,
  onMenu: (path: string[], x: number, y: number) => void,
) {
  const keys = rows.map(whole);
  const mine = keysIn(which, picked);
  return {
    keys,
    /** Whether this row is one of the picked. */
    has: (key: string): boolean => mine.includes(key),
    /** Where the tab stop is: the end the range is measured from, or the first row before a reader
     *  has touched the list. Every list keeps one, so Tab reaches each of them. */
    stop: (picked.which === which ? picked.anchor : null) ?? keys[0],
    press: (key: string, how: How): void => { onPicked(pick(which, keys, picked, key, how)); },
    /** Walking with the arrows, from the row the keyboard is on to the one beside it. */
    walk: (from: string, to: string, spread: boolean): void => {
      onPicked(pick(which, keys, picked, to, spread ? "spread" : "one", from));
    },
    /**
     * The menu the row carries, opened after the row has been stood on.
     *
     * A menu opened away from what is picked is a menu about this row alone: the set is put down,
     * because the alternative is a box standing over one row and acting on others. Opened on a row
     * already in the set it changes nothing — that is the press a reader makes to act on what they
     * gathered.
     */
    menu: (path: string[], x: number, y: number): void => {
      if (!mine.includes(path.join("/"))) onPicked(pick(which, keys, picked, path.join("/"), "one"));
      onMenu(path, x, y);
    },
  };
}

/** What a list hands its rows, as `picking` answers it. */
type Picking = ReturnType<typeof picking>;

/**
 * The box one list's rows are drawn in, and the arrows that walk them.
 *
 * **It is a grid and not a list of choices.** The rows carry things to press — a box that stages,
 * a press that says a conflict is settled — and what may hold a control is a cell of a grid, where
 * a choice may not. That is also what lets a row say it is one of the picked (`aria-selected`)
 * while the box inside it goes on saying whether the path is staged.
 *
 * A key with the machine's own on it is not this list's, the same way it is not the tree's
 * (`./FolderTree`): what the reader means by ⌘ or Ctrl is the machine's word. Shift is the one
 * exception, and it reaches from the end the range is measured from to where the walk arrived.
 */
function RowList({ what, on, onSpace, children }: {
  /** The name over the list, which is what this box is called by anything reading it out. */
  what: string;
  on: Picking;
  /**
   * Space on the row the keyboard is standing on, where the list has a box for it to press.
   *
   * **Absent on the list of conflicts.** The box is what the other two lists do to a row, and what
   * it would do to a conflict is stage it — which is the reader declaring the merge settled there,
   * not a thing to tick in passing (`AMB-D-906`, 2-7).
   */
  onSpace?: (key: string) => void;
  children: ReactNode;
}) {
  const onKey = (e: ReactKeyboardEvent<HTMLUListElement>) => {
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    const row = (e.target as HTMLElement).closest<HTMLElement>('[role="row"]');
    if (row === null || !e.currentTarget.contains(row)) return;
    const at = on.keys.indexOf(row.dataset.key ?? "");
    if (at < 0) return;
    const list = e.currentTarget;
    const go = (to: string | undefined) => {
      if (to === undefined) return;
      e.preventDefault();
      on.walk(on.keys[at], to, e.shiftKey);
      // The row itself, because the stop follows the set and the set has only just been told to
      // move: a reader walking a list is standing on the row they arrived at, not on the one they
      // left.
      list.querySelector<HTMLElement>(`[data-key="${CSS.escape(to)}"]`)?.focus();
    };
    switch (e.key) {
      case "ArrowDown": go(on.keys[at + 1]); break;
      case "ArrowUp": go(on.keys[at - 1]); break;
      // The two ends of the list are reached the way the steps are: a reader holding Shift is
      // asking for everything between, however far away the end is.
      case "Home": go(on.keys[0]); break;
      case "End": go(on.keys[on.keys.length - 1]); break;
      // The box of the row the keyboard is on, pressed from the keyboard. The box itself is not a
      // tab stop — the row is — so without this a reader working the list by keyboard could gather
      // rows and then have no way to stage them.
      case " ":
        e.preventDefault();
        onSpace?.(on.keys[at]);
        break;
    }
  };
  return (
    <ul
      className="gitpanel__list"
      role="grid"
      aria-label={what}
      // Said on the list, because it is a fact about the list and not about any one row: a reader
      // being read to is told the rows can be picked out several at a time before they meet one.
      aria-multiselectable
      onKeyDown={onKey}
    >
      {children}
    </ul>
  );
}

/**
 * One path the merge could not settle: git's letters, the name, and where it stands.
 *
 * **The name is the way into the file**, because this half and the tree are two tabs of one rail:
 * a reader standing here has no tree to find the file in, and the file is where a conflict is
 * settled. What opens is the ordinary editor — a conflicted file is an ordinary file, and Amenbo
 * draws no screen of its own for the marks in it (`AMB-D-906`, 2-7).
 *
 * **What stands at the end is the count, until there is none — and then the press.** The two never
 * stand together: while something is left to settle there is nothing to declare, and once nothing
 * is left the number would be a nought nobody needs to read.
 */
function ConflictRow({ row, left, running, picked, stop, onPress, onOpen, onSettle, onMenu }: {
  row: GitEntryDto;
  left: number | undefined;
  running: boolean;
  /** Whether the reader has this row in the set they gathered (`./gitPick`). */
  picked: boolean;
  /** Whether this row holds the list's tab stop. */
  stop: boolean;
  /** A press on the row itself, which is what moves the set. */
  onPress: (how: How) => void;
  onOpen?: (path: string[]) => void;
  onSettle: (row: GitEntryDto) => void;
  onMenu: (path: string[], x: number, y: number) => void;
}) {
  const name = row.path[row.path.length - 1] ?? "";
  const holding = row.path.slice(0, -1).join("/");
  const path = whole(row);
  const said = (
    <>
      <span className="gitpanel__mark">{letters(row)}</span>
      <span className="gitpanel__name">{name}</span>
      {holding !== "" && <span className="gitpanel__where">{holding}</span>}
    </>
  );
  return (
    <li
      className={`gitpanel__row gitpanel__row--conflict${picked ? " gitpanel__row--picked" : ""}`}
      role="row"
      data-key={path}
      aria-selected={picked}
      tabIndex={stop ? 0 : -1}
      title={path}
      onClick={(e) => press(e, onPress)}
      onContextMenu={(e) => {
        e.preventDefault();
        // Stood on before the menu opens, because the row a menu is about is the row a reader comes
        // back to when it closes — and a right-click is not a press the browser moves the focus for.
        e.currentTarget.focus();
        onMenu(row.path, e.clientX, e.clientY);
      }}
    >
      <span className="gitpanel__cell" role="gridcell">
        {onOpen === undefined
          ? <span className="gitpanel__conflictname">{said}</span>
          : (
            <button
              className="gitpanel__conflictname gitpanel__conflictopen"
              type="button"
              // The set is left where it is: this press is the way into the file, and a reader who
              // gathered five rows to act on has not put them down by reading one of them.
              onClick={(e) => { e.stopPropagation(); onOpen(row.path); }}
            >
              {said}
            </button>
          )}
        {left === undefined ? null : left > 0
          ? <span className="gitpanel__marks">{tf("git.conflictMarks", { n: left })}</span>
          : (
            <button
              className="btn gitpanel__settle"
              type="button"
              disabled={running}
              onClick={(e) => { e.stopPropagation(); onSettle(row); }}
            >
              {t("git.settleOne")}
            </button>
          )}
      </span>
    </li>
  );
}

/**
 * A press on a row, as the set of picked rows hears it.
 *
 * **Ctrl and a press is how a Mac asks for the menu**, and the webview may or may not send a click
 * beside that menu — so the press is let go of there rather than read as the plain one it is not
 * (`./FolderTree` reads it the same way).
 */
function press(e: ReactMouseEvent<HTMLElement>, onPress: (how: How) => void): void {
  if (hostOs() === "macos" && e.ctrlKey) return;
  onPress(howOf(e));
}

/** Whether a press landed on the box a row carries, rather than on the row itself. */
function inBox(target: EventTarget | null): boolean {
  return target instanceof Element && target.closest(".gitpanel__check") !== null;
}

/** One of the two lists, under its name — drawn with nothing in it as well, since which of the two
 *  a path is in is the answer, and a list that disappeared would leave the other unnamed. */
function Changes({
  what, none, rows, staged, running, which, picked, onPicked, onToggle, onMenu, onOpen,
}: {
  what: string;
  none: string;
  rows: GitEntryDto[];
  /** Which of git's two answers this list is, which is what a box in it does when it is pressed. */
  staged: boolean;
  /** A door is out, so nothing here is pressed until it comes back. */
  running: boolean;
  /** Which list this is to the set of picked rows (`./gitPick`). */
  which: Which;
  picked: Picked;
  onPicked: (picked: Picked) => void;
  /** Stage these paths, or take them back out — whichever this list's box does, in one call. */
  onToggle: (paths: string[][]) => void;
  /** Open the menu this row carries, at the point the pointer was (`./FileMenu`). */
  onMenu: (path: string[], x: number, y: number) => void;
  /** Read what the picked rows are holding, in the column across the panes. Absent where there is
   *  nowhere to read it. */
  onOpen?: () => void;
}) {
  const on = picking(which, rows, picked, onPicked, onMenu);
  /** What pressing one row's box is about: the set, where that row is in it, and the row alone
   *  where it is not — the rule the menu is read by (`rowsAbout`). */
  const toggle = (key: string): void => {
    const row = rows.find((one) => whole(one) === key);
    if (row !== undefined) onToggle(rowsAbout(rows, keysIn(which, picked), row.path));
  };
  return (
    <section className="gitpanel__section">
      {/* The name of the list, and before it the box that takes the whole of it at once. The box is
          beside the heading rather than inside it: what it does is this list's, not part of what
          the list is called. */}
      <div className="gitpanel__headrow">
        {rows.length > 0 && (
          <input
            className="gitpanel__check"
            type="checkbox"
            checked={staged}
            disabled={running}
            aria-label={t(staged ? "git.unstageAll" : "git.stageAll")}
            onChange={() => onToggle(rows.map((row) => row.path))}
          />
        )}
        <h3 className="gitpanel__head">{what} {rows.length > 0 && <span>{rows.length}</span>}</h3>
      </div>
      {rows.length === 0
        ? <p className="files__none">{none}</p>
        : (
          <RowList what={what} on={on} onSpace={toggle}>
            {rows.map((row) => (
              <ChangedRow
                key={whole(row)}
                row={row}
                staged={staged}
                running={running}
                picked={on.has(whole(row))}
                stop={on.stop === whole(row)}
                onPress={(how) => on.press(whole(row), how)}
                onToggle={() => toggle(whole(row))}
                onMenu={on.menu}
                onOpen={onOpen}
              />
            ))}
          </RowList>
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
 * **Pressed on a row of the set it is about the set**, and about this row alone where the row is
 * not in one — the rule the menu is read by (`rowsAbout`). It stays ticked or empty either way: what
 * it says is which list the row is in, and five rows of one list are in the same one.
 *
 * **The mark is git's own letters and the colour is the tree's.** A reader who has seen a row of the
 * tree go that colour is reading the same answer here, so the two wear one set of colours
 * (`./gitMark`, `AMB-D-785`).
 *
 * **The folder holding it is drawn faintly after the name**, not above it. A list of twenty changes
 * broken into a heading per folder is a page rather than a list, and the names are what a reader
 * runs their eye down.
 *
 * **A second press reads what the row is holding**, the way a second press on a row of the tree
 * opens the file (`AMB-D-835`). The first of the two has already put the row in the set — on its
 * own, where nothing else was picked — so what opens across the panes is what the reader is looking
 * at (`./GitDiff`). The box is not a way in: pressing it twice is staging and unstaging, and a
 * reader doing that is not asking to read anything — which is why the second press is turned away
 * here rather than stopped at the box. The first of the two puts every door of this half down, and
 * a press on a control that has gone down arrives at the row around it instead.
 */
function ChangedRow({
  row, staged, running, picked, stop, onPress, onToggle, onMenu, onOpen,
}: {
  row: GitEntryDto;
  staged: boolean;
  running: boolean;
  /** Whether the reader has this row in the set they gathered (`./gitPick`). */
  picked: boolean;
  /** Whether this row holds the list's tab stop. */
  stop: boolean;
  /** A press on the row itself, which is what moves the set. */
  onPress: (how: How) => void;
  /** Press this row's box, which the list reads as being about the set where this row is in it. */
  onToggle: () => void;
  onMenu: (path: string[], x: number, y: number) => void;
  /** Read what the picked rows are holding. Absent where there is nowhere to read it. */
  onOpen?: () => void;
}) {
  const name = row.path[row.path.length - 1] ?? "";
  const holding = row.path.slice(0, -1).join("/");
  const mark: GitMark = markOf(row);
  const path = whole(row);
  return (
    <li
      className={`gitpanel__row gitpanel__row--${mark}${picked ? " gitpanel__row--picked" : ""}`}
      role="row"
      data-key={path}
      aria-selected={picked}
      tabIndex={stop ? 0 : -1}
      title={path}
      onClick={(e) => press(e, onPress)}
      onDoubleClick={(e) => { if (!inBox(e.target)) onOpen?.(); }}
      onContextMenu={(e) => {
        e.preventDefault();
        e.currentTarget.focus();
        onMenu(row.path, e.clientX, e.clientY);
      }}
    >
      <span className="gitpanel__cell" role="gridcell">
        {/* The path is said in full, because the box stands away from the name in the reading order
            of anything that reads the row out.

            **The set is left where it is when the box is pressed.** The box is what this list does
            to one path, and a reader who gathered five rows to stage together has not begun again
            by ticking one of them. */}
        <input
          className="gitpanel__check"
          type="checkbox"
          checked={staged}
          disabled={running}
          aria-label={tf(staged ? "git.unstageOne" : "git.stageOne", { path })}
          onClick={(e) => e.stopPropagation()}
          onChange={() => onToggle()}
        />
        <span className="gitpanel__mark">{letters(row)}</span>
        {/* A folder git named as a whole rather than naming what is inside it, which is what it does
            with an untracked one. The slash is how git writes that, and how the tree reads it. */}
        <span className="gitpanel__name">{name}{row.isDir ? "/" : ""}</span>
        {holding !== "" && <span className="gitpanel__where">{holding}</span>}
      </span>
    </li>
  );
}

/** The two letters git wrote. The space it writes for "nothing to say" is kept as one that does not
 *  collapse, so the pair stands in one column however much of it git filled in. */
function letters(row: GitEntryDto): string {
  return `${row.index}${row.worktree}`.replaceAll(" ", " ");
}
