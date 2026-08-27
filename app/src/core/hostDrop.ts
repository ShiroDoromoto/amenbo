// Files dragged in from the desktop, as the host hands them over (`AMB-D-775`).
//
// **The page no longer receives them.** `dragDropEnabled` is on for both windows
// (`app/src-tauri/tauri.conf.json`, `crate::windows`), so the OS drag lands on the application and
// the page is told about it through one Tauri event. What arrives is the thing the page could never
// have: the **paths** of what was dropped. Reading a whole file into memory to hand it over is no
// longer the only way, and Linux — where the webview is told nothing at all about an outside drag —
// is on the same road as the other two (`AMB-T-3740`).
//
// What the host's event does *not* carry is everything the DOM used to do for free, and this module
// is the whole of putting it back:
//
// | | why it is here |
// |---|---|
// | which element it is over | the event carries a point, not a target — so the point is resolved against the page |
// | the units of that point | named `PhysicalPosition` on all three, but only Windows means it (`AMB-T-3749`) |
// | what that point is measured from | the webview on Windows and Linux — the **window** on macOS, title bar included (`AMB-T-3793`) |
// | hover, and scrolling at the edge | `dragover` never fires, so the highlight has to be driven from `over` |
// | copy or move | the modifier keys do not ride the event on any of the three; the OS is asked at the moment of the drop (`crate::dropped`) |
// | drags that carry no files | macOS reports a text selection being dragged as a drop with no paths |
//
// A watcher names the elements it can receive a drop on with a selector, and is told when the
// pointer is over one, when it leaves, and what landed. Several watchers can be up at once — each
// resolves the same point against its own selector — which is what lets one panel answer for its
// folders while another answers for its attachment wells.
import type { DropEffectDto } from "../bindings/bindings";
import { invoke } from "./ipc";
import { hostOs, type HostOs } from "./platform";
import { inTauri } from "./snapshot";

/**
 * What the reader asked for by what they were holding when they let go (`crate::dropped`).
 *
 * `default` is neither — the caller's own idea of what a plain drop means, which is not this
 * module's to have.
 */
export type DropEffect = DropEffectDto;

/** A point in the page, in CSS pixels, and whatever of the watcher's elements is under it. */
export type DropAt = { x: number; y: number; el: Element | null };

/** One face's answer to drops: what it can receive them on, and what it does about them. */
export type HostDropWatch = {
  /** What marks an element as somewhere a drop can land, as a CSS selector. */
  select: string;
  /** The drag moved over the page. `el` is null where it is over none of this watcher's elements. */
  over?: (at: DropAt) => void;
  /** The drag left the window, or turned out to carry no files. */
  leave?: () => void;
  /**
   * Files landed on one of this watcher's elements. A drop that landed on none of them is not
   * reported at all — the paths are not this watcher's to take.
   */
  drop?: (at: DropAt & { el: Element }, paths: string[], effect: DropEffect) => void;
  /**
   * The box to scroll while the drag hangs near its top or bottom edge, if the face has one. Asked
   * for on each move rather than held, because the box the panel scrolls in is remounted as the
   * panel changes what it is showing.
   */
  scroller?: () => Element | null;
};

/** How near an edge the drag has to hang before the box under it starts to move, in CSS pixels. */
const EDGE = 40;
/** How far one move scrolls it. `over` arrives 15–30 times a second, which is what sets the pace. */
const EDGE_STEP = 12;

/**
 * Turn the host's point into this page's coordinates.
 *
 * **All three operating systems call it `PhysicalPosition` and only one of them means it**
 * (`AMB-T-3740`): macOS hands over logical points and Linux logical pixels, both of which are
 * already CSS pixels, while Windows hands over device pixels. So Windows — and only Windows — is
 * divided by the scale the page is drawn at. A 150% display was two rows out before this
 * (`AMB-T-3749`).
 *
 * The operating system is whatever the webview admits to being on (`./platform`), and an `other` it
 * cannot place is left alone: two of the three need no division, and dividing a machine this side
 * cannot name would break the one road Linux has.
 *
 * **And it is measured from a different corner on macOS**, which {@link webviewInset} answers for:
 * wry gives the point against the window on that OS and against the webview on the other two, so
 * what is taken off here is nothing at all except where the page hangs below a title bar.
 */
export function toPagePoint(
  position: { x: number; y: number },
  os: HostOs,
  scale: number,
  inset: { x: number; y: number },
): { x: number; y: number } {
  const divisor = os === "windows" && scale > 0 ? scale : 1;
  return { x: position.x / divisor - inset.x, y: position.y / divisor - inset.y };
}

