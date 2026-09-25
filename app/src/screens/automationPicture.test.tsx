// @vitest-environment jsdom
// What the picture of the placements puts on the screen (`AMB-T-5255`). Where each thing goes is
// `automationLayout.test.ts`'s; this is what a reader can press and read.
//
// What these guard: **a spot is a button carrying the name of the action standing there**
// (`AMB-D-949`), which is what the panel beside the picture is opened from (`AMB-T-5256`); **a `+`
// stands on every edge** and is held shut while nothing is listening for the press, so one never
// lands on nothing; **where a run opens is marked on the box**; **a spot a required input does not
// reach names that input** rather than only turning red; and **an automation with nothing on it says
// so in words**.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { t, tf } from "../core/i18n";
import { AutomationPicture } from "./AutomationPicture";
import { automationGraph, type PicGraph } from "./automationLayout";
import type { AutomationDetailDto, AutomationPlacementDto } from "../bindings/bindings";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

function step(
  over: Partial<AutomationPlacementDto> & { id: number; name: string },
): AutomationPlacementDto {
  return {
    actionId: 900 + over.id,
    global: false,
    prompt: "",
    interactive: false,
    reportToTask: false,
    showHistory: true,
    showNotes: true,
    showDecisions: true,
    showComments: true,
    exits: [{ id: over.id * 10, name: "完了", outputs: [{ name: "task", kind: "task_take", required: true }] }],
    inputs: [],
    settings: [],
    steps: [],
    ...over,
  };
}

function detail(over: Partial<AutomationDetailDto> = {}): PicGraph {
  return automationGraph({
    id: 7,
    projectId: 1,
    name: "Morning round",
    notes: "",
    entryPlacementId: 1,
    archived: false,
    placements: [step({ id: 1, name: "Take the next task" })],
    edges: [],
    wires: [],
    heldBy: [],
    ...over,
  })!;
}

async function render(props: Parameters<typeof AutomationPicture>[0]) {
  await act(async () => {
    root.render(createElement(AutomationPicture, props));
  });
}

const nodes = () => [...container.querySelectorAll<HTMLButtonElement>(".autopic__node")];
const plusses = () => [...container.querySelectorAll<HTMLButtonElement>(".autopic__plus")];

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the picture of the steps", () => {
  /// The screen puts its "+ first …" where the picture would be, so an empty one says nothing itself.
  it("draws nothing where there is nothing to draw", async () => {
    await render({ graph: detail({ placements: [] }) });
    expect(container.textContent).toBe("");
    expect(nodes()).toHaveLength(0);
  });

  it("draws a step as a button carrying its name, and hands its id back when pressed", async () => {
    const onPickBox = vi.fn();
    await render({ graph: detail(), onPickBox });
    expect(nodes()[0]!.textContent).toContain("Take the next task");
    await act(async () => {
      nodes()[0]!.click();
    });
    expect(onPickBox).toHaveBeenCalledWith(1);
  });

  it("holds a step shut while nothing is listening for the press", async () => {
    await render({ graph: detail() });
    expect(nodes()[0]!.disabled).toBe(true);
  });

  it("marks the step whose contents are being shown", async () => {
    await render({ graph: detail(), selectedBoxId: 1, onPickBox: vi.fn() });
    expect(nodes()[0]!.getAttribute("aria-pressed")).toBe("true");
    expect(nodes()[0]!.className).toContain("autopic__node--on");
  });

  it("stands a + on every edge, shut while nothing is listening for the press", async () => {
    const one = detail({
      placements: [step({ id: 1, name: "take" }), step({ id: 2, name: "work", exits: [{ id: 20, name: "完了", outputs: [] }] })],
      edges: [
        { id: 1, fromId: 1, exitName: "完了", toId: 2, ends: "go" },
        { id: 2, fromId: 2, exitName: "完了", ends: "done" },
      ],
    });
    await render({ graph: one });
    expect(plusses()).toHaveLength(2);
    expect(plusses()[0]!.disabled).toBe(true);
    expect(plusses()[0]!.getAttribute("aria-label")).toBe(t("auto.pic.insert"));

    const onInsert = vi.fn();
    await render({ graph: one, onInsert });
    await act(async () => {
      plusses()[0]!.click();
    });
    expect(onInsert).toHaveBeenCalledWith(1);
  });

  it("names the action standing at a spot, and the required input nothing reaches", async () => {
    const one = detail({
      placements: [
        step({ id: 1, name: "take" }),
        step({
          id: 2,
          name: "Review",
          actionId: 4,
          exits: [{ id: 20, name: "完了", outputs: [] }],
          inputs: [{ name: "draft", kind: "file", required: true }],
        }),
      ],
      edges: [{ id: 1, fromId: 1, exitName: "完了", toId: 2, ends: "go" }],
    });
    await render({ graph: one });
    const review = nodes().find((node) => node.textContent?.includes("Review"))!;
    expect(review.className).toContain("autopic__node--unfed");
    // A mark with the count; the names it is missing are said on hover, and the second line keeps
    // saying where the action comes from.
    const mark = review.querySelector(".autopic__unfed")!;
    expect(mark.textContent).toBe("1");
    expect(mark.getAttribute("title")).toBe(tf("auto.pic.unfed", { names: "draft" }));
    expect(review.querySelector(".autopic__lib")?.textContent).toBe(t("auto.actions.reachProject"));
    expect(nodes()[0]!.querySelector(".autopic__unfed")).toBeNull();
  });

  /// An action with nothing in it cannot be started on, which the box says as a dashed outline and a
  /// mark — never for a built-in, which Amenbo carries out itself.
  it("marks the action with nothing in it, and not one with a step or a built-in", async () => {
    const one = detail({
      placements: [
        step({ id: 1, name: "hollow" }),
        step({ id: 2, name: "full", steps: [{ stepId: 5, name: "do" }] }),
        step({ id: 3, name: "take_task", builtin: "take_task" }),
      ],
    });
    await render({ graph: one });
    const box = (name: string) => nodes().find((node) => node.textContent?.includes(name))!;
    expect(box("hollow").className).toContain("autopic__node--empty");
    expect(box("hollow").textContent).toContain(t("auto.pic.emptyMark"));
    expect(box("full").className).not.toContain("autopic__node--empty");
    expect(nodes().filter((node) => node.className.includes("autopic__node--empty"))).toHaveLength(1);
  });

  /// Which box a launch enters by cannot be read off the lines: two stretches nothing joins are
  /// drawn the same, so the box says it.
  it("marks the placement a run opens, and only that one", async () => {
    const one = detail({
      placements: [
        step({ id: 1, name: "take" }),
        step({ id: 2, name: "work", exits: [{ id: 20, name: "完了", outputs: [] }] }),
      ],
      edges: [{ id: 1, fromId: 1, exitName: "完了", toId: 2, ends: "go" }],
      entryPlacementId: 2,
    });
    await render({ graph: one });
    const marked = nodes().filter((node) => node.querySelector(".autopic__entry") !== null);
    expect(marked).toHaveLength(1);
    expect(marked[0]!.textContent).toContain("work");
    expect(marked[0]!.textContent).toContain(t("auto.pic.entry"));
  });

  it("marks nothing while no placement is named the entry", async () => {
    await render({ graph: detail({ entryPlacementId: undefined }) });
    expect(container.querySelectorAll(".autopic__node .autopic__entry")).toHaveLength(0);
  });

  it("outlines the span of one task and names it", async () => {
    await render({ graph: detail() });
    expect(container.querySelectorAll(".autopic__lap")).toHaveLength(1);
    expect(container.textContent).toContain(t("auto.pic.lap"));
  });
});

