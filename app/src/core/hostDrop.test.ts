// @vitest-environment jsdom
// What the host's drag-and-drop has to get right, none of which the browser does for us any more.
//
// The page is no longer told which element a file was dropped on — it is told a point, in units that
// differ by operating system, over an event that fires for gestures carrying no files at all. Each
// of those is a way to land a file in the wrong folder, or in a folder nobody was pointing at, and
// each is measured here (`AMB-T-3740`, `AMB-T-3749`).
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { scrollAtEdge, toPagePoint, watchHostDrop } from "./hostDrop";

/** No chrome to take off — what Windows and Linux always hand over. */
const NONE = { x: 0, y: 0 };

type DragDrop =
  | { type: "enter"; paths: string[]; position: { x: number; y: number } }
  | { type: "over"; position: { x: number; y: number } }
  | { type: "drop"; paths: string[]; position: { x: number; y: number } }
  | { type: "leave" };

const hoisted = vi.hoisted(() => ({
  /** The host's side of the subscription: what it was given, and whether it was let go of. */
  handler: null as null | ((event: { payload: DragDrop }) => void),
  unlistened: false,
  /** What the host answers when asked what the keys were at the drop. */
  effect: "default" as string,
  asked: [] as string[],
}));

vi.mock("./ipc", () => ({
  invoke: async (name: string) => {
    hoisted.asked.push(name);
    return hoisted.effect;
  },
}));

vi.mock("@tauri-apps/api/webview", () => ({
  getCurrentWebview: () => ({
    onDragDropEvent: async (take: (event: { payload: DragDrop }) => void) => {
      hoisted.handler = take;
      return () => { hoisted.unlistened = true; };
    },
  }),
}));

/**
 * Put an element under the point. jsdom lays nothing out and has no `elementFromPoint` at all, so
 * what is under the pointer is stated rather than measured — the walk up from it to the watcher's
 * own element is the part being tested, and that is real DOM.
 */
function under(el: Element | null): void {
  (document as unknown as { elementFromPoint: () => Element | null }).elementFromPoint = () => el;
}

/** Say the host said it, and let the promise the drop starts settle before anything is read. */
async function say(event: DragDrop): Promise<void> {
  hoisted.handler?.({ payload: event });
  await Promise.resolve();
  await Promise.resolve();
}

