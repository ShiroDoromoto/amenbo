// The branch line of the rail's git half: where the folder is standing, and the way onto another
// branch (`AMB-D-906`, 2-6 and 3-4).
//
// **The name is made in the list, never in a window of its own.** Pressing for a new branch turns
// that row into a box to type in, the way a name is made in the tree (`./FolderTree`). A window
// opened over the list would be a second place to look at while the question is about the row under
// the pointer.
//
// **What git refuses with is what the reader is shown.** Moving onto a branch that changes a file
// being edited exits 1 with git's own three lines about which files stand in the way
// (`AMB-T-4901` measured it on all three systems), and those lines go on the screen as they came.
// Amenbo neither rewrites them nor asks beforehand whether the move is wise: the window does not
// guard the pane's agent against the reader or the reader against it (`AMB-D-906`, 3-1).
//
// **The merge is started from this same list, on a second face of it.** Which branch to bring in is
// the same question as which branch to move onto, asked of the same names — and a row that did one
// thing on a press and another on a modifier would be a row whose answer a reader cannot see before
// making it. So the face is switched first and every row on it says what it does.
//
// **A merge underway is drawn under the branch line for as long as it lasts**, which is what the
// way out of it hangs from. Where it is drawn is the answer to the one thing left open when the
// conflicts were designed (`AMB-T-4919`): the list of them is in this rail and takes no part in
// what Escape folds (`AMB-D-815`), and a merge with every conflict already settled has no list at
// all while still being a merge. The line the branch is named on is there either way.
import { Fragment, useEffect, useRef, useState } from "react";
import type { GitBranchDto } from "../bindings/bindings";
import { Icon } from "../components/Icon";
import { Menu, MenuItem } from "../components/Menu";
import { errText, t, tf } from "../core/i18n";
import { asTyped } from "../core/keys";
import {
  folderGitBranchCreate, folderGitBranches, folderGitMerge, folderGitMergeAbort, folderGitSwitch,
} from "./folder";

/** What git wrote on the way back from one of this line's doors, and whether it was a refusal. */
type Said = { text: string; refused: boolean };

/** Which question the list is asking: which branch to stand on, or which one to bring in. */
type Face = "go" | "merge";

/**
 * The branch the folder is on, and every other one it could be on.
 *
 * `on` is the branch as `folderGitStatus` answered it — the one call that says which branch is
 * checked out, so the list below is asked only for the names and the counts and never for that.
 */
