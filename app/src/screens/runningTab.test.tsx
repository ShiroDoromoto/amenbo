// @vitest-environment jsdom
// The "running" tab (`AMB-T-5259`). Only the read and the three writes are stubbed; the rows, the
// wording of every state and which buttons a run carries all run for real.
//
// What these guard: **a row says what the run is, how far in it is and what it is on** — the four
// things a reader opens this tab to see, and the project it is in, because the tab crosses projects;
// **a pause that has been asked for reads as neither of the two states it sits between**, since a run
// told "running" would be pressed again and one told "paused" is not stopped yet; **the buttons match
// the state** — a paused run is picked up rather than paused again, and a stopped one carries none at
// all; and **the row goes to the pane while the buttons move the run**, which is the one thing a
// press inside a press would silently get wrong.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AutomationRunCardDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  runs: [] as AutomationRunCardDto[],
  /** Every write the buttons asked for, in order. */
  moved: [] as string[],
  /** What the next press should fail with, or nothing. */
  refuse: null as unknown,
}));

vi.mock("../core/automations", () => ({
  useLiveRuns: () => hoisted.runs,
  pauseRun: (run: number) => move(`pause ${run}`),
  resumeRun: (run: number) => move(`resume ${run}`),
  stopRun: (run: number) => move(`stop ${run}`),
}));

function move(what: string): Promise<void> {
  hoisted.moved.push(what);
  return hoisted.refuse === null ? Promise.resolve() : Promise.reject(hoisted.refuse);
}

import { t, tf } from "../core/i18n";
import { RunningTab } from "./RunningTab";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
let went: { project: number; run: number }[];

function run(over: Partial<AutomationRunCardDto> = {}): AutomationRunCardDto {
  return {
    run: 4,
    project: 1,
    projectName: "amenbo",
    automation: 7,
    automationName: "Morning round",
    status: "running",
    pauseRequested: false,
    stepsDone: 2,
    ...over,
  };
}

async function render(runs: AutomationRunCardDto[]) {
  hoisted.runs = runs;
  await act(async () => {
    root.render(createElement(RunningTab, {
      onGoToRun: (project: number, one: number) => { went.push({ project, run: one }); },
    }));
  });
}

const buttons = () => [...container.querySelectorAll<HTMLButtonElement>("button")];
function button(label: string): HTMLButtonElement {
  const found = buttons().find((b) => b.textContent?.includes(label));
  if (!found) throw new Error(`no button labelled ${label}`);
  return found;
}
const labels = () => buttons().map((b) => b.textContent ?? "");

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  hoisted.runs = [];
  hoisted.moved = [];
  hoisted.refuse = null;
  went = [];
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the running tab", () => {
  it("says so when nothing is running", async () => {
    await render([]);
    expect(container.textContent).toContain(t("auto.running.empty"));
  });

  it("says what each run is, how far in it is, and what it is on", async () => {
    await render([run({ stepName: "Take one", stepsDone: 2, task: { id: 51, ref: "AMB-T-51", title: "Draw the tab" } })]);
    const row = container.querySelector(".autorun__go")?.textContent ?? "";
    expect(row).toContain("Morning round");
    expect(row).toContain(tf("auto.run.step", { n: 2, step: "Take one" }));
    expect(row).toContain("AMB-T-51");
    expect(row).toContain("Draw the tab");
    // The tab crosses projects, so each row says which one it is about.
    expect(row).toContain("amenbo");
    expect(row).toContain(t("auto.run.running"));
  });

  it("reads a pause that has been asked for as neither running nor paused", async () => {
    await render([run({ pauseRequested: true })]);
    expect(container.textContent).toContain(t("auto.run.pausing"));
    expect(button(t("auto.run.pause")).disabled).toBe(true);
  });

  it("offers the move the state has, and none to a run that is over", async () => {
    await render([run({ status: "paused" })]);
    expect(labels()).toContain(t("auto.run.resume"));
    expect(labels()).not.toContain(t("auto.run.pause"));

    await render([run({ status: "queued" })]);
    expect(labels()).toContain(t("auto.run.pause"));

    await render([run({ status: "stopped", stoppedReason: "by_human" })]);
    expect(labels()).not.toContain(t("auto.run.stop"));
    expect(labels()).not.toContain(t("auto.run.pause"));
    expect(container.textContent).toContain(t("auto.run.stopped"));
    expect(container.textContent).toContain(t("auto.run.byHuman"));
  });

  it("puts every reason a run can stop for into words", async () => {
    // Every arm core writes (`amenbo_core::model::AutomationStoppedReason`), so a reason added there
    // without words on this side is caught here rather than on somebody's screen.
    for (const [reason, key] of [
      ["crashed", "auto.run.crashed"],
      ["max_times", "auto.run.maxTimes"],
      ["no_agent", "auto.run.noAgent"],
      ["by_human", "auto.run.byHuman"],
      ["no_way_on", "auto.run.noWayOn"],
    ] as const) {
      await render([run({ status: "stopped", stoppedReason: reason })]);
      expect(container.querySelector(".autorun__why")?.textContent).toBe(t(key));
    }
  });

  it("goes to the run's pane when the row is pressed, and moves the run when a button is", async () => {
    await render([run({ run: 9, project: 3 })]);
    await act(async () => { (container.querySelector(".autorun__go") as HTMLButtonElement).click(); });
    expect(went).toEqual([{ project: 3, run: 9 }]);
    expect(hoisted.moved).toEqual([]);

    await act(async () => { button(t("auto.run.stop")).click(); });
    expect(hoisted.moved).toEqual(["stop 9"]);
    // The press that moves the run is not also a press on the row it stands in.
    expect(went).toEqual([{ project: 3, run: 9 }]);
  });

  it("says out loud when a press was refused", async () => {
    hoisted.refuse = { code: "invalid_value", message_en: "run '9' is done — it is over already" };
    await render([run({ run: 9 })]);
    await act(async () => { button(t("auto.run.pause")).click(); });
    expect(container.querySelector('[role="alert"]')?.textContent).toContain("it is over already");
  });
});