/**
 * How far the page sits inside the window, in CSS pixels — nothing on Windows and Linux, the title
 * bar on macOS.
 *
 * **Only macOS needs taking off**, and the difference is wry's, not the operating system's. There
 * the point is flipped out of `NSDraggingInfo.draggingLocation`, which is the window's; on Windows
 * the drop target is registered on the webview's own child window and the point arrives through
 * `ScreenToClient` against that, and on Linux it arrives on the webview widget's own `drag-motion`.
 * Both of those are already the page's own corner.
 *
 * The bar is **measured, not named**: the window the host says it has, against the viewport the page
 * says it has. That difference is the bar, whatever height this version of macOS draws it at — 32
 * CSS pixels on the machine this was measured on, which is neither of the two numbers a search would
 * have offered. Nothing else Tauri exposes carries it: the window's outer and inner sizes come back
 * equal, its outer and inner positions come back equal, and the webview's own position comes back as
 * the origin (all read out of the running app, `AMB-T-3793`).
 *
 * **Only macOS is asked**, and not because the other two have no title bar — because there the point
 * already arrives against the page, so there is nothing to take off. Asking them anyway would put a
 * rounding error where a zero belongs: Windows draws at fractional scales, and a viewport that came
 * back one pixel short of the client area would move every drop by a pixel that is not there.
 *
 * Anything unanswerable is no inset, which is the reading the other two get anyway.
 */
async function webviewInset(os: HostOs): Promise<{ x: number; y: number }> {
  if (os !== "macos") return { x: 0, y: 0 };
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    const inner = await getCurrentWindow().innerSize();
    const scale = window.devicePixelRatio > 0 ? window.devicePixelRatio : 1;
    return {
      x: inner.width / scale - window.innerWidth,
      y: inner.height / scale - window.innerHeight,
    };
  } catch {
    return { x: 0, y: 0 };
  }
}

/**
 * Scroll `box` if `y` is within {@link EDGE} of its top or bottom.
 *
 * The rows a drop can land on go on past the bottom of the panel, and the drag cannot use the wheel
 * to reach them on every machine — so hanging at the edge is the way down. Nothing here is animated:
 * the `over` events are the clock, and a step each is a steady rate on all three (`AMB-T-3740`).
 */
export function scrollAtEdge(box: Element, y: number): void {
  const rect = box.getBoundingClientRect();
  if (y - rect.top < EDGE) box.scrollTop -= EDGE_STEP;
  else if (rect.bottom - y < EDGE) box.scrollTop += EDGE_STEP;
}

/**
 * Take up one face's answer to files dropped on the window, until the returned function is called.
 *
 * Outside Tauri there is no host to hear from and nothing is subscribed: a browser `npm run dev`
 * draws the same panel and simply never sees a drop.
 */
export async function watchHostDrop(watch: HostDropWatch): Promise<() => void> {
  if (!inTauri()) return () => {};
  const { getCurrentWebview } = await import("@tauri-apps/api/webview");
  // macOS repeats the same point twice while the drag stands still, so the last one is remembered
  // and a repeat is dropped: a highlight redrawn from an identical move is work nobody asked for
  // (`AMB-T-3740`).
  let last: string | null = null;
  const os = hostOs();
  // Read once here and again as each drag arrives. Moving the window does not change it — a title bar
  // is the same height wherever the window stands — but going full screen takes the bar away, and a
  // page still taking one off would land every drop a row or two below the one being pointed at.
  let inset = await webviewInset(os);
  const unlisten = await getCurrentWebview().onDragDropEvent(({ payload }) => {
    if (payload.type === "leave") {
      last = null;
      watch.leave?.();
      return;
    }
    // A drag carrying nothing is not a drag this side has anything to do with. macOS sends one for
    // every text selection dragged inside the page, and answering it would light up rows under a
    // gesture that was never about files (`AMB-T-3740`).
    if (payload.type !== "over" && payload.paths.length === 0) {
      last = null;
      watch.leave?.();
      return;
    }
    if (payload.type === "enter") void webviewInset(os).then((fresh) => { inset = fresh; });
    const { x, y } = toPagePoint(payload.position, os, window.devicePixelRatio, inset);
    const el = document.elementFromPoint(x, y)?.closest(watch.select) ?? null;
    if (payload.type === "drop") {
      last = null;
      if (el === null) {
        watch.leave?.();
        return;
      }
      const paths = payload.paths;
      // Asked for here rather than carried by the event, and asked for now rather than later: what
      // is being read is the keyboard as it is at this instant (`crate::dropped`).
      void dropEffect().then((effect) => watch.drop?.({ x, y, el }, paths, effect));
      return;
    }
    const here = `${x},${y}`;
    if (here === last) return;
    last = here;
    const box = watch.scroller?.();
    if (box) scrollAtEdge(box, y);
    watch.over?.({ x, y, el });
  });
  return unlisten;
}

/**
 * What the operating system says the reader was holding as they let go.
 *
 * A failure is `default` rather than an error: the keys are a refinement of a drop that already
 * happened, and a face that could not read them still has a drop to answer.
 */
async function dropEffect(): Promise<DropEffect> {
  try {
    return await invoke<DropEffect>("drop_effect", {});
  } catch {
    return "default";
  }
}
