// @vitest-environment jsdom
// What the picture of the steps puts on the screen (`AMB-T-5255`). Where each thing goes is
// `automationLayout.test.ts`'s; this is what a reader can press and read.
//
// What these guard: **a step is a button carrying its name**, which is what the step panel beside
// the picture is opened from (`AMB-T-5256`); **a `+` stands on every edge** and is held shut while
// the dialog behind it is unbuilt (`AMB-T-5257`), so a press never lands on nothing; **a step whose
// prompt came from the library says so**, and **one a required input does not reach names that
// input** rather than only turning red; and **an automation with no steps says so in words**.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { t, tf } from "../core/i18n";
import { AutomationPicture } from "./AutomationPicture";
import type { AutomationDetailDto, AutomationStepDto } from "../bindings/bindings";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

function step(over: Partial<AutomationStepDto> & { id: number; name: string }): AutomationStepDto {
  return {
    prompt: "",
    agent: "claude-code",
    interactive: false,
    reportToTask: false,
    showHistory: true,
    exits: [{ id: over.id * 10, outputs: [{ name: "task", kind: "task_take", required: true }] }],
    inputs: [],
    settings: [],
    ...over,
  };
}

function detail(over: Partial<AutomationDetailDto> = {}): AutomationDetailDto {
  return {
    id: 7,
    projectId: 1,
    name: "Morning round",
    notes: "",
    preamble: "",
    entryStepId: 1,
    archived: false,
    steps: [step({ id: 1, name: "Take the next task" })],
    edges: [],
    wires: [],
    ...over,
  };
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
    await render({ automation: detail({ steps: [] }) });
    expect(container.textContent).toContain(t("auto.pic.empty"));
    expect(nodes()).toHaveLength(0);
  });

  it("draws a step as a button carrying its name, and hands its id back when pressed", async () => {
    const onPickStep = vi.fn();
    await render({ automation: detail(), onPickStep });
    expect(nodes()[0]!.textContent).toContain("Take the next task");
    await act(async () => {
      nodes()[0]!.click();
    });
    expect(onPickStep).toHaveBeenCalledWith(1);
  });

  it("holds a step shut while nothing is listening for the press", async () => {
    await render({ automation: detail() });
    expect(nodes()[0]!.disabled).toBe(true);
  });

  it("marks the step whose contents are being shown", async () => {
    await render({ automation: detail(), selectedStepId: 1, onPickStep: vi.fn() });
    expect(nodes()[0]!.getAttribute("aria-pressed")).toBe("true");
    expect(nodes()[0]!.className).toContain("autopic__node--on");
  });

  it("stands a + on every edge, shut until there is a dialog behind it", async () => {
    const one = detail({
      steps: [step({ id: 1, name: "take" }), step({ id: 2, name: "work", exits: [{ id: 20, outputs: [] }] })],
      edges: [
        { id: 1, fromStepId: 1, toStepId: 2, ends: "go" },
        { id: 2, fromStepId: 2, ends: "done" },
      ],
    });
    await render({ automation: one });
    expect(plusses()).toHaveLength(2);
    expect(plusses()[0]!.disabled).toBe(true);
    expect(plusses()[0]!.getAttribute("aria-label")).toBe(t("auto.pic.insert"));

    const onInsertStep = vi.fn();
    await render({ automation: one, onInsertStep });
    await act(async () => {
      plusses()[0]!.click();
    });
    expect(onInsertStep).toHaveBeenCalledWith(1);
  });

  it("says where a step got its prompt, and which required input nothing reaches", async () => {
    const one = detail({
      steps: [
        step({ id: 1, name: "take" }),
        step({
          id: 2,
          name: "review",
          actionId: 4,
          actionName: "Review",
          exits: [{ id: 20, outputs: [] }],
          inputs: [{ name: "draft", kind: "file", required: true }],
        }),
      ],
      edges: [{ id: 1, fromStepId: 1, toStepId: 2, ends: "go" }],
    });
    await render({ automation: one });
    const review = nodes().find((node) => node.textContent?.includes("review"))!;
    expect(review.textContent).toContain(tf("auto.pic.fromAction", { action: "Review" }));
    expect(review.textContent).toContain("draft");
    expect(review.className).toContain("autopic__node--unfed");
    expect(review.className).toContain("autopic__node--action");
  });

  it("outlines the span of one task and names it", async () => {
    await render({ automation: detail() });
    expect(container.querySelectorAll(".autopic__lap")).toHaveLength(1);
    expect(container.textContent).toContain(t("auto.pic.lap"));
  });
});
