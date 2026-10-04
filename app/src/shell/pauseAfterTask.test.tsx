// @vitest-environment jsdom
// The project's pause on the header (`AMB-D-1009`). Only the read of the runs and the write are
// stubbed.
//
// What these guard: **it asks only while there is a run to ask** — a run of this project, going,
// asked for neither pause, that takes tasks — and says why on hover when there is not; **it is
// `aria-disabled`, not `disabled`**, so the hover that says why still reaches it; and **a press asks
// each run of the project shown that would be asked, one by one, with nothing in between** — and a
// run refused does not keep the rest from being asked.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AutomationRunCardDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  runs: [] as AutomationRunCardDto[],
  /** The runs a press asked, in order. */
  asked: [] as number[],
  /** The runs that refuse when asked. */
  refusing: [] as number[],
  notices: [] as string[],
}));

vi.mock("../core/automations", () => ({
  useLiveRuns: () => hoisted.runs,
  pauseBeforeNextTask: (run: number) => {
    hoisted.asked.push(run);
    return hoisted.refusing.includes(run) ? Promise.reject(new Error(`run ${run} ended`)) : Promise.resolve();
  },
}));

vi.mock("../core/notice", () => ({
  pushNotice: (text: string) => hoisted.notices.push(text),
}));

import { t, tf } from "../core/i18n";
import { PauseAfterTask } from "./PauseAfterTask";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

function run(over: Partial<AutomationRunCardDto> = {}): AutomationRunCardDto {
  return {
    run: 7,
    project: 1,
    projectName: "amenbo",
    automation: 3,
    automationName: "nightly",
    status: "running",
    pauseRequested: false,
    pauseBeforeNextTask: false,
    pausableBeforeNextTask: true,
    waiting: false,
    stepsDone: 1,
    reportWithheld: [],
    acknowledged: false,
    ...over,
  };
}

async function render(runs: AutomationRunCardDto[]) {
  hoisted.runs = runs;
  await act(async () => {
    root.render(createElement(PauseAfterTask, { projectId: 1 }));
  });
}

const button = () => container.querySelector<HTMLButtonElement>("button")!;

async function press() {
  await act(async () => {
    button().click();
    await new Promise((r) => setTimeout(r, 0));
  });
}

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  hoisted.runs = [];
  hoisted.asked = [];
  hoisted.refusing = [];
  hoisted.notices = [];
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the project's pause", () => {
  it("asks each run of the project shown that would be asked", async () => {
    await render([
      run({ run: 7 }),
      run({ run: 8, project: 2 }),
      run({ run: 9, pausableBeforeNextTask: false }),
      run({ run: 10 }),
    ]);
    expect(button().getAttribute("aria-disabled")).toBe("false");
    expect(button().disabled).toBe(false);
    expect(button().title).toBe(t("face.pauseAfterTask"));
    expect(button().getAttribute("aria-label")).toBe(t("face.pauseAfterTask"));
    expect(button().textContent).toBe("");

    await press();
    expect(hoisted.asked).toEqual([7, 10]);
  });

  it("asks the rest when one run refuses, and says the refusal", async () => {
    hoisted.refusing = [7];
    await render([run({ run: 7 }), run({ run: 8 })]);

    await press();
    expect(hoisted.asked).toEqual([7, 8]);
    expect(hoisted.notices).toHaveLength(1);
  });

  it("counts the runs still to pause once every one is asked, and asks nobody", async () => {
    await render([
      run({ run: 7, pausableBeforeNextTask: false, pauseBeforeNextTask: true }),
      run({ run: 8, pausableBeforeNextTask: false, pauseRequested: true }),
    ]);
    expect(button().getAttribute("aria-disabled")).toBe("true");
    expect(button().disabled).toBe(false);
    expect(button().title).toBe(tf("face.pauseAfterTaskWaiting", { n: 2 }));

    await press();
    expect(hoisted.asked).toEqual([]);
  });

  it("is pressable again once a new run turns up", async () => {
    await render([
      run({ run: 7, pausableBeforeNextTask: false, pauseBeforeNextTask: true }),
      run({ run: 8 }),
    ]);
    expect(button().getAttribute("aria-disabled")).toBe("false");
    expect(button().title).toBe(t("face.pauseAfterTask"));
  });

  it("says nothing is running where no run of this project would be asked", async () => {
    await render([
      run({ run: 7, project: 2 }),
      run({ run: 8, status: "paused", pausableBeforeNextTask: false }),
    ]);
    expect(button().getAttribute("aria-disabled")).toBe("true");
    expect(button().title).toBe(t("auto.running.empty"));

    await press();
    expect(hoisted.asked).toEqual([]);
  });
});
