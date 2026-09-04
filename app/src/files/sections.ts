// Which folders the file face draws, in what order, and under what name.
//
// A project is bound to as many folders as somebody bound it to, and the face draws all of them
// (`AMB-D-778`). The order is the order the paths sort in — which is how the store holds them, and
// not a ranking: no folder is the project's main one, and building a way to say which would be
// deciding a question that was deliberately left open (`AMB-D-531`).

/** One folder as the face draws it: where it is, what to call it, and whether it is still there. */
export type FolderSectionRow = {
  path: string;
  /** What the heading says. Short where short is enough to tell them apart, longer where it is not. */
  label: string;
  exists: boolean;
};

/** The segments of a path, either separator, with nothing empty in between. */
function parts(path: string): string[] {
  return path.split(/[\\/]+/).filter((one) => one !== "");
}

/** The last `depth` segments of a path, joined the way they were written. */
function tail(segments: string[], depth: number): string {
  return segments.slice(Math.max(0, segments.length - depth)).join("/");
}

/**
 * The sections to draw for a project's bound folders, sorted by path and named apart from each
 * other.
 *
 * **A folder's own name is not always enough to name it.** Two repositories with an `app` in each
 * are drawn as `app` and `app`, and a reader looking at two identical headings has no way to tell
 * which tree is which. So a name that collides grows leftwards, one segment at a time, until it
 * does not — and only the ones that collide grow, because a path spelled out in full says less at a
 * glance than the one word that was enough.
 */
export function sectionsOf(folders: { path: string; exists: boolean }[]): FolderSectionRow[] {
  const rows = [...folders].sort((a, b) => a.path.localeCompare(b.path));
  const segments = rows.map((one) => parts(one.path));
  const depth = rows.map(() => 1);

  // Grow until no two names are the same, or until growing has stopped changing anything: two
  // bindings that resolve to the same path have the same name however far back it is taken.
  for (;;) {
    const named = rows.map((_, i) => tail(segments[i]!, depth[i]!));
    const clashes = named.filter((name, i) => named.some((other, j) => j !== i && other === name));
    if (clashes.length === 0) break;
    let grew = false;
    for (let i = 0; i < rows.length; i += 1) {
      if (clashes.includes(named[i]!) && depth[i]! < segments[i]!.length) {
        depth[i] += 1;
        grew = true;
      }
    }
    if (!grew) break;
  }

  return rows.map((one, i) => ({
    path: one.path,
    // A folder that is the whole of a drive or a filesystem has no segments to name it by, so it is
    // named by the only spelling it has.
    label: tail(segments[i]!, depth[i]!) || one.path,
    exists: one.exists,
  }));
}
