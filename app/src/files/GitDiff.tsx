// What the rows picked out in the rail's git half are holding, read in the column across the panes
// (`AMB-D-906`, 2-2 and 2-4).
//
// **It is read here because it wraps** (`AMB-D-835`). The rail is where a person presses — a path, a
// box, a message — and each of those fits on one line; a patch does not, and the widest place in the
// window is this column.
//
// **One face for however many rows.** A reader who gathered five changes to stage together is
// checking five things before they do, and five tabs would be five presses to read one answer — so
// the patches stand one under another, each under the name git gave it. Every other product
// measured draws a diff in one reused tab rather than in a tab per file, and one of them stacks the
// whole change the way this does (`AMB-T-4913`).
//
// **It is one call, and the order is git's** (`crate::folder_git`). git writes the patches by path,
// whatever order the rows were picked in, which is the order the rail lists them in too.
//
// **It has no layer of its own.** The history goes a layer deeper each time it is pressed, and Escape
// comes back up through them; here there is nothing under the patch, so the first press of Escape is
// already the column's own (`AMB-D-815`, `./FilesPanel`).
import { useEffect, useState } from "react";
import { t } from "../core/i18n";
import { folderGitTreeDiff, onFolderChanged } from "./folder";
import { PatchText } from "./PatchText";

/** The rows the rail has picked out, as the column is handed them: their paths, and which of git's
 *  two answers they are in — what the working tree holds that the index does not, or what the index
 *  holds that the last commit does not (`./gitPick`). */
export type DiffPick = { paths: string[][]; staged: boolean };

/** Where one file's patch begins in what git wrote. A line of a patch's body is led by a space, a
 *  plus, a minus or a backslash, so this at the front of a line is a boundary and never content. */
const HEAD = "diff --git ";

/**
 * The patches for the picked rows, one under another.
 *
 * **It asks again on the same word the rail does**: the host says the folder moved, and what a row
 * is holding is exactly what that word is about. A patch left standing while the file under it is
 * written to is the one thing this face must not show — it is read to decide whether to stage.
 */
export function GitDiff({ projectId, root, picked }: {
  projectId: number | null;
  /** The folder the window is on, as its path. */
  root: string | null;
  /** The rows picked out, or nothing where the reader has put the set down. */
  picked: DiffPick | null;
}) {
  const [patch, setPatch] = useState<string | null>(null);
  // How many times the host has said the folder moved — a save in a pane is one of those.
  const [moved, setMoved] = useState(0);
  // What is asked about, as one word: the array is rebuilt on every draw, and what the read below is
  // about is which paths are in it.
  const asked = picked?.paths.map((one) => one.join("/")).join("\n") ?? "";

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
    if (projectId === null || root === null || picked === null) { setPatch(null); return; }
    let alive = true;
    void folderGitTreeDiff(projectId, root, picked.paths, picked.staged)
      .then((now) => { if (alive) setPatch(now); })
      .catch(() => { if (alive) setPatch(""); });
    return () => { alive = false; };
    // Asked about by `asked` rather than by `picked`, which is a fresh object on every draw.
  }, [projectId, root, asked, picked?.staged, moved]);

  // Nothing gathered. The face stands — a reader who pressed for it has it — and says what to press
  // rather than drawing an empty frame they have to guess at.
  if (picked === null) return <p className="files__none">{t("git.diffNone")}</p>;
  if (patch === null) return <div className="patch" />;
  const files = cut(patch);
  if (files.length === 0) return <p className="files__none">{t("git.diffEmpty")}</p>;
  return (
    <div className="gitdiff">
      {files.map((one) => (
        <section key={one.name} className="gitdiff__file">
          {/* git's own spelling of the path, which is the repository's. A row of the rail is spelled
              from the bound folder, and two folders of one repository can hold the same name — so
              the heading says which file this is and not only what it is called. */}
          <h3 className="gitdiff__name" title={one.name}>{one.name}</h3>
          <pre className="patch"><PatchText text={one.text} /></pre>
        </section>
      ))}
    </div>
  );
}

/**
 * What git wrote, cut into one piece per file.
 *
 * git answers for several paths in one text, each file's patch led by its own `diff --git` line —
 * so the cutting is reading back what git already marked, not this side deciding where a file ends.
 */
function cut(patch: string): { name: string; text: string }[] {
  const files: { name: string; text: string }[] = [];
  let lines: string[] = [];
  const close = () => {
    if (lines.length > 0) files.push({ name: nameOf(lines), text: lines.join("\n") });
  };
  for (const line of patch.split("\n")) {
    if (line.startsWith(HEAD)) {
      close();
      lines = [];
    }
    // Everything before the first head is nothing git wrote for a file, and there is none of it in
    // a patch — but a text that is not one is not drawn as if it were.
    if (lines.length > 0 || line.startsWith(HEAD)) lines.push(line);
  }
  close();
  return files;
}

/**
 * The path one file's piece is about, as git spelled it.
 *
 * **It is read off the head of the patch rather than off the rows that were asked for**, because
 * what comes back is git's list and not the reader's: git writes nothing for a path it found no
 * change in, so the two lists are not the same length and lining them up by order would put a name
 * over another file's patch.
 *
 * The `+++` line is where the file is now and the `---` line where it was, so a file being deleted
 * is named by the second. Where there is neither — a file git reads as bytes, which it says one line
 * about — the head's own second name is it.
 */
function nameOf(lines: string[]): string {
  const now = lines.find((one) => one.startsWith("+++ b/"));
  if (now !== undefined) return now.slice("+++ b/".length);
  const was = lines.find((one) => one.startsWith("--- a/"));
  if (was !== undefined) return was.slice("--- a/".length);
  const head = lines[0] ?? "";
  const second = head.indexOf(" b/");
  return second < 0 ? head.slice(HEAD.length) : head.slice(second + " b/".length);
}
