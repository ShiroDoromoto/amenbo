// @vitest-environment jsdom
// The "running" tab (`AMB-T-5259`). Only the read and the three writes are stubbed; the rows, the
// wording of every state and which buttons a run carries all run for real.
//
// What these guard: **a row says what the run is, how far in it is and what it is on** — the four
// things a reader opens this tab to see, and the project it is in where the tab crosses projects;
// **how far in it is names the action the step was opened from**, two spots standing on the same
// action running steps of the same names (`AMB-D-949`), **and falls back to the step alone where that
// spot has been taken off the picture**;
// **a run with no task says it has not taken one yet, and only while it still can**;
// **a pause that has been asked for reads as neither of the two states it sits between**, since a run
// told "running" would be pressed again and one told "paused" is not stopped yet; **the buttons match
// the state** — a paused run is picked up rather than paused again, and a failure carries only the
// press that acknowledges it; and **the row goes to the pane while the buttons move the run**, which
// is the one thing a press inside a press would silently get wrong.
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
  acknowledgeRun: (run: number) => move(`acknowledge ${run}`),
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

async function render(runs: AutomationRunCardDto[], projectId: number | null = null) {
  hoisted.runs = runs;
  await act(async () => {
    root.render(createElement(RunningTab, {
      projectId,
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
    await render([run({ stepName: "Take one", actionName: "Groundwork", stepsDone: 2, task: { id: 51, ref: "AMB-T-51", title: "Draw the tab", seq: 1 } })]);
    const row = container.querySelector(".autorun__go")?.textContent ?? "";
    expect(row).toContain("Morning round");
    expect(row).toContain(tf("auto.run.step", {
      n: 2,
      step: tf("auto.run.inAction", { action: "Groundwork", step: "Take one" }),
    }));
    expect(row).toContain("AMB-T-51");
    expect(row).toContain("Draw the tab");
    // The tab crosses projects, so each row says which one it is about.
    expect(row).toContain("amenbo");
    expect(row).toContain(t("auto.run.running"));
  });

  it("says a run that has not taken a task yet has not, and says nothing of one that ended without", async () => {
    await render([run({ run: 1 }), run({ run: 2, status: "paused" }), run({ run: 3, status: "failed" })]);
    const tasks = [...container.querySelectorAll(".autorun__task")].map((one) => one.textContent);
    expect(tasks).toEqual([t("auto.run.noTask"), t("auto.run.noTask"), ""]);
  });

  /// A spot taken off the picture while its run walks on leaves the step with nothing to be inside
  /// of. The line says the step alone rather than a name the picture no longer holds.
  it("says the step alone where the spot it was opened from has gone", async () => {
    await render([run({ stepName: "Take one", stepsDone: 2 })]);
    const row = container.querySelector(".autorun__go")?.textContent ?? "";
    expect(row).toContain(tf("auto.run.step", { n: 2, step: "Take one" }));
  });

  it("reads a pause that has been asked for as neither running nor paused", async () => {
    await render([run({ pauseRequested: true })]);
    expect(container.textContent).toContain(t("auto.run.pausing"));
    expect(button(t("auto.run.pause")).disabled).toBe(true);
  });

  it("offers the move the state has, and to a failure only the press that acknowledges it", async () => {
    await render([run({ status: "paused" })]);
    expect(labels()).toContain(t("auto.run.resume"));
    expect(labels()).not.toContain(t("auto.run.pause"));

    await render([run({ status: "running" })]);
    expect(labels()).toContain(t("auto.run.pause"));

    await render([run({ status: "failed", stoppedReason: "crashed" })]);
    expect(labels()).not.toContain(t("auto.run.stop"));
    expect(labels()).not.toContain(t("auto.run.pause"));
    expect(container.textContent).toContain(t("auto.run.failed"));
    expect(container.textContent).toContain(t("auto.run.crashed"));
    await act(async () => { button(t("auto.run.acknowledge")).click(); });
    expect(hoisted.moved).toEqual(["acknowledge 4"]);
  });

  it("puts every reason a run can fail for into words", async () => {
    // Every arm core writes (`amenbo_core::model::AutomationStoppedReason`), so a reason added there
    // without words on this side is caught here rather than on somebody's screen.
    for (const [reason, key] of [
      ["crashed", "auto.run.crashed"],
      ["max_times", "auto.run.maxTimes"],
      ["no_agent", "auto.run.noAgent"],
      ["no_input", "auto.run.noInput"],
      ["no_way_on", "auto.run.noWayOn"],
      ["halted", "auto.run.halted"],
    ] as const) {
      await render([run({ status: "failed", stoppedReason: reason })]);
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

// The two entrances (`AMB-D-954`): from a project, that project's runs alone and no project column;
// from the sidebar, every project's, each naming its project.
describe("the running tab's reach", () => {
  const two = () => [
    run({ run: 4, project: 1, projectName: "amenbo", automationName: "Morning round" }),
    run({ run: 5, project: 2, projectName: "site", automationName: "Publish" }),
  ];

  it("lists one project's runs alone from that project, and leaves the project off the rows", async () => {
    await render(two(), 2);
    const rows = [...container.querySelectorAll(".autorun")].map((one) => one.textContent ?? "");
    expect(rows).toHaveLength(1);
    expect(rows[0]).toContain("Publish");
    expect(container.querySelector(".autorun__project")).toBeNull();
  });

  it("lists every project's runs from the sidebar, each naming its project", async () => {
    await render(two(), null);
    const projects = [...container.querySelectorAll(".autorun__project")].map((one) => one.textContent);
    expect(projects).toEqual(["amenbo", "site"]);
  });

  it("says nothing is running in a project whose runs are all elsewhere", async () => {
    await render(two(), 3);
    expect(container.textContent).toContain(t("auto.running.empty"));
  });
});
