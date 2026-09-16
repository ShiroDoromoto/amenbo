// Where a file's name ends and what it is written in begins.
//
// **Two faces cut a name at the same place for different reasons**, so the cut is made once. The
// tree selects the stem when a rename opens, because that is the part a rename is usually about
// (`./FolderTree`); the rail's git half keeps the end of the name on the screen when the row is too
// narrow for the whole of it, because a run of paths that differ only past the cut is a list of rows
// nobody can tell apart (`./GitPanel`).

/**
 * Where a name's stem ends — the last dot, or the whole length where there is none to cut at.
 *
 * The last dot and not the first: `archive.tar.gz` is renamed by changing `archive.tar`, and a name
 * cut at the first dot would hand back a stem nobody meant. A dot at the very front is not one of
 * these — `.gitignore` is a name, not an extension on an empty stem — and a name with no dot in it
 * is all stem, so both answer with the whole length (the convention `./grammars` reads names by).
 */
export function stemEnd(name: string): number {
  const dot = name.lastIndexOf(".");
  return dot <= 0 ? name.length : dot;
}

/**
 * How much of a name's end is kept out of the way of the cut. Eight characters, which is the
 * shortest that tells apart the names this was found on: a run of drafts of one document differ in
 * a word before the extension, so keeping the extension alone would draw three rows the same.
 */
const TAIL = 8;

/**
 * Where a name is cut when its row is too narrow for the whole of it — the extension and a little
 * of what comes before it, so the reader keeps the end.
 *
 * **Never inside the extension.** A name whose extension is longer than the tail is cut at the last
 * dot and nowhere else: what is written after that dot is the one part of a name that says what the
 * file is, and half of it says nothing.
 */
export function tailFrom(name: string): number {
  return Math.min(stemEnd(name), Math.max(0, name.length - TAIL));
}
