// Which line of the tree is the widest, out of all of them and not only the ones drawn.
//
// The tree draws only the rows in view (`AMB-D-817`), so the width a browser would work out from
// the document is the width of whatever happens to be on the screen: the panel would scroll
// sideways as far as the longest name in view and no further, and the reach would change under the
// reader as they scrolled down. The width is worked out here off every line instead, and the one
// line it lands on is what the tree is laid out to (`./FolderTree`).

/**
 * The widest of the lines, or nothing where none of them has a name.
 *
 * **Measured, and not counted in letters.** `iiii` and `MMMM` are the same length and nowhere near
 * the same width, and a tree of Japanese names next to Latin ones is the case where a count picks
 * the wrong row. What is compared is how far in a line stands (`depth` × `step`) and how wide its
 * name is drawn; everything else on a row is the same width on every row.
 */
export function widest<T extends { depth: number; name: string }>(
  lines: readonly T[],
  wide: (name: string) => number,
  step: number,
): T | null {
  let best: T | null = null;
  let reach = -Infinity;
  for (const line of lines) {
    const at = line.depth * step + wide(line.name);
    if (at > reach) {
      best = line;
      reach = at;
    }
  }
  return best;
}
