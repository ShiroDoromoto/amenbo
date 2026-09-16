// Which of the project's folders the window is on (`AMB-D-905`).
//
// **One folder, chosen here, and the whole window's answer to which repository it is on.** The tree
// below draws that folder and nothing else, and the git doors this rail is growing act on it — so a
// reader pressing commit knows what they are committing without reading anything but this line. Every
// bound folder drawn at once was the honest answer while the rail only read (`AMB-D-778`), and it
// stops being one the moment a press writes: a window with six trees on it and one git has no way to
// say which of the six a press was about.
//
// **Not drawn at all where there is one folder to draw.** A control that only ever says the same
// thing is a press nobody needs, and the project bound to one folder — which is all but one of them
// — is left looking exactly as it did.
//
// **The dot is what the tree stopped saying.** Six trees at once said which folders had something
// changed in them, by the colour on their rows (`AMB-D-785`), and one tree cannot. So the list says
// it instead, coarsely: something in here, or nothing. It is asked for when the list opens rather
// than watched, because watching every folder is the cost this decision was taken to stop paying.
import { Fragment, useEffect, useState } from "react";
import { t } from "../core/i18n";
import { Icon } from "../components/Icon";
import { Menu, MenuItem } from "../components/Menu";
import { folderGitStatus } from "./folder";
import type { FolderSectionRow } from "./sections";

/**
 * The folder the window is on, and the way to another one.
 *
 * `root` is the path of the folder being drawn and `sections` the folders there are to choose from,
 * both handed down: the answer belongs to the face around this rail, since the tree below and the
 * git doors beside it are two readers of one choice (`../shell/TerminalFace`).
 */
export function RootPick({ projectId, sections, root, onRoot }: {
  /** The project whose folders these are, for the git question the dots are read from. */
  projectId: number;
  /** Every folder the project is bound to, named apart from each other (`./sections`). */
  sections: FolderSectionRow[];
  /** The one being drawn. */
  root: string;
  /** Draw another one instead. */
  onRoot: (path: string) => void;
}) {
  // Where the list was opened, or nothing while it is shut. The point is the button's own bottom
  // corner rather than the pointer's: this is a list belonging to a control, and one that opened
  // under the pointer would stand away from the control it is about.
  const [at, setAt] = useState<{ x: number; y: number } | null>(null);
  // The folders git has something to say about, as their paths — nothing until the answers are in,
  // which is a list drawn without dots rather than a list held back.
  const [changed, setChanged] = useState<string[]>([]);
  const open = at !== null;
  // The folders as one string, so the question below is asked again when the list of them changes
  // and not merely when the caller hands over a fresh array. NUL because it is the one character a
  // path cannot hold.
  const asked = sections.map((one) => `${one.path}\0${one.exists}`).join("\u0001");

  // Asked when the list opens, and asked again every time it opens: it is a question about what is
  // on the disk right now, and the answer kept from last time would be about whenever that was. A
  // folder that has gone, and one that is no repository, both answer with nothing — which is a row
  // with no dot and not a failure to draw.
  useEffect(() => {
    if (!open) return;
    let alive = true;
    void Promise.all(sections.map((one) => (one.exists
      ? folderGitStatus(projectId, one.path).then((rows) => rows.length > 0).catch(() => false)
      : Promise.resolve(false))))
      .then((answers) => {
        if (alive) setChanged(sections.filter((_, i) => answers[i] === true).map((one) => one.path));
      });
    return () => { alive = false; };
  }, [open, projectId, asked]);

  // The choice is between folders, so there is nothing to choose with one of them.
  if (sections.length < 2) return null;
  const on = sections.find((one) => one.path === root);

  return (
    // No box of its own: the button is a row of the rail's head and the list is fixed to the
    // window, so a wrapper round the two would be a node with nothing to do.
    <Fragment>
      <button
        className="rootpick__on"
        type="button"
        aria-haspopup="menu"
        aria-expanded={open}
        // The whole path, because the name on the button is the short one that tells it from the
        // others and two checkouts of one repository are told apart by where they are.
        title={`${t("files.rootPick")}\n${root}`}
        // Opened and never toggled here: the list closes itself on the pointer going down
        // anywhere outside it, and this press is one of those — a button that also closed would
        // shut the list on the way down and open it again on the way up (`../components/Menu`).
        onClick={(e) => {
          const box = e.currentTarget.getBoundingClientRect();
          setAt({ x: box.left, y: box.bottom });
        }}
      >
        <span className="rootpick__name">{on?.label ?? ""}</span>
        <Icon name="chevronDown" />
      </button>
      {at !== null && (
        <Menu at={at} onClose={() => setAt(null)}>
          {sections.map((one) => (
            <MenuItem
              key={one.path}
              onClick={() => {
                setAt(null);
                onRoot(one.path);
              }}
            >
              {/* The box is drawn for every row, with or without a mark in it, so the names stand
                  in one column rather than stepping in and out as folders change under them. The
                  mark says what it is out loud, since it stands alone and the name beside it says
                  nothing about it (`AMB-D-439`). */}
              <span className="rootpick__dot">
                {changed.includes(one.path) && (
                  <Icon name="dot" label={t("files.rootChanged")} />
                )}
              </span>
              {one.label}
            </MenuItem>
          ))}
        </Menu>
      )}
    </Fragment>
  );
}
