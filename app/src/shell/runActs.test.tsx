// @vitest-environment jsdom
// A run's moves as one mark and the list behind it (`./RunActs`).
//
// What these guard: **the mark is the state the run is in** and pressing it moves nothing; **the list
// is the moves that state has** — a run going paused at the end of the action or of the task, or
// force-cancelled, a held one resumed or cancelled; **the pause once the task is over is offered only
// where the run can take it, and once asked for says it is waiting** while the pause at the end of the
// action stays; and **a pick is handed to the caller and closes the list**.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const hoisted = vi.hoisted(() => ({ moved: [] as string[] }));

vi.mock("../core/automations", () => ({
  pauseRun: async (run: number) => { hoisted.moved.push(`pause ${run}`); },
  pauseBeforeNextTask: async (run: number) => { hoisted.moved.push(`pause-before-next-task ${run}`); },
  forceCancelRun: async (run: number) => { hoisted.moved.push(`force-cancel ${run}`); return true; },
  resumeRun: async (run: number) => { hoisted.moved.push(`resume ${run}`); },
  cancelRun: async (run: number) => { hoisted.moved.push(`cancel ${run}`); },
}));

import { t } from "../core/i18n";
import { RunActs, type RunMoves } from "./RunActs";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
let busy: boolean;

function moves(over: Partial<RunMoves> = {}): RunMoves {
  return {
    run: 5,
    status: "running",
    pauseRequested: false,
    pauseBeforeNextTask: false,
    pausableBeforeNextTask: true,
    ...over,
  };
}

async function render(run: RunMoves) {
  await act(async () => {
    root.render(createElement(RunActs, {
      run,
      busy,
      onPress: (move: () => Promise<unknown>) => void move(),
    }));
  });
}
const mark = () => container.querySelector<HTMLButtonElement>(".runacts__mark")!;
const list = () => document.body.querySelector(".runacts__list");
const items = () => [...document.body.querySelectorAll<HTMLButtonElement>(".runacts__one")];
async function open() {
  await act(async () => { mark().click(); });
}

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  hoisted.moved = [];
  busy = false;
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("a run's moves", () => {
  it("draws the state as the mark, and moves nothing when it is pressed", async () => {
    await render(moves());
    expect(mark().querySelector('[data-icon="pause"]')).not.toBeNull();
    expect(list()).toBeNull();
    await open();
    expect(list()).not.toBeNull();
    expect(hoisted.moved).toEqual([]);

    await render(moves({ status: "paused" }));
    expect(mark().querySelector('[data-icon="play"]')).not.toBeNull();
  });

  it("opens the list as the pointer comes onto the mark", async () => {
    await render(moves());
    await act(async () => { mark().dispatchEvent(new MouseEvent("mouseover", { bubbles: true })); });
    expect(list()).not.toBeNull();
  });

  it("offers a run going the two pauses and force cancel, the last in the stop colour", async () => {
    await render(moves());
    await open();
    expect(items().map((one) => one.textContent)).toEqual([
      t("auto.run.pauseAfterAction"),
      t("auto.run.pauseAfterTask"),
      t("auto.run.forceCancel"),
    ]);
    expect(items().map((one) => one.disabled)).toEqual([false, false, false]);
    expect(items()[2]!.classList.contains("runacts__one--stop")).toBe(true);
  });

  it("offers a held run resume and cancel", async () => {
    await render(moves({ status: "paused" }));
    await open();
    expect(items().map((one) => one.textContent)).toEqual([t("auto.run.resume"), t("auto.run.cancel")]);
  });

  it("offers no pause once the task is over to a run that takes no task", async () => {
    await render(moves({ pausableBeforeNextTask: false }));
    await open();
    expect(items()[1]!.disabled).toBe(true);
  });

  it("says the pause once the task is over is waiting, and keeps the pause at the action's end", async () => {
    await render(moves({ pauseBeforeNextTask: true, pausableBeforeNextTask: false }));
    await open();
    expect(items()[0]!.disabled).toBe(false);
    expect(items()[1]!.disabled).toBe(true);
    expect(items()[1]!.textContent).toBe(t("auto.run.pauseAfterTaskWaiting"));
  });

  it("asks for neither pause again while one at the action's end is on its way", async () => {
    await render(moves({ pauseRequested: true, pausableBeforeNextTask: false }));
    await open();
    expect(items().map((one) => one.disabled)).toEqual([true, true, false]);
  });

  it("hands the pick on, and closes", async () => {
    await render(moves());
    await open();
    await act(async () => { items()[1]!.click(); });
    expect(hoisted.moved).toEqual(["pause-before-next-task 5"]);
    expect(list()).toBeNull();

    await open();
    await act(async () => { items()[2]!.click(); });
    expect(hoisted.moved).toEqual(["pause-before-next-task 5", "force-cancel 5"]);

    await render(moves({ status: "paused" }));
    await open();
    await act(async () => { items()[0]!.click(); });
    await open();
    await act(async () => { items()[1]!.click(); });
    expect(hoisted.moved.slice(2)).toEqual(["resume 5", "cancel 5"]);
  });

  it("takes no move while one is still on its way", async () => {
    busy = true;
    await render(moves());
    await open();
    expect(items().every((one) => one.disabled)).toBe(true);
  });

  it("closes on Escape", async () => {
    await render(moves());
    await open();
    await act(async () => { window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" })); });
    expect(list()).toBeNull();
  });
});
