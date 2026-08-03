// @vitest-environment jsdom
// The port a nudge reaches the screen through. Only the boundary is stubbed (core's `pending_nudges` /
// `mark_nudge_put`); the host's own branching runs for real.
//
// What these guard is the contract the port has with core, which is easy to keep and quiet to break:
// core is told a nudge was put **after** it is on screen and never before (a nudge recorded and not
// drawn is closed for someone who never saw it), a nudge with no view here is neither drawn nor
// recorded, and only the stages this surface is actually in are reported open. Plus the state every
// build is in until the first nudge is declared: nothing declared, nothing asked.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const hoisted = vi.hoisted(() => ({
  /** The ids core says are due (it has already judged them). */
  dueIds: [] as string[],
  /** The open-stage lists that reached core — one per evaluation, so the count is the evaluations. */
  asked: [] as string[][],
  /** Every id recorded as put. A view that never drew must leave this empty. */
  put: [] as string[],
}));

vi.mock("../core/mutations", () => ({
  fetchPendingNudges: (openStages: string[]) => {
    hoisted.asked.push(openStages);
    return Promise.resolve(hoisted.dueIds);
  },
  markNudgePut: (id: string) => {
    hoisted.put.push(id);
    return Promise.resolve();
  },
}));

import { NudgeHost, NUDGE_REEVALUATE_AFTER_MS, type NudgeHostProps, type NudgeView } from "./NudgeHost";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
let now = 0;

/** A view that says so on screen, and reports whether it had already been recorded when it first drew. */
function viewOf(text: string, seenPutAtFirstDraw?: { value: string[] }): NudgeView {
  let drawn = false;
  return ({ onClose }) => {
    if (!drawn) {
      drawn = true;
      if (seenPutAtFirstDraw) seenPutAtFirstDraw.value = [...hoisted.put];
    }
    return createElement("button", { onClick: onClose }, text);
  };
}

async function render(props: NudgeHostProps) {
  await act(async () => {
    root.render(createElement(NudgeHost, props));
  });
}

/** The window coming back to the front, which is the second evaluation trigger. */
async function focus() {
  await act(async () => {
    window.dispatchEvent(new Event("focus"));
  });
}

beforeEach(() => {
  hoisted.dueIds = [];
  hoisted.asked = [];
  hoisted.put = [];
  now = 1_000_000;
  vi.spyOn(Date, "now").mockImplementation(() => now);
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
  vi.restoreAllMocks();
});

describe("the nudge host", () => {
  it("asks core nothing while this build has no nudge it could put", async () => {
    hoisted.dueIds = ["autostart"]; // Even with one due: there is no view, so there is nothing to ask for.
    await render({ views: {}, stages: {} });
    expect(hoisted.asked).toEqual([]);
    expect(container.textContent).toBe("");
  });

  it("puts the nudge core says is due, and records it only once it is on screen", async () => {
    const atFirstDraw = { value: ["not drawn"] };
    hoisted.dueIds = ["autostart"];
    await render({ views: { autostart: viewOf("start me at login?", atFirstDraw) }, stages: {} });
    expect(container.textContent).toContain("start me at login?");
    // Recorded before it drew would mean core closing a nudge nobody saw.
    expect(atFirstDraw.value).toEqual([]);
    expect(hoisted.put).toEqual(["autostart"]);
  });

  it("leaves a nudge it cannot word alone — undrawn, and so unrecorded", async () => {
    hoisted.dueIds = ["a-nudge-from-a-later-build"];
    await render({ views: { autostart: viewOf("start me at login?") }, stages: {} });
    expect(container.textContent).toBe("");
    expect(hoisted.put).toEqual([]);
    expect(hoisted.asked).toEqual([[]]); // It did ask; it just had nothing to draw with the answer.
  });

  it("reports the stages it is in, and leaves out the ones it is not", async () => {
    await render({
      views: { autostart: viewOf("…") },
      stages: {
        autostart_unanswered: () => Promise.resolve(true),
        already_answered: () => Promise.resolve(false),
      },
    });
    expect(hoisted.asked).toEqual([["autostart_unanswered"]]);
  });

  it("re-evaluates on a focus return, but not before the interval is up", async () => {
    await render({ views: { autostart: viewOf("…") }, stages: {} });
    expect(hoisted.asked).toHaveLength(1); // Startup.

    await focus();
    expect(hoisted.asked).toHaveLength(1); // Back within the hour: what a nudge is judged on has not moved.

    now += NUDGE_REEVALUATE_AFTER_MS;
    await focus();
    expect(hoisted.asked).toHaveLength(2);
  });

  it("does not replace a nudge already on screen", async () => {
    hoisted.dueIds = ["autostart"];
    await render({ views: { autostart: viewOf("start me at login?") }, stages: {} });
    now += NUDGE_REEVALUATE_AFTER_MS;
    await focus();
    expect(hoisted.asked).toHaveLength(1); // Nothing is asked while one is up, so nothing is put twice.
    expect(hoisted.put).toEqual(["autostart"]);
  });

  it("takes a nudge off the screen when its view is done with it", async () => {
    hoisted.dueIds = ["autostart"];
    await render({ views: { autostart: viewOf("start me at login?") }, stages: {} });
    await act(async () => {
      container.querySelector("button")?.click();
    });
    expect(container.textContent).toBe("");
  });
});
