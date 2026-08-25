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
  const sep = root.includes("\\") && !root.includes("/") ? "\\" : "/";
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
