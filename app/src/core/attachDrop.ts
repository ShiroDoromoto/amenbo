// Files dropped on an attachment well, routed to the well they landed on.
//
// The host hands the page a point and a list of paths, and nothing else (`./hostDrop`). Which well a
// drop belongs to used to be the DOM's answer for free — the browser fired `drop` on the element
// under the pointer — and this is that answer put back.
//
// **One watch for every well on the page, rather than one each.** A screen has as many wells as it
// has comments, and each watch of its own would register a listener of its own, ask the host for the
// window's inset again on every drag, and resolve the same point over again. So the wells subscribe
// here, the watch is taken up when the first arrives and let go when the last leaves, and what marks
// a well in the page is the one attribute this reads.
import { watchHostDrop } from "./hostDrop";

/** The attribute that makes an element a well, holding the key of the well it is. */
export const WELL_ATTR = "data-attach-well";

/** What one well wants to be told: whether the drag is over it, and what landed on it. */
export type AttachWell = {
  /** True while the drag is over this well and false as it leaves — never true for two at once. */
  over: (over: boolean) => void;
  /** The paths that landed on this well. */
  drop: (paths: string[]) => void;
};

/** Every well on the page now, by the key it wrote into its own attribute. */
const wells = new Map<string, AttachWell>();
/** The watch, while there is anything to watch for. A promise, because taking it up is asynchronous
 *  and the last well can leave before it has finished arriving. */
let watching: Promise<() => void> | null = null;
/** The well the drag is over, so the one before it can be told it no longer is. */
let lit: string | null = null;

/** Move the highlight, telling the well that had it and the well that has it — and only on a change,
 *  since the host repeats a point while the drag stands still. */
function light(key: string | null): void {
  if (key === lit) return;
  if (lit !== null) wells.get(lit)?.over(false);
  lit = key;
  if (key !== null) wells.get(key)?.over(true);
}

/** The key an element was marked with, or nothing where it was marked with none. */
function keyOf(el: Element | null): string | null {
  return el?.getAttribute(WELL_ATTR) ?? null;
}

/**
 * Take up one well's answer to files dropped on it, until the returned function is called.
 *
 * `key` is what the well writes into its own {@link WELL_ATTR}, and is what a drop is matched
 * against — so it has to name the one thing a drop would be attached to and nothing else.
 */
export function watchAttachWell(key: string, well: AttachWell): () => void {
  wells.set(key, well);
  if (watching === null) {
    watching = watchHostDrop({
      select: `[${WELL_ATTR}]`,
      over: ({ el }) => light(keyOf(el)),
      leave: () => light(null),
      drop: ({ el }, paths) => {
        const landed = keyOf(el);
        light(null);
        if (landed !== null) wells.get(landed)?.drop(paths);
      },
    });
  }
  return () => {
    wells.delete(key);
    if (lit === key) lit = null;
    // The last well out puts the watch down. A page with no wells on it is a page where the host's
    // event has nobody to reach, and a listener nobody reads is one more thing every drag pays for.
    if (wells.size === 0) {
      const going = watching;
      watching = null;
      void going?.then((stop) => stop());
    }
  };
}
