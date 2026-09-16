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
import { Fragment, useEffect, useRef, useState } from "react";
import type { GitBranchDto } from "../bindings/bindings";
import { Icon } from "../components/Icon";
import { Menu, MenuItem } from "../components/Menu";
import { errText, t, tf } from "../core/i18n";
import { asTyped } from "../core/keys";
import { folderGitBranchCreate, folderGitBranches, folderGitSwitch } from "./folder";

/**
 * The branch the folder is on, and every other one it could be on.
 *
 * `on` is the branch as `folderGitStatus` answered it — the one call that says which branch is
 * checked out, so the list below is asked only for the names and the counts and never for that.
 */
export function GitBranch({ projectId, root, on, onMoved }: {
  /** The project the folder is bound to. */
  projectId: number;
  /** The folder the window is on, as its path. */
  root: string;
  /** Where the checked-out branch stands, from the same read the rows came from. */
  on: GitBranchDto;
  /** A move that went through. What the folder holds has changed under every reader of it. */
  onMoved: () => void;
}) {
  // Where the list was opened, or nothing while it is shut — the button's own bottom corner, the
  // way the folder picker beside this one opens its list (`./RootPick`).
  const [at, setAt] = useState<{ x: number; y: number } | null>(null);
  const [branches, setBranches] = useState<GitBranchDto[]>([]);
  // Whether the row at the foot of the list is a box to type a name into.
  const [making, setMaking] = useState(false);
  // What git said in refusing, or nothing. It stands until the next press, because a refusal about
  // the working tree is about a thing the reader now has to go and do something about.
  const [said, setSaid] = useState<string | null>(null);
  const open = at !== null;
  const here = on.name ?? t("git.detached");

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
  };

  /** Move onto one of them. The branch already checked out is not a move, and asking git to go
   *  where it is would spend a call to be told nothing. */
  const go = (name: string | null) => {
    shut();
    if (name === null || name === on.name) return;
    setSaid(null);
    void folderGitSwitch(projectId, root, name)
      .then(() => onMoved())
      .catch((e: unknown) => setSaid(errText(e)));
  };

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
      {/* git's own words, in git's own layout: the three lines it writes name one file per line, and
          run together they name none of them. */}
      {said !== null && <p className="gitpanel__said gitpanel__said--refused">{said}</p>}
      {at !== null && (
        <Menu at={at} onClose={shut}>
          {branches.map((one) => (
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
          {making
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