describe("what the picture marks, as the mock draws it", () => {
  /** Take a task, then check it, which either sends it back to be fixed or on to be shipped. */
  const branching = () =>
    detail({
      placements: [
        step({ id: 1, name: "take", global: true }),
        step({
          id: 2,
          name: "check",
          exits: [
            { id: 21, name: "fix it", outputs: [] },
            { id: 22, name: "ship it", outputs: [] },
            { id: 23, name: "*", outputs: [] },
          ],
        }),
        step({ id: 3, name: "fix", exits: [{ id: 30, name: "完了", outputs: [] }] }),
        step({ id: 4, name: "ship", exits: [{ id: 40, name: "完了", outputs: [] }] }),
      ],
      edges: [
        { id: 1, fromId: 1, exitName: "完了", toId: 2, ends: "go" },
        { id: 2, fromId: 2, exitName: "fix it", toId: 3, ends: "go" },
        { id: 3, fromId: 2, exitName: "ship it", toId: 4, ends: "go" },
        { id: 4, fromId: 2, exitName: "*", ends: "halt" },
      ],
    });
  it("numbers each box, marks the one that takes the task, and says which library it comes from", async () => {
    await render({ graph: branching() });
    expect(nodes().map((one) => one.querySelector(".autopic__no")?.textContent)).toEqual(["1", "2", "3", "4"]);
    // Over the box that takes the task, and over no other.
    const takes = [...container.querySelectorAll<HTMLElement>(".autopic__takes")];
    expect(takes.map((one) => one.textContent)).toEqual([t("auto.step.takesTask")]);
    expect(takes[0]!.style.top).toBe(nodes()[0]!.style.top);
    expect(nodes()[0]!.querySelector(".autopic__lib")?.textContent).toBe(t("auto.actions.reachGlobal"));
    expect(nodes()[1]!.querySelector(".autopic__lib")?.textContent).toBe(t("auto.actions.reachProject"));
  });

  it("colours a line by what it is, ends it in an arrow, and says what the colours mean", async () => {
    await render({ graph: branching() });
    const drawn = [...container.querySelectorAll<SVGPolylineElement>("polyline")].map((one) => ({
      cls: one.getAttribute("class") ?? "",
      head: one.getAttribute("marker-end"),
    }));
    // In the order the layout lists them: the edges in the order they were made.
    expect(drawn[0]!.cls).toContain("autopic__line--next");
    expect(drawn[1]!.cls).toContain("autopic__line--branch");
    expect(drawn[2]!.cls).toContain("autopic__line--branch");
    expect(drawn[3]!.cls).toContain("autopic__line--error");
    // Into a box it ends in a head; one that stops the run ends in its words instead.
    expect(drawn[0]!.head).toMatch(/^url\(#.*-next\)$/);
    expect(drawn[3]!.head).toBeNull();
    const legend = container.querySelector(".autopic__legend")!;
    for (const key of [
      "auto.pic.legendNext",
      "auto.pic.legendBack",
      "auto.pic.legendBranch",
      "auto.pic.legendWire",
      "auto.pic.lap",
      "auto.pic.entry",
      "auto.pic.legendUnfed",
    ]) {
      expect(legend.textContent).toContain(t(key));
    }
    // Only an action's picture has lines leaving it by one of its ways out.
    expect(legend.textContent).not.toContain(t("auto.pic.legendLeaves"));
  });
});
