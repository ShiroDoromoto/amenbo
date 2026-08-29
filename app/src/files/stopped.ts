// What to say about a carry that stopped part-way — the one sentence both doors into a folder end at.
//
// A carry is not one act: it takes the rows it was given in order and stops on the first that will
// not go, so what a reader has to be told is where it got to and why it got no further. Both the
// drop onto a chosen folder (`crate::folder_write::folder_import`) and the hand-over into the
// project's own inbox (`… ::folder_inbox`) answer in that shape, and a reader who dropped on a row
// and one who handed a pane a file are owed the same sentence for the same refusal.
import type { FolderStoppedDto } from "../bindings/bindings";
import { formatNumber, t, tf } from "../core/i18n";

/** The shape both carries answer in: what got there, and the one it stopped on. */
export type Carry = { arrived: readonly unknown[]; stopped: FolderStoppedDto | null };

/**
 * Why the carry stopped, in the reader's language where the answer is Amenbo's own.
 *
 * The host names its own three refusals and sends the sentence with them (`crate::dto`); everything
 * else it stops on is the machine's, and the machine's words go through as they came. Which is the
 * whole of the split: what Amenbo decided means the same thing every time and can be said in any
 * language, and what a filesystem said is one sentence in whatever language it was built with.
 */
export function whyStopped(stopped: FolderStoppedDto): string {
  switch (stopped.code) {
    case "taken":
      return t("files.stoppedTaken");
    case "inside":
      return t("files.stoppedInside");
    case "nameless":
      return t("files.stoppedNameless");
    case "nobin":
      return t("files.stoppedNoBin");
    case "emptied":
      return t("files.stoppedEmptied");
    default:
      return stopped.why;
  }
}

/**
 * What to say about a carry that stopped, or nothing where the whole of it arrived.
 *
 * The count is in the sentence because a carry is not one act: stopping on the second of three
 * leaves one file in the folder, and a line that named only the failure would have the reader
 * looking for the one that did arrive.
 */
export function stoppedLine(carried: Carry): string | null {
  const stopped = carried.stopped;
  if (stopped === null) return null;
  const about = { name: stopped.name, why: whyStopped(stopped) };
  return carried.arrived.length === 0
    ? tf("files.dropStopped", about)
    : tf("files.dropPartly", { ...about, count: formatNumber(carried.arrived.length) });
}
