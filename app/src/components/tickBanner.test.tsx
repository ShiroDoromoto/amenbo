// @vitest-environment jsdom
// The offer to be woken once an hour. Only the boundary is stubbed (core's judgement and the three
// writes); the banner's own branching and wording run for real.
//
// What is pinned here is the difference between the three buttons, since it is the whole of `AMB-D-663`
// and nothing on screen can show it: they look alike and differ only in what they leave on disk.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { t } from "../core/i18n";

const hoisted = vi.hoisted(() => ({
  /** What core says about putting the question here today. */
  shows: true,
  /** How many times it was asked — the evidence it is read once. */
  asked: 0,
  /** The answers recorded, in order (`true` is a yes). */
  answered: [] as boolean[],
  /** How many times the banner was put off for the day. */
  deferred: 0,
  /** What a write should fail with, or null to let it land. */
  writeFails: null as string | null,
}));

vi.mock("../core/snapshot", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/snapshot")>();
  return { ...orig, inTauri: () => true, subscribe: () => () => {} };
});
vi.mock("../core/mutations", () => ({
  fetchTickBanner: () => {
    hoisted.asked += 1;
    return Promise.resolve(hoisted.shows);
  },
  answerTick: (yes: boolean) => {
    if (hoisted.writeFails) return Promise.reject(new Error(hoisted.writeFails));
    hoisted.answered.push(yes);
    return Promise.resolve();
  },
  deferTickBanner: () => {
    if (hoisted.writeFails) return Promise.reject(new Error(hoisted.writeFails));
    hoisted.deferred += 1;
    return Promise.resolve();
  },
}));

import { TickBanner } from "./TickBanner";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

async function render() {
  await act(async () => {
    root.render(createElement(TickBanner));
  });
}

const banner = () => container.querySelector(".tickbanner");

function button(label: string): HTMLButtonElement {
  const found = [...container.querySelectorAll("button")].find((b) => b.textContent?.includes(label));
  if (!found) throw new Error(`no button labelled ${label}`);
  return found as HTMLButtonElement;
}

async function press(label: string) {
  await act(async () => {
    button(label).click();
  });
}

beforeEach(() => {
  hoisted.shows = true;
  hoisted.asked = 0;
  hoisted.answered = [];
  hoisted.deferred = 0;
  hoisted.writeFails = null;
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the tick's banner", () => {
  it("is up only where core says there is a question to put, and asks once", async () => {
    await render();
    expect(banner()).not.toBeNull();
    expect(hoisted.asked).toBe(1);
  });

  it("says nothing where core says there is nothing to ask", async () => {
    hoisted.shows = false;
    await render();
    expect(banner()).toBeNull();
  });

  it("records a yes when the reader asks for the checking to start", async () => {
    await render();
    await press(t("tickBanner.start"));

    expect(hoisted.answered).toEqual([true]);
    expect(hoisted.deferred).toBe(0);
    expect(banner(), "the question is answered, so it is over").toBeNull();
  });

  it("records a no when the reader asks not to be shown it again", async () => {
    await render();
    await press(t("tickBanner.never"));

    expect(hoisted.answered).toEqual([false]);
    expect(banner()).toBeNull();
  });

  // The one button of the three that answers nothing. It has to reach disk all the same: this band
  // spans the app and outlives any one screen, so a "later" held in the webview would be a button that
  // changes nothing past the next launch.
  it("puts the day on record for a later, and answers neither way", async () => {
    await render();
    await press(t("tickBanner.later"));

    expect(hoisted.deferred).toBe(1);
    expect(hoisted.answered, "later is not an answer").toEqual([]);
    expect(banner(), "and it is off the screen for today").toBeNull();
  });

  // A registration the scheduler refused must not leave a config claiming a timer — core's order is what
  // holds that, and the banner's part is to stay up and say so rather than report an answer nobody has.
  it("stays up with the reason when the write did not land", async () => {
    hoisted.writeFails = "the scheduler would not take it";
    await render();
    await press(t("tickBanner.start"));

    expect(banner(), "nothing was recorded, so nothing is over").not.toBeNull();
    expect(container.querySelector(".errortext")?.textContent).toContain("the scheduler would not take it");
    expect(hoisted.answered).toEqual([]);
  });
});
