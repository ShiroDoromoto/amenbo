// The rows a narrowed tree draws: what the filter found, and every folder on the way down to one.
//
// **The finding is the host's and the shape is ours.** `folder_names` walks the whole bound folder
// and comes back with the paths that hold what was typed, in no particular arrangement — a reader
// filtering a tree is doing it instead of opening folders to look, so it looks past what is open
// (`crate::folder::folder_names`). What a tree draws is a list of rows in the order they are read
// down, each knowing how far in it stands and how many names stand beside it, and that is what is
// built here.
//
// **Every folder on the way is drawn, and drawn open.** A match six folders down with none of them
// on the screen would be a row with nothing to say where it is; with them it reads as the tree it
// is part of. They are drawn open because they *are* open — there is nothing under them here but
// the way to a match.

import type { FolderNameDto } from "../bindings/bindings";
import type { Row } from "./FolderTree";

/** One drawn name: what the filter found, or a folder standing on the way to one. */
type Node = {
  path: string[];
  isDir: boolean;
  /** Whether the repository ignores it. A folder on the way is asked of what is under it. */
  ignored: boolean;
};

/** The rows to draw for what the filter found, in the order a reader goes down them. */
export function linesOfNames(found: FolderNameDto[]): Row[] {
  const nodes = new Map<string, Node>();
  for (const one of found) {
    nodes.set(one.path.join("/"), { path: one.path, isDir: one.isDir, ignored: one.ignored });
    // Every folder above it, put in once however many matches stand under it.
    for (let n = 1; n < one.path.length; n += 1) {
      const at = one.path.slice(0, n);
      const key = at.join("/");
      if (!nodes.has(key)) nodes.set(key, { path: at, isDir: true, ignored: true });
    }
  }

  const under = new Map<string, Node[]>();
  for (const node of nodes.values()) {
    const parent = node.path.slice(0, -1).join("/");
    under.set(parent, [...(under.get(parent) ?? []), node]);
  }
  // The same order a level comes back in: folders first, then names, each run read the way a person
  // reads them (`crate::folder::folder_entries`).
  for (const list of under.values()) {
    list.sort((a, b) =>
      Number(b.isDir) - Number(a.isDir) || nameOf(a).toLowerCase().localeCompare(nameOf(b).toLowerCase()));
  }

  // **A folder on the way is ignored where everything drawn under it is.** The host answers for the
  // names it found and not for the folders above them, and a folder the repository ignores drawn
  // unmarked would be the one thing the tree is careful to say (`AMB-D-786`) going unsaid. Asked
  // bottom-up, because what a folder is depends on what is under it.
  const marked = (key: string): boolean => {
    const kids = under.get(key);
    const node = nodes.get(key);
    if (kids === undefined || kids.length === 0) return node?.ignored ?? false;
    return kids.every((kid) => marked(kid.path.join("/")));
  };

  const out: Row[] = [];
  const walk = (parent: string, depth: number) => {
    const list = under.get(parent) ?? [];
    list.forEach((node, i) => {
      const key = node.path.join("/");
      const kids = under.get(key) ?? [];
      out.push({
        key,
        path: node.path,
        name: nameOf(node),
        isDir: node.isDir,
        ignored: marked(key),
        in: parent,
        depth,
        setsize: list.length,
        posinset: i + 1,
        unfolded: kids.length > 0,
      });
      if (kids.length > 0) walk(key, depth + 1);
    });
  };
  walk("", 0);
  return out;
}

const nameOf = (node: Node): string => node.path[node.path.length - 1] ?? "";