export function GitBranch({ projectId, root, on, merging, onMoved }: {
  /** The project the folder is bound to. */
  projectId: number;
  /** The folder the window is on, as its path. */
  root: string;
  /** Where the checked-out branch stands, from the same read the rows came from. */
  on: GitBranchDto;
  /** Whether a merge is underway, from that same read — `MERGE_HEAD` in the repository's own
   *  directory, which is there for the whole of a merge and not only while a path is unsettled. */
  merging: boolean;
  /** A move that went through. What the folder holds has changed under every reader of it. */
  onMoved: () => void;
}) {
  // Where the list was opened, or nothing while it is shut — the button's own bottom corner, the
  // way the folder picker beside this one opens its list (`./RootPick`).
  const [at, setAt] = useState<{ x: number; y: number } | null>(null);
  const [branches, setBranches] = useState<GitBranchDto[]>([]);
  // Whether the row at the foot of the list is a box to type a name into.
  const [making, setMaking] = useState(false);
  // Which of the two questions the rows are answering.
  const [face, setFace] = useState<Face>("go");
  // What git said, or nothing. It stands until the next press, because a refusal about the working
  // tree is about a thing the reader now has to go and do something about.
  const [said, setSaid] = useState<Said | null>(null);
  // Whether the band under the line is standing over its question rather than over its button.
  const [asking, setAsking] = useState(false);
  const open = at !== null;
  const here = on.name ?? t("git.detached");

  // A merge that has ended takes its question with it. The band is gone either way — aborted, or
  // concluded from the list of conflicts below — and a question left standing would come back up
  // over the next merge already answered.
  useEffect(() => { if (!merging) setAsking(false); }, [merging]);

  // Asked when the list opens, and asked again every time it opens: which branches there are is a
  // question about the repository right now, and a list kept from last time is about whenever that
  // was. A folder that has gone answers with nothing, which is a list with only the row for making
  // one in it.
  useEffect(() => {
    if (!open) return;
    let alive = true;
    void folderGitBranches(projectId, root)
      .then((now) => { if (alive) setBranches(now); })
      .catch(() => { if (alive) setBranches([]); });
    return () => { alive = false; };
  }, [open, projectId, root]);

  const shut = () => {
    setAt(null);
    setMaking(false);
    setFace("go");
  };

  /** Move onto one of them. The branch already checked out is not a move, and asking git to go
   *  where it is would spend a call to be told nothing. */
  const go = (name: string | null) => {
    shut();
    if (name === null || name === on.name) return;
    setSaid(null);
    void folderGitSwitch(projectId, root, name)
      .then(() => onMoved())
      .catch((e: unknown) => setSaid({ text: errText(e), refused: true }));
  };

  /**
   * Bring one of them into the branch being stood on.
   *
   * **The half around this one is told to look again whichever way git answered.** A merge that
   * stopped on conflicts exits non-zero and has still written the whole of them into the working
   * tree, so a refusal here is a screen that has changed as much as a merge that went through.
   */
  const bring = (name: string | null) => {
    shut();
    if (name === null) return;
    setSaid(null);
    void folderGitMerge(projectId, root, name)
      .then((wrote) => setSaid(wrote === "" ? null : { text: wrote, refused: false }))
      .catch((e: unknown) => setSaid({ text: errText(e), refused: true }))
      .finally(() => onMoved());
  };

  /** Put the tree back where it stood before the merge — answered for, above. */
  const drop = () => {
    setAsking(false);
    setSaid(null);
    void folderGitMergeAbort(projectId, root)
      .then((wrote) => setSaid(wrote === "" ? null : { text: wrote, refused: false }))
      .catch((e: unknown) => setSaid({ text: errText(e), refused: true }))
      .finally(() => onMoved());
  };

  // Every branch but the one already being stood on: git answers `Already up to date.` to a merge of
  // a branch into itself, which is a call spent to be told nothing.
  const others = branches.filter((one) => one.name !== null && one.name !== on.name);

  return (
    <Fragment>
      <div className="gitpanel__branch">
        {/* A checkout made at a commit rather than at a branch. git writes no name there, so the
            words say what it is rather than leaving the line empty. */}
        <span className="gitpanel__branchname" title={here}>{here}</span>
        {/* Only the count there is one of. A pair of zeroes is a branch level with the one it is
            measured by, and drawing them says "nothing to do" in two numbers instead of none. */}
        {on.ahead > 0 && (
          <span className="gitpanel__count" title={tf("git.ahead", { n: on.ahead })}>
            ↑{on.ahead}
          </span>
        )}
        {on.behind > 0 && (
          <span className="gitpanel__count" title={tf("git.behind", { n: on.behind })}>
            ↓{on.behind}
          </span>
        )}
        <button
          className="gitpanel__pick"
          type="button"
          aria-haspopup="menu"
          aria-expanded={open}
          title={t("git.branches")}
          // Opened and never toggled here: the list closes itself on the pointer going down
          // anywhere outside it, and this press is one of those (`../components/Menu`).
          onClick={(e) => {
            const box = e.currentTarget.getBoundingClientRect();
            setAt({ x: box.right, y: box.bottom });
          }}
        >
          <Icon name="chevronDown" label={t("git.branches")} />
        </button>
      </div>
      {/* A merge underway, and the way out of it. It hangs from the branch line because that line is
          drawn for as long as the repository has one, where the list of conflicts below is drawn
          only while a path is still unsettled — and a merge is no less underway for having had
          every conflict settled and staged.

          **The question is put in the band itself rather than in a window over it**, the way a
          branch is named in the list rather than in a window of its own. What it is asking about is
          the band it is standing in.

          **It is asked every time.** A conflict settled by hand and never written down is in no
          commit and no reflog, so this press loses work git cannot give back — and unlike throwing
          one file's changes away, it is not a row sitting a pixel from the rows that open a file
          (`AMB-D-777`), so there is nothing here for a reader to want turned off. */}
      {merging && (
        <div className="gitpanel__merging">
          <p className="gitpanel__mergingsays">
            <Icon name="warning" />
            {t("git.merging")}
          </p>
          {asking && <p className="gitpanel__mergingwarn">{t("git.mergeAbortGone")}</p>}
          <div className="gitpanel__mergingdo">
            {asking
              ? (
                <Fragment>
                  <button className="btn btn--danger" type="button" onClick={drop}>
                    {t("git.mergeAbortGo")}
                  </button>
                  {/* What the press lands on, the way the question before throwing changes away
                      puts it on keeping them: of the two answers here, one of them is undone by
                      nothing. */}
                  <button className="btn" type="button" autoFocus onClick={() => setAsking(false)}>
                    {t("git.mergeAbortKeep")}
                  </button>
                </Fragment>
              )
              : (
                <button className="btn" type="button" onClick={() => setAsking(true)}>
                  {t("git.mergeAbort")}
                </button>
              )}
          </div>
        </div>
      )}
      {/* git's own words, in git's own layout: the three lines it writes name one file per line, and
          run together they name none of them. */}
      {said !== null && (
        <p className={`gitpanel__said${said.refused ? " gitpanel__said--refused" : ""}`}>
          {said.text}
        </p>
      )}
      {at !== null && (
        // The face is handed over because the rows are replaced whole when it changes, and the
        // reader would otherwise be left standing on a row that is no longer there.
        <Menu at={at} face={face} onClose={shut}>
          {face === "merge" && others.map((one) => (
            <MenuItem key={one.name ?? ""} onClick={() => bring(one.name)}>
              {/* Every row says what it does. The list looks like the one this face was reached
                  from, and the two do opposite things to the working tree. */}
              <span className="gitpanel__ison" />
              <span className="gitpanel__pickname">{tf("git.mergeOne", { name: one.name ?? "" })}</span>
              <span className="gitpanel__pickcount">
                {one.ahead > 0 && `↑${one.ahead}`}
                {one.behind > 0 && `↓${one.behind}`}
              </span>
            </MenuItem>
          ))}
          {face === "go" && branches.map((one) => (
            <MenuItem key={one.name ?? ""} onClick={() => go(one.name)}>
              {/* The box is drawn for every row, with or without a mark in it, so the names stand in
                  one column rather than stepping in and out as the list changes under them. */}
              <span className="gitpanel__ison">
                {one.name === on.name && <Icon name="check" label={t("git.onBranch")} />}
              </span>
              <span className="gitpanel__pickname">{one.name}</span>
              <span className="gitpanel__pickcount">
                {one.ahead > 0 && `↑${one.ahead}`}
                {one.behind > 0 && `↓${one.behind}`}
              </span>
            </MenuItem>
          ))}
          {face === "go" && (making
            ? (
              <NewBranch
                onName={async (name) => {
                  await folderGitBranchCreate(projectId, root, name);
                  shut();
                  onMoved();
                }}
                onEnd={() => setMaking(false)}
              />
            )
            : (
              <MenuItem apart onClick={() => setMaking(true)}>
                <span className="gitpanel__ison"><Icon name="plus" /></span>
                <span className="gitpanel__pickname">{t("git.newBranch")}</span>
                <span className="gitpanel__pickcount">{tf("git.fromBranch", { name: here })}</span>
              </MenuItem>
            ))}
          {/* The way to the other face. It is out of the list where there is no other branch to
              bring in, and out of it while a merge is already underway — git refuses the second
              merge in its own words, and a door that can only ever come back refused is one to
              leave out rather than to offer. It is out of it at a detached HEAD as well: what a
              merge made there belongs to is a commit no branch names, which is the one place this
              window has nothing to say about afterwards. */}
          {face === "go" && !making && !merging && on.name !== null && others.length > 0 && (
            <MenuItem apart onClick={() => { setSaid(null); setFace("merge"); }}>
              <span className="gitpanel__ison"><Icon name="foldRight" /></span>
              <span className="gitpanel__pickname">{t("git.mergeFrom")}</span>
              <span className="gitpanel__pickcount">{tf("git.mergeInto", { name: here })}</span>
            </MenuItem>
          )}
        </Menu>
      )}
    </Fragment>
  );
}

