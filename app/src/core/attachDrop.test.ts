// @vitest-environment jsdom
// Which attachment well a dragged file belongs to, now that nothing answers that for us.
//
// A screen carries a well under the record and one under every comment. The browser used to fire
// `drop` on the one under the pointer; the host hands over a point instead (`AMB-D-775`), so the
// well is worked out from the element it resolves to. Getting it wrong files somebody's file under
// the wrong comment, which is the kind of mistake nobody goes looking for afterwards.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { DropAt, HostDropWatch } from "./hostDrop";

const hoisted = vi.hoisted(() => ({
  /** The watch the module took up, and how many times it has taken one up at all. */
  watch: null as HostDropWatch | null,
  taken: 0,
  stopped: 0,
}));

// The host's side, stood in for: what is under test is the routing, not how a point becomes an
// element (`./hostDrop` has its own tests for that).
vi.mock("./hostDrop", () => ({
  watchHostDrop: async (watch: HostDropWatch) => {
    hoisted.watch = watch;
    hoisted.taken += 1;
    return () => { hoisted.stopped += 1; hoisted.watch = null; };
  },
}));

import { watchAttachWell, WELL_ATTR } from "./attachDrop";

/** An element marked as the well `key`, the way the panel marks one. */
function wellEl(key: string): Element {
  const el = document.createElement("div");
  el.setAttribute(WELL_ATTR, key);
  return el;
}

const at = (el: Element | null): DropAt => ({ x: 1, y: 1, el });

/** The watch is taken up asynchronously, so a test that pushes an event waits for it to arrive. */
async function settled() {
  await new Promise((r) => setTimeout(r, 0));
  return hoisted.watch!;
}

beforeEach(() => {
  hoisted.watch = null;
  hoisted.taken = 0;
  hoisted.stopped = 0;
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("a file dropped on one of the wells", () => {
  it("goes to the well it landed on and to no other", async () => {
    const landed: Record<string, string[][]> = { body: [], comment: [] };
    const stopBody = watchAttachWell("task:1", {
      over: () => {},
      drop: (paths) => { landed.body!.push(paths); },
    });
    const stopComment = watchAttachWell("task_comment:7", {
      over: () => {},
      drop: (paths) => { landed.comment!.push(paths); },
    });
    const watch = await settled();

    watch.drop?.(at(wellEl("task_comment:7")) as DropAt & { el: Element }, ["/a.png"], "copy");
    expect(landed.comment).toEqual([["/a.png"]]);
    expect(landed.body).toEqual([]);

    stopBody();
    stopComment();
  });

  it("goes nowhere when it landed on a well nothing is listening for", async () => {
    const landed: string[][] = [];
    const stop = watchAttachWell("task:1", { over: () => {}, drop: (p) => { landed.push(p); } });
    const watch = await settled();

    // A well that has gone off the screen between the drag starting and the file landing.
    watch.drop?.(at(wellEl("task_comment:99")) as DropAt & { el: Element }, ["/a.png"], "copy");
    expect(landed).toEqual([]);
    stop();
  });
});

describe("the highlight under the drag", () => {
  it("is on one well at a time, and leaves the one it was on", async () => {
    const lit: string[] = [];
    const stopA = watchAttachWell("task:1", {
      over: (over) => lit.push(`a:${over}`),
      drop: () => {},
    });
    const stopB = watchAttachWell("task:2", {
      over: (over) => lit.push(`b:${over}`),
      drop: () => {},
    });
    const watch = await settled();

    watch.over?.(at(wellEl("task:1")));
    // The same point again, which macOS sends while the drag stands still: nothing is said twice.
    watch.over?.(at(wellEl("task:1")));
    watch.over?.(at(wellEl("task:2")));
    watch.over?.(at(null));
    expect(lit).toEqual(["a:true", "a:false", "b:true", "b:false"]);

    stopA();
    stopB();
  });

  it("goes out when the drag leaves the window", async () => {
    const lit: string[] = [];
    const stop = watchAttachWell("task:1", { over: (o) => lit.push(`${o}`), drop: () => {} });
    const watch = await settled();

    watch.over?.(at(wellEl("task:1")));
    watch.leave?.();
    expect(lit).toEqual(["true", "false"]);
    stop();
  });
});

describe("the watch itself", () => {
  /** A screen has as many wells as it has comments, and one listener answers for all of them. */
  it("is one however many wells there are", async () => {
    const stops = ["task:1", "task_comment:2", "task_comment:3"].map((key) =>
      watchAttachWell(key, { over: () => {}, drop: () => {} }));
    await settled();
    expect(hoisted.taken).toBe(1);

    for (const stop of stops) stop();
    await settled();
    // And it is put down when the last well goes: a listener nobody reads is one more thing every
    // drag on the machine pays for.
    expect(hoisted.stopped).toBe(1);
  });

  it("is taken up again when a well comes back after the last one left", async () => {
    watchAttachWell("task:1", { over: () => {}, drop: () => {} })();
    await settled();
    expect(hoisted.stopped).toBe(1);

    const stop = watchAttachWell("task:2", { over: () => {}, drop: () => {} });
    await settled();
    expect(hoisted.taken).toBe(2);
    stop();
  });
});
