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
import { t } from "../core/i18n";
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
    prompt: "",
    interactive: false,
    reportToTask: false,
    showHistory: true,
    exits: [{ id: over.id * 10, outputs: [{ name: "task", kind: "task_take", required: true }] }],
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
  it("says so where there is nothing to draw", async () => {
    await render({ graph: detail({ placements: [] }) });
    expect(container.textContent).toContain(t("auto.pic.empty"));
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
      placements: [step({ id: 1, name: "take" }), step({ id: 2, name: "work", exits: [{ id: 20, outputs: [] }] })],
      edges: [
        { id: 1, fromId: 1, toId: 2, ends: "go" },
        { id: 2, fromId: 2, ends: "done" },
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
          exits: [{ id: 20, outputs: [] }],
          inputs: [{ name: "draft", kind: "file", required: true }],
        }),
      ],
      edges: [{ id: 1, fromId: 1, toId: 2, ends: "go" }],
    });
    await render({ graph: one });
    const review = nodes().find((node) => node.textContent?.includes("Review"))!;
    expect(review.textContent).toContain("draft");
    expect(review.className).toContain("autopic__node--unfed");
  });

  /// Which box a launch enters by cannot be read off the lines: two stretches nothing joins are
  /// drawn the same, so the box says it.
  it("marks the placement a run opens, and only that one", async () => {
    const one = detail({
      placements: [
        step({ id: 1, name: "take" }),
        step({ id: 2, name: "work", exits: [{ id: 20, outputs: [] }] }),
      ],
      edges: [{ id: 1, fromId: 1, toId: 2, ends: "go" }],
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
    expect(container.querySelectorAll(".autopic__entry")).toHaveLength(0);
  });

  it("outlines the span of one task and names it", async () => {
    await render({ graph: detail() });
    expect(container.querySelectorAll(".autopic__lap")).toHaveLength(1);
    expect(container.textContent).toContain(t("auto.pic.lap"));
  });
});
