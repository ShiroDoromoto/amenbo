// Where a path drawn in a pane lands, and whether that is somewhere this face can open.
//
// A path clicked in a pane arrives as it was drawn (`../talk/refLinks`), so it comes in every shape
// a path is written in: absolute, relative to wherever the agent that printed it had got to, with
// `./` in front of it. What it must not do is reach out of the folder the file face is rooted at,
// and that is settled here as well as at the host's own fence — not because this side is trusted,
// but because a file this face cannot answer for should not be opened in it (`AMB-D-747`).

/**
 * The file a path names inside the project's folder, as segments from it — or nothing, where it
 * lands anywhere else.
 *
 * `cwd` is the folder the pane was in, which is what a relative path is read against; with none,
 * the root is the only other thing it could be read against.
 */
export function fileUnder(root: string, cwd: string | null, target: string): string[] | null {
  if (target === "") return null;
  const sep = separator(root);
  const parts = (path: string) => path.split(/[\\/]+/).filter((p) => p !== "" && p !== ".");
  const absolute = /^([a-zA-Z]:[\\/]|[\\/])/.test(target);
  const from = absolute ? "" : (cwd ?? root);
  const whole = absolute ? target : `${from}${sep}${target}`;

  const wanted: string[] = [];
  for (const part of parts(whole)) {
    if (part === "..") {
      if (wanted.pop() === undefined) return null;
    } else {
      wanted.push(part);
    }
  }
  const under = parts(root);
  // Inside the folder, and not merely starting with the same letters: `/work/repo-2` is not in
  // `/work/repo`, which comparing the strings would say it was.
  if (wanted.length <= under.length) return null;
  if (!under.every((part, i) => part === wanted[i])) return null;
  return wanted.slice(under.length);
}

/**
 * The whole path a row names, written the way the folder it is under is written — the other
 * direction of `fileUnder`, for handing a file to something that is not this face.
 *
 * What reads it is a shell, and a shell is on one machine: a folder spelt with backslashes is a
 * Windows path, and joining it with slashes would hand somebody a path their own machine has to
 * guess at.
 */
export function fileAt(root: string, path: string[]): string {
  const sep = separator(root);
  return [root.replace(/[\\/]+$/, ""), ...path].join(sep);
}

/** Which slash the folder is written with. A path with no slash in it at all is read as the one
 *  every machine but Windows takes, which is also the one Windows itself accepts. */
function separator(root: string): string {
  return root.includes("\\") && !root.includes("/") ? "\\" : "/";
}

/**
 * The same question asked of every folder the project is bound to: which one the path lands in, and
 * where inside it.
 *
 * **The deepest folder that accepts it wins.** One bound folder can be inside another, and then a
 * path lands in both — the answer that says something is the inner one, whose tree actually has a
 * row for it. Reading it against the outer folder would open the file in a section it is not drawn
 * in.
 *
 * **A relative path with no folder to read it against opens nothing.** With one folder that folder
 * was the only thing it could mean; with several it means as many files, and the face has no way to
 * choose between them (`AMB-D-778`). An absolute path is untouched by this — it says where it is,
 * and every folder is asked whether that is inside it.
 */
export function fileUnderAny(
  roots: string[],
  cwd: string | null,
  target: string,
): { root: string; path: string[] } | null {
  const absolute = /^([a-zA-Z]:[\\/]|[\\/])/.test(target);
  if (!absolute && cwd === null) return null;
  let found: { root: string; path: string[] } | null = null;
  for (const root of roots) {
    const path = fileUnder(root, cwd, target);
    // The deeper root is the one with fewer segments left over: both answers name the same file.
    if (path !== null && (found === null || path.length < found.path.length)) {
      found = { root, path };
    }
  }
  return found;
}