beforeEach(() => {
  hoisted.handler = null;
  hoisted.unlistened = false;
  hoisted.effect = "default";
  hoisted.asked = [];
  // Inside Tauri as far as the module is concerned; there is no host without it and it subscribes
  // to nothing.
  (window as unknown as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = {};
  under(null);
});

afterEach(() => {
  delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
  vi.restoreAllMocks();
  document.body.innerHTML = "";
});

describe("the point the host hands over", () => {
  // All three call it `PhysicalPosition` and only Windows means it. On a 150% display the rows were
  // two out before this division (`AMB-T-3749`); on macOS, where the number is already logical,
  // dividing would have halved it.
  it("is divided by the scale on Windows and left alone everywhere else", () => {
    expect(toPagePoint({ x: 300, y: 150 }, "windows", 1.5, NONE)).toEqual({ x: 200, y: 100 });
    expect(toPagePoint({ x: 200, y: 100 }, "macos", 2, NONE)).toEqual({ x: 200, y: 100 });
    expect(toPagePoint({ x: 201, y: 121 }, "other", 1, NONE)).toEqual({ x: 201, y: 121 });
  });

  /** A scale of zero is not a machine, and dividing by it would put the drop nowhere at all. */
  it("is left alone when the page reports no scale", () => {
    expect(toPagePoint({ x: 300, y: 150 }, "windows", 0, NONE)).toEqual({ x: 300, y: 150 });
  });

  // macOS measures the point from the window rather than from the page, so a drop landed a title
  // bar's worth below the row being pointed at — two rows, in the file tree (measured in the VM).
  it("takes off how far the page sits inside the window", () => {
    expect(toPagePoint({ x: 200, y: 100 }, "macos", 2, { x: 0, y: 28 })).toEqual({ x: 200, y: 72 });
  });
});

describe("hanging at the edge of the box", () => {
  /** The rows go on past the bottom of the panel, and the drag has no other way down. */
  it("moves the box towards whichever edge the drag is near, and not at all in the middle", () => {
    const box = document.createElement("div");
    box.getBoundingClientRect = () => ({ top: 100, bottom: 500 }) as DOMRect;
    box.scrollTop = 200;

    scrollAtEdge(box, 110);
    expect(box.scrollTop).toBeLessThan(200);

    box.scrollTop = 200;
    scrollAtEdge(box, 490);
    expect(box.scrollTop).toBeGreaterThan(200);

    box.scrollTop = 200;
    scrollAtEdge(box, 300);
    expect(box.scrollTop).toBe(200);
  });
});

describe("a watcher", () => {
  /** The point is resolved against the page, because the event names no element at all. */
  it("reports the element under the point, and none where there is none of its own", async () => {
    const target = document.createElement("div");
    document.body.append(target);
    const over: (Element | null)[] = [];
    under(target);

    const stop = await watchHostDrop({ select: "div", over: ({ el }) => over.push(el) });
    await say({ type: "enter", paths: ["/a.txt"], position: { x: 10, y: 10 } });
    under(null);
    await say({ type: "over", position: { x: 20, y: 20 } });

    expect(over).toEqual([target, null]);
    stop();
    expect(hoisted.unlistened).toBe(true);
  });

  // macOS repeats the same point while the drag stands still. A highlight redrawn from a move that
  // did not move is work for nothing (`AMB-T-3740`).
  it("says nothing about a move that repeats the point it was already at", async () => {
    under(null);
    let moves = 0;

    await watchHostDrop({ select: "div", over: () => { moves += 1; } });
    await say({ type: "over", position: { x: 5, y: 5 } });
    await say({ type: "over", position: { x: 5, y: 5 } });
    await say({ type: "over", position: { x: 6, y: 5 } });

    expect(moves).toBe(2);
  });

  // macOS reports a text selection dragged inside the page as a drop with no paths. Answering it
  // would light up a folder under a gesture that was never about files (`AMB-T-3740`).
  it("treats a drop carrying no paths as the drag leaving", async () => {
    const target = document.createElement("div");
    document.body.append(target);
    under(target);
    let dropped = 0;
    let left = 0;

    await watchHostDrop({
      select: "div",
      drop: () => { dropped += 1; },
      leave: () => { left += 1; },
    });
    await say({ type: "drop", paths: [], position: { x: 10, y: 10 } });

    expect(dropped).toBe(0);
    expect(left).toBe(1);
    expect(hoisted.asked).toEqual([]);
  });

  /** The paths are not this watcher's to take when they landed on none of its elements. */
  it("does not report a drop that landed on nothing of its own", async () => {
    under(null);
    let dropped = 0;
    let left = 0;

    await watchHostDrop({
      select: "div",
      drop: () => { dropped += 1; },
      leave: () => { left += 1; },
    });
    await say({ type: "drop", paths: ["/a.txt"], position: { x: 10, y: 10 } });

    expect(dropped).toBe(0);
    expect(left).toBe(1);
  });

  // The keys ride on none of the three drag events, so the host is asked — and asked at the drop,
  // which is the instant being asked about (`crate::dropped`).
  it("hands over the paths, the element they landed on, and what the host says the keys were", async () => {
    const outer = document.createElement("div");
    const inner = document.createElement("span");
    outer.append(inner);
    document.body.append(outer);
    under(inner);
    hoisted.effect = "move";
    const landed: { el: Element; paths: string[]; effect: string }[] = [];

    await watchHostDrop({
      select: "div",
      drop: ({ el }, paths, effect) => { landed.push({ el, paths, effect }); },
    });
    await say({ type: "drop", paths: ["/a.txt", "/b"], position: { x: 10, y: 10 } });

    // The element the drop is reported on is the watcher's own, found by walking up from whatever
    // happened to be under the point: a row inside a folder belongs to that folder.
    expect(landed).toEqual([{ el: outer, paths: ["/a.txt", "/b"], effect: "move" }]);
    expect(hoisted.asked).toEqual(["drop_effect"]);
  });
});