/**
 * The row at the foot of the list, while it is a box to type a name into.
 *
 * **The refusal is drawn here and nowhere else.** Which names git will have is the one thing a
 * reader cannot work out for themselves — a name already taken, a name git reads as syntax — and
 * the place to say so is where they are still typing, with what they wrote still in front of them
 * (`./FolderTree` says it the same way about a file's name).
 *
 * Escape is not caught here: it closes the list, which is what taking the row back amounts to.
 * Leaving the box, on the other hand, keeps nothing — a branch is made where the reader is standing
 * and moves them onto it, which is too much to do because a press landed somewhere else.
 */
function NewBranch({ onName, onEnd }: {
  onName: (name: string) => Promise<void>;
  /** The row is a row again — the name was refused for a reason nothing here can mend, or the box
   *  was left. */
  onEnd: () => void;
}) {
  const [name, setName] = useState("");
  const [refused, setRefused] = useState<string | null>(null);
  const [asking, setAsking] = useState(false);
  const box = useRef<HTMLInputElement | null>(null);

  // The box is the reader's the moment it is drawn — it took the place of the row they pressed, and
  // one they had to click into first would be one they typed past.
  useEffect(() => { box.current?.focus(); }, []);

  const make = () => {
    // An answer already on its way is not a second name to ask for. Nothing typed is no name at
    // all, and a refusal still standing is the name git has just said no to — typing changes it,
    // and typing is what clears the refusal.
    if (asking || name.trim() === "" || refused !== null) return;
    setAsking(true);
    onName(name.trim())
      .catch((e: unknown) => {
        setRefused(errText(e));
        setAsking(false);
        box.current?.focus();
      });
  };

  return (
    <div className="gitpanel__make">
      <input
        {...asTyped}
        ref={box}
        className="files__namebox"
        aria-label={t("git.branchName")}
        placeholder={t("git.branchName")}
        value={name}
        onChange={(e) => { setName(e.target.value); setRefused(null); }}
        onKeyDown={(e) => { if (e.key === "Enter") make(); }}
        onBlur={onEnd}
      />
      {refused !== null && <p className="gitpanel__said gitpanel__said--refused">{refused}</p>}
    </div>
  );
}
