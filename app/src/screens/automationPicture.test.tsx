// @vitest-environment jsdom
// What the picture of the placements puts on the screen (`AMB-T-5255`). Where each thing goes is
// `automationLayout.test.ts`'s; this is what a reader can press and read.
//
// What these guard: **a spot is a button carrying the name of the action standing there**
// (`AMB-D-949`), which is what the panel beside the picture is opened from (`AMB-T-5256`); **a `+`
// stands on every edge**, shown while its line is pointed at or goes into or out of the picked box, and is held shut while nothing is
// listening for the press, so one never lands on nothing; **where a run opens is marked on the box**; **a spot a required input does not
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

  /// A run's pane sends the reader here with the box it stopped at picked (`AMB-T-5594`).
  it("brings a box picked out of sight to the middle until the reader moves, and leaves one in sight", async () => {
    // jsdom lays nothing out, so the observer is stood in by one whose answer the test gives. An
    // observer let go answers no more, as the real one does.
    const watched: { box: Element; answer: (ratio: number, height?: number) => void }[] = [];
    const wasObserver = globalThis.IntersectionObserver;
    globalThis.IntersectionObserver = class {
      private gone = false;
      constructor(private readonly call: IntersectionObserverCallback) {}
      observe(box: Element) {
        watched.push({
          box,
          answer: (ratio, height = 40) => {
            if (this.gone) return;
            this.call(
              [{ intersectionRatio: ratio, boundingClientRect: { height } } as IntersectionObserverEntry],
              this as never,
            );
          },
        });
      }
      unobserve() {}
      disconnect() {
        this.gone = true;
      }
    } as never;
    // The box taking a size is stood in the same way: the test says when it happens.
    const sizes: (() => void)[] = [];
    const wasSize = globalThis.ResizeObserver;
    globalThis.ResizeObserver = class {
      constructor(private readonly call: ResizeObserverCallback) {}
      observe() {
        sizes.push(() => this.call([], this as never));
      }
      disconnect() {}
    } as never;
    const moved: [Element, ScrollIntoViewOptions | undefined][] = [];
    const wasScroll = Element.prototype.scrollIntoView;
    Element.prototype.scrollIntoView = function scrollIntoView(this: Element, how?: ScrollIntoViewOptions) {
      moved.push([this, how]);
    } as never;
    try {
      await render({ graph: null, selectedBoxId: 1, onPickBox: vi.fn() });
      expect(watched).toHaveLength(0);
      await render({ graph: detail(), selectedBoxId: 1, onPickBox: vi.fn() });
      expect(watched.map((one) => one.box)).toEqual([nodes()[0]]);
      watched[0]!.answer(0.5);
      expect(moved).toEqual([[nodes()[0], { block: "center", inline: "nearest" }]]);
      // Drawn again with the same pick, the watch is not set up again.
      await render({ graph: detail(), selectedBoxId: 1, onPickBox: vi.fn() });
      expect(watched).toHaveLength(1);
      // Picked again, and wholly in sight this time: it stays where it is.
      await render({ graph: detail(), onPickBox: vi.fn() });
      await render({ graph: detail(), selectedBoxId: 1, onPickBox: vi.fn() });
      watched[1]!.answer(1);
      expect(moved).toHaveLength(1);
      // Seen whole, then pushed down by a band that lands after it (`AMB-T-5595`): brought back.
      watched[1]!.answer(0.3);
      expect(moved).toHaveLength(2);
      // Brought into sight and pushed out again by the next band: brought back again.
      watched[1]!.answer(1);
      watched[1]!.answer(0.3);
      expect(moved).toHaveLength(3);
      // On a face not shown yet it has no size, and the watch waits rather than moving or ending.
      // Out of sight at both of the observer's looks, the box crosses nothing and the observer stays
      // silent, so the box taking a size asks it again, and its fresh answer moves the box.
      await render({ graph: detail(), onPickBox: vi.fn() });
      await render({ graph: detail(), selectedBoxId: 1, onPickBox: vi.fn() });
      watched[2]!.answer(0, 0);
      expect(moved).toHaveLength(3);
      sizes[sizes.length - 1]!();
      expect(watched).toHaveLength(4);
      watched[3]!.answer(0);
      expect(moved).toHaveLength(4);
      // A band landing over the board changes the document without touching the box: asked again.
      const band = document.body.appendChild(document.createElement("div"));
      await act(async () => {});
      band.remove();
      await act(async () => {});
      expect(watched.length).toBeGreaterThan(4);
      watched[watched.length - 1]!.answer(0.3);
      expect(moved).toHaveLength(5);
      // The reader's own wheel ends the watch: where the screen stands is theirs from then on.
      await render({ graph: detail(), onPickBox: vi.fn() });
      await render({ graph: detail(), selectedBoxId: 1, onPickBox: vi.fn() });
      const last = watched.length;
      watched[last - 1]!.answer(1);
      window.dispatchEvent(new Event("wheel"));
      watched[last - 1]!.answer(0.3);
      expect(moved).toHaveLength(5);
    } finally {
      globalThis.IntersectionObserver = wasObserver;
      globalThis.ResizeObserver = wasSize;
      Element.prototype.scrollIntoView = wasScroll;
    }
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

  /// One on every line all the time crowded the picture (`AMB-T-5697`): the `+` shows while its line is
  /// pointed at, and is a button the whole time, so Tab still reaches it.
  it("shows a line's + while the line is pointed at, and only that line's", async () => {
    const one = detail({
      placements: [step({ id: 1, name: "take" }), step({ id: 2, name: "work", exits: [{ id: 20, name: "完了", outputs: [] }] })],
      edges: [
        { id: 1, fromId: 1, exitName: "完了", toId: 2, ends: "go" },
        { id: 2, fromId: 2, exitName: "完了", ends: "done" },
      ],
    });
    await render({ graph: one, onInsert: vi.fn() });
    const near = () => plusses().map((plus) => plus.className.includes("autopic__plus--near"));
    expect(near()).toEqual([false, false]);
    expect(plusses().every((plus) => plus.tabIndex === 0 && !plus.disabled)).toBe(true);

    const hits = [...container.querySelectorAll(".autopic__hit")];
    expect(hits).toHaveLength(2);
    await act(async () => {
      hits[1]!.dispatchEvent(new MouseEvent("mouseover", { bubbles: true, relatedTarget: document.body }));
    });
    expect(near()).toEqual([false, true]);
    await act(async () => {
      hits[1]!.dispatchEvent(new MouseEvent("mouseout", { bubbles: true, relatedTarget: document.body }));
    });
    expect(near()).toEqual([false, false]);
  });

  /// A `+` only a pointer brings up is one nobody finds (`AMB-D-1003`): the lines into and out of the
  /// picked box show theirs all the time, and the others still wait to be pointed at.
  it("shows the + on the lines into and out of the picked box, and on no others", async () => {
    const three = detail({
      placements: [
        step({ id: 1, name: "take" }),
        step({ id: 2, name: "work" }),
        step({ id: 3, name: "close", exits: [{ id: 30, name: "完了", outputs: [] }] }),
      ],
      edges: [
        { id: 1, fromId: 1, exitName: "完了", toId: 2, ends: "go" },
        { id: 2, fromId: 2, exitName: "完了", toId: 3, ends: "go" },
        { id: 3, fromId: 3, exitName: "完了", ends: "done" },
      ],
    });
    const shown = () => plusses().filter((plus) => plus.className.includes("autopic__plus--near")).length;

    await render({ graph: three, onInsert: vi.fn() });
    expect(plusses()).toHaveLength(3);
    expect(shown()).toBe(0);

    await render({ graph: three, onInsert: vi.fn(), selectedBoxId: 1, onPickBox: vi.fn() });
    expect(shown()).toBe(1);

    await render({ graph: three, onInsert: vi.fn(), selectedBoxId: 2, onPickBox: vi.fn() });
    expect(shown()).toBe(2);

    await render({ graph: three, onInsert: vi.fn(), selectedBoxId: 3, onPickBox: vi.fn() });
    expect(shown()).toBe(2);
  });

  /// The wires took the right of the picture and crossed the tops of the boxes (`AMB-D-1001`): they
  /// are read in the panel, and the picture draws and names none of them, whatever is picked.
  it("draws and names no wire, whichever box is picked", async () => {
    const one = detail({
      placements: [
        step({ id: 1, name: "take" }),
        step({ id: 2, name: "work", exits: [{ id: 20, name: "完了", outputs: [] }] }),
        step({ id: 3, name: "close", inputs: [{ name: "task", kind: "task_take", required: true }] }),
      ],
      edges: [
        { id: 1, fromId: 1, exitName: "完了", toId: 2, ends: "go" },
        { id: 2, fromId: 2, exitName: "完了", toId: 3, ends: "go" },
      ],
      wires: [{ id: 5, fromId: 1, fromExitName: "完了", fromPortName: "task", toId: 3, toPortName: "task" }],
    });
    const words = () => [...container.querySelectorAll("text.autopic__word")].map((one) => one.textContent);
    for (const picked of [undefined, 1, 2, 3]) {
      await render({ graph: one, onPickBox: vi.fn(), selectedBoxId: picked });
      // The two edges; the last box's way out says nothing yet and hangs dashed on its own.
      expect(container.querySelectorAll("polyline.autopic__line:not(.autopic__line--open)")).toHaveLength(2);
      expect(words().some((word) => word?.includes("task"))).toBe(false);
    }
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

  /// An action made on the spot and still being made says so, and not that it is empty: having
  /// nothing in it yet is what being made means (`AMB-D-1005`).
  it("marks the action still being made, in place of empty", async () => {
    const one = detail({
      placements: [step({ id: 1, name: "making", draft: true }), step({ id: 2, name: "hollow" })],
    });
    await render({ graph: one });
    const box = (name: string) => nodes().find((node) => node.textContent?.includes(name))!;
    expect(box("making").className).toContain("autopic__node--draft");
    expect(box("making").textContent).toContain(t("chip.draft"));
    expect(box("making").className).not.toContain("autopic__node--empty");
    expect(box("hollow").className).not.toContain("autopic__node--draft");
  });

  /// A box Amenbo put on for the reader says so, and what it is there for, so it is not taken for one
  /// the reader put there themselves (`AMB-T-5797`).
  it("marks a box put on for the reader, and says what it is there for", async () => {
    const one = detail({
      placements: [step({ id: 1, name: "write" }), step({ id: 2, name: "close" })],
    });
    await render({ graph: one, placedForYou: new Set([2]) });
    const box = (name: string) => nodes().find((node) => node.textContent?.includes(name))!;
    expect(box("close").textContent).toContain(t("auto.pic.placedForYou"));
    expect(box("close").textContent).toContain(t("auto.pic.placedForYouWhy"));
    expect(box("write").textContent).not.toContain(t("auto.pic.placedForYou"));
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

describe("what a step's card says it takes in and hands on (AMB-T-5798)", () => {
  // Two steps: the first reads what the action was handed and hands on a theme, which the second reads.
  function action(): PicGraph {
    return {
      ...detail({
        placements: [
          step({
            id: 1,
            name: "Ask for a theme",
            inputs: [{ name: "topic", kind: "value", required: true }],
            exits: [{ id: 10, name: "完了", outputs: [{ name: "theme", kind: "value", required: true }] }],
          }),
          step({
            id: 2,
            name: "Write it up",
            inputs: [{ name: "theme", kind: "value", required: true }],
            exits: [{ id: 20, name: "完了", outputs: [] }],
          }),
        ],
        edges: [{ id: 1, fromId: 1, exitName: "完了", toId: 2, ends: "go" }],
        wires: [
          { id: 1, fromId: 0, fromPortName: "topic", toId: 1, toPortName: "topic" },
          { id: 2, fromId: 1, fromExitName: "完了", fromPortName: "theme", toId: 2, toPortName: "theme" },
        ],
      }),
      boundary: { inputs: [{ name: "topic", kind: "value", required: true }], exits: [] },
    };
  }
  const io = (at: number) => [...nodes()[at]!.querySelectorAll(".autopic__iopart")].map((one) => one.textContent);

  it("names each input with where it comes from, and each output, on an action's picture", async () => {
    await render({ graph: action() });
    expect(io(0)).toEqual([
      tf("auto.pic.ioIn", { names: tf("auto.pic.ioFromAction", { name: "topic" }) }),
      tf("auto.pic.ioOut", { names: "theme" }),
    ]);
    expect(io(1)).toEqual([tf("auto.pic.ioIn", { names: tf("auto.pic.ioFrom", { name: "theme", no: 1 }) })]);
  });

  it("names an input nothing reaches by its name alone, and says nothing for a step with neither", async () => {
    const graph = action();
    await render({ graph: { ...graph, wires: [] } });
    expect(io(1)).toEqual([tf("auto.pic.ioIn", { names: "theme" })]);
    const bare = { ...graph, boxes: graph.boxes.map((box) => ({ ...box, inputs: [], exits: [] })) };
    await render({ graph: bare });
    expect(container.querySelector(".autopic__io")).toBeNull();
  });

  it("says none of it on an automation's picture, whose second line is the library, and keeps its boxes a line shorter", async () => {
    await render({ graph: action() });
    const tall = parseFloat(nodes()[0]!.style.height);
    const { boundary: _, ...graph } = action();
    await render({ graph });
    expect(container.querySelector(".autopic__io")).toBeNull();
    expect(parseFloat(nodes()[0]!.style.height)).toBeLessThan(tall);
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
    const drawn = [...container.querySelectorAll<SVGPolylineElement>("polyline.autopic__line")].map((one) => ({
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
      "auto.pic.lap",
      "auto.pic.entry",
      "auto.pic.legendUnfed",
      "auto.pic.legendOpen",
    ]) {
      expect(legend.textContent).toContain(t(key));
    }
    // Only an action's picture has lines leaving it by one of its ways out.
    expect(legend.textContent).not.toContain(t("auto.pic.legendLeaves"));
  });
});

/// Drawn on a run's pane, the picture is the run's trail (`AMB-T-5775`).
describe("the picture of a run", () => {
  const two = () =>
    detail({
      placements: [step({ id: 1, name: "take" }), step({ id: 2, name: "work", exits: [{ id: 20, name: "完了", outputs: [] }] })],
      edges: [
        { id: 1, fromId: 1, exitName: "完了", toId: 2, ends: "go" },
        { id: 2, fromId: 2, exitName: "完了", ends: "done" },
      ],
    });
  const trail = { boxes: new Set([1, 2]), edges: new Set([1]), at: 2 };

  it("lights what the run walked, marks the box under way, and puts nothing in", async () => {
    await render({ graph: two(), selectedBoxId: 2, trail });
    expect(nodes()[0]!.className).toContain("autopic__node--lit");
    expect(nodes()[0]!.className).not.toContain("autopic__node--at");
    expect(nodes()[1]!.className).toContain("autopic__node--at");
    expect(nodes()[1]!.className).toContain("autopic__node--on");
    const lit = [...container.querySelectorAll("polyline.autopic__line")].map((one) =>
      (one.getAttribute("class") ?? "").includes("autopic__line--lit"),
    );
    expect(lit).toEqual([true, false]);
    // Its arrowhead is lit with it.
    expect(container.querySelector("polyline.autopic__line--lit")?.getAttribute("marker-end")).toMatch(/-lit\)$/);
    expect(plusses()).toHaveLength(0);
    expect(container.querySelector(".autopic__legend")).toBeNull();
  });

  it("brings the box the run reaches to the middle, though it is in sight already", async () => {
    const wasObserver = globalThis.IntersectionObserver;
    const wasScroll = Element.prototype.scrollIntoView;
    const scrolled = vi.fn();
    globalThis.IntersectionObserver = class {
      observe() {}
      unobserve() {}
      disconnect() {}
    } as unknown as typeof IntersectionObserver;
    Element.prototype.scrollIntoView = scrolled;
    try {
      await render({ graph: two(), selectedBoxId: 1, trail: { ...trail, at: 1 } });
      expect(scrolled).toHaveBeenLastCalledWith({ block: "center", inline: "center" });
      scrolled.mockClear();
      await render({ graph: two(), selectedBoxId: 2, trail });
      expect(scrolled).toHaveBeenCalledTimes(1);
    } finally {
      globalThis.IntersectionObserver = wasObserver;
      Element.prototype.scrollIntoView = wasScroll;
    }
  });
});

describe("a way out nothing has been decided for (AMB-D-1003)", () => {
  const lone = () => detail({ entryPlacementId: 1, placements: [step({ id: 1, name: "write" })] });
  const presses = () => [...container.querySelectorAll<HTMLButtonElement>(".autopic__open")];

  it("hangs dashed and ends in a press shown without being pointed at, which puts the next box on it", async () => {
    const onOpenExit = vi.fn();
    await render({ graph: lone(), onOpenExit });
    expect(container.querySelectorAll("polyline.autopic__line--open")).toHaveLength(1);
    expect(presses().map((one) => one.textContent)).toEqual([t("auto.pic.openPut")]);
    expect(presses()[0]!.disabled).toBe(false);
    await act(async () => presses()[0]!.click());
    expect(onOpenExit).toHaveBeenCalledWith({ boxId: 1, exitName: "完了" });
  });

  it("holds the press shut where nothing can be put in, and draws none on a run's trail", async () => {
    await render({ graph: lone() });
    expect(presses()[0]!.disabled).toBe(true);
    await render({ graph: lone(), trail: { boxes: new Set(), edges: new Set() } });
    expect(presses()).toHaveLength(0);
  });
});
