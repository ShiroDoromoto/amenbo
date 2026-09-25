// @vitest-environment jsdom
// What the panel draws for the pressed spot and what each control writes (`AMB-T-5256`,
// `AMB-T-5282`, `AMB-T-5374`). The reads and the write doors are stubbed; every field, its control and
// what it sends run for real.
//
// What these guard: **nothing pressed says so** rather than drawing an empty form; **only what is
// this spot's own is written here** (`AMB-D-954`) — the answer to a setting, the wire into an input,
// what happens after each way out, the entry, and **who carries out each step** (`AMB-D-960`) —
// while **what the action declares and what its steps carry are read, not written**: no box to
// rename, no prompt, nothing to declare;
// **the action is named with the press that goes to where it is built**, and an empty one says so;
// **a setting is answered by the control its kind takes**, a task filter on rows rather than in a
// filter expression; **an input is filled from a list of what fits**; **the error way out is always
// the last line of the ways out**; and **a refusal lands on the panel** rather than in the console.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type {
  AutomationActionDetailDto,
  AutomationDetailDto,
  AutomationPlacementDto,
} from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  action: null as AutomationActionDetailDto | null,
  answerCfg: vi.fn(),
  chooseAgent: vi.fn(),
  setWire: vi.fn(),
  clearWire: vi.fn(),
  setEntry: vi.fn(),
  addEdge: vi.fn(),
  editEdge: vi.fn(),
  removeEdge: vi.fn(),
  removePlacement: vi.fn(),
}));

vi.mock("../core/automations", () => ({
  useAutomationAction: () => hoisted.action,
  answerAutomationCfg: hoisted.answerCfg,
  chooseAutomationAgent: hoisted.chooseAgent,
  setAutomationWire: hoisted.setWire,
  clearAutomationWire: hoisted.clearWire,
  setAutomationEntry: hoisted.setEntry,
  addAutomationEdge: hoisted.addEdge,
  editAutomationEdge: hoisted.editEdge,
  removeAutomationEdge: hoisted.removeEdge,
  removeAutomationPlacement: hoisted.removePlacement,
}));
// Taking a spot off asks first, and what the machine would put up is not this test's business.
vi.mock("../core/dialog", () => ({ confirmDialog: () => Promise.resolve(true) }));
vi.mock("../core/boundFolders", () => ({
  useBoundFolders: () => ({ all: [], live: [], answered: true }),
}));
// The machine's own answers. `inTauri` is false in this environment, so neither probe is made and
// what the panel draws is the spot's own value plus whatever these would have added.
vi.mock("../core/ipc", () => ({ invoke: () => Promise.resolve(null) }));

import { t, tf } from "../core/i18n";
import { AutomationStepPanel } from "./AutomationStepPanel";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

/** The action standing at a spot, flattened onto it the way the door hands it over. */
function spot(over: Partial<AutomationPlacementDto> = {}): AutomationPlacementDto {
  return {
    id: 1,
    name: "Take the next task",
    actionId: 4, global: false,
    stepId: 11,
    prompt: "take one",
    interactive: false,
    reportToTask: false,
    showHistory: true,
    showTask: true,
    exits: [{ id: 10, outputs: [] }, { id: 11, name: "*", outputs: [] }],
    inputs: [],
    settings: [],
    steps: [],
    ...over,
  };
}

function detail(over: Partial<AutomationDetailDto> = {}): AutomationDetailDto {
  return {
    id: 7,
    projectId: 1,
    name: "Morning round",
    notes: "",
    entryPlacementId: 1,
    archived: false,
    placements: [spot()],
    edges: [],
    wires: [],
    heldBy: [],
    ...over,
  };
}

type Props = Parameters<typeof AutomationStepPanel>[0];

/** The library action standing at the spot, as its own screen reads it. */
function action(over: Partial<AutomationActionDetailDto> = {}): AutomationActionDetailDto {
  return {
    id: 4,
    name: "Take the next task",
    note: "Takes one task off the list",
    global: false,
    usedBy: 2,
    steps: [],
    edges: [],
    wires: [],
    exits: [],
    inputs: [],
    settings: [],
    heldBy: [],
    ...over,
  };
}

/** The screen's two callbacks are its business, so a test that is not about them passes neither. */
async function render(
  props: Omit<Props, "onRemoved" | "onOpenAction"> & {
    onRemoved?: () => void;
    onOpenAction?: (id: number) => void;
  },
) {
  await act(async () => {
    root.render(
      createElement(AutomationStepPanel, {
        onRemoved: () => undefined,
        onOpenAction: () => undefined,
        ...props,
      }),
    );
  });
}

const selects = () => [...container.querySelectorAll<HTMLSelectElement>("select")];

/** Type into a controlled box the way React hears it — its own value tracker has to be moved. */
async function typeInto(box: HTMLInputElement, value: string) {
  await act(async () => {
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
    setter.call(box, value);
    box.dispatchEvent(new Event("input", { bubbles: true }));
  });
}
const boxes = () => [...container.querySelectorAll<HTMLInputElement>("input")];

/** The tick box of the row that reads like this, found by the words beside it. */
const checkFor = (label: string) =>
  [...container.querySelectorAll<HTMLLabelElement>(".autostep__check")]
    .find((one) => one.textContent?.includes(label))!
    .querySelector<HTMLInputElement>("input")!;

/** The pulldown that says what happens after one way out, found by the way out it hangs on. */
const nextFor = (exit: string) =>
  [...container.querySelectorAll<HTMLSelectElement>("select")].find(
    (one) => one.getAttribute("aria-label") === exit,
  )!;

/** Pick a value on a pulldown the way a reader does. */
async function pick(control: HTMLSelectElement, value: string) {
  await act(async () => {
    control.value = value;
    control.dispatchEvent(new Event("change", { bubbles: true }));
  });
}

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  hoisted.action = action();
  for (const one of [hoisted.answerCfg, hoisted.setWire, hoisted.clearWire, hoisted.setEntry,
    hoisted.addEdge, hoisted.editEdge, hoisted.removeEdge, hoisted.removePlacement]) one.mockReset();
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the panel of one spot", () => {
  it("says so while nothing is pressed", async () => {
    await render({ automation: detail(), placementId: null });
    expect(container.textContent).toContain(t("auto.step.none"));
    expect(selects()).toHaveLength(0);
  });




  /// What the action declares and what its steps carry are written on its own screen, where a rewrite
  /// reaches every placement of it — so none of them is a control here.
  it("reads what the action declares and what its steps carry, and writes none of it", async () => {
    const one = detail({
      placements: [
        spot({
          exits: [{ id: 10, name: "got one", outputs: [{ name: "task", kind: "task_take", required: true }] }, { id: 11, name: "*", outputs: [] }],
          inputs: [{ name: "note", kind: "value", required: true }],
          settings: [{ name: "depth", kind: "choice", required: false, options: '["quick"]' }],
        }),
      ],
    });
    await render({ automation: one, placementId: 1 });
    expect(container.querySelector("textarea")).toBeNull();
    expect(container.querySelector(".autostep__declare")).toBeNull();
    // The names are drawn as words, not in boxes to rewrite.
    expect(boxes().some((one) => one.value === "got one" || one.value === "Take the next task")).toBe(false);
    expect(container.textContent).toContain("got one");
    expect(container.textContent).toContain("task");
    expect(container.textContent).toContain("note");
    expect(container.textContent).toContain("depth");
  });

  /// Who carries a step out is this spot's own (`AMB-D-960`): one row per step of the action, the
  /// agent first and then its model, and the empty agent is nobody chosen.
  it("chooses who carries out each step of the action here", async () => {
    const chosen = detail({
      placements: [
        spot({
          steps: [
            { stepId: 11, name: "取る", agent: "claude-code", model: "opus" },
            { stepId: 12, name: "見直す" },
          ],
        }),
      ],
    });
    await render({ automation: chosen, placementId: 1 });
    expect(container.textContent).toContain(t("auto.place.agents"));
    const agentOf = (step: string) =>
      selects().find((one) => one.getAttribute("aria-label") === tf("auto.place.agentOf", { step }))!;
    const modelOf = (step: string) =>
      selects().find((one) => one.getAttribute("aria-label") === tf("auto.place.modelOf", { step }))!;
    expect(agentOf("取る").value).toBe("claude-code");
    expect(modelOf("取る").value).toBe("opus");
    expect(agentOf("見直す").value).toBe("");
    expect(modelOf("見直す").disabled).toBe(true);

    await pick(modelOf("取る"), "");
    expect(hoisted.chooseAgent).toHaveBeenCalledWith(1, 11, "claude-code", null);
    await pick(agentOf("取る"), "");
    expect(hoisted.chooseAgent).toHaveBeenCalledWith(1, 11, null, null);
  });

  it("names the action with its reach and a press to where it is built", async () => {
    const opened = vi.fn();
    await render({ automation: detail(), placementId: 1, onOpenAction: opened });
    expect(container.textContent).toContain("Take the next task");
    expect(container.textContent).toContain(t("auto.actions.reachProject"));
    expect(container.textContent).toContain("Takes one task off the list");
    expect(container.textContent).toContain(t("auto.place.ownedWhat"));
    const press = [...container.querySelectorAll<HTMLButtonElement>("button")].find(
      (one) => one.textContent === t("auto.place.open"),
    )!;
    await act(async () => press.click());
    expect(opened).toHaveBeenCalledWith(4);
  });

  it("holds every write shut while a run holds the automation, and still goes to the action (AMB-D-961)", async () => {
    const opened = vi.fn();
    await render({ automation: detail(), placementId: 1, onOpenAction: opened, readOnly: true });
    const press = [...container.querySelectorAll<HTMLButtonElement>("button")].find(
      (one) => one.textContent === t("auto.place.open"),
    )!;
    expect(press.closest("fieldset")).toBeNull();
    await act(async () => press.click());
    expect(opened).toHaveBeenCalledWith(4);
    const remove = [...container.querySelectorAll<HTMLButtonElement>("button")].find(
      (one) => one.textContent === t("auto.step.placementRemove"),
    )!;
    expect(remove.closest("fieldset")?.disabled).toBe(true);
  });

  it("says an action with nothing in it cannot be started until it is built", async () => {
    await render({ automation: detail(), placementId: 1 });
    expect(container.textContent).toContain(t("auto.place.empty"));
    hoisted.action = action({
      steps: [{ id: 11, name: "claim", prompt: "take", interactive: false, reportToTask: false, showHistory: true, showTask: true, exits: [], inputs: [] } as unknown as AutomationActionDetailDto["steps"][number]],
    });
    await render({ automation: detail(), placementId: 1 });
    expect(container.textContent).not.toContain(t("auto.place.empty"));
  });

  it("answers a task filter on rows, and writes the object naming each part", async () => {
    const one = detail({
      placements: [spot({ settings: [{ name: "which", kind: "taskfilter", required: true }] })],
    });
    await render({ automation: one, placementId: 1 });
    const chip = [...container.querySelectorAll<HTMLButtonElement>(".autostep__chip")].find(
      (b) => b.textContent === t("filter.opt.assignee.meAi"),
    )!;
    await act(async () => {
      chip.click();
    });
    // The answer is the placement's, so one action placed twice is not answered for both.
    expect(hoisted.answerCfg).toHaveBeenCalledWith(1, "which", '{"assignee":["me-ai"]}');
  });

  it("says the order as a sentence, and writes it beside the parts it orders", async () => {
    // The order has no label of its own: it is the end of the sentence the rows begin (`AMB-T-5412`).
    const one = detail({
      placements: [spot({
        settings: [{ name: "which", kind: "taskfilter", required: true, value: '{"status":["todo"]}' }],
      })],
    });
    await render({ automation: one, placementId: 1 });
    const list = container.querySelector<HTMLSelectElement>(".autostep__rows select")!;
    // An answer that names no order is taken highest priority first, and the list says so.
    expect(list.value).toBe("priority");
    expect(list.disabled).toBe(false);
    expect(list.closest(".autostep__row")?.textContent).toContain(t("auto.step.sort.priority"));
    await act(async () => {
      list.value = "due";
      list.dispatchEvent(new Event("change", { bubbles: true }));
    });
    expect(hoisted.answerCfg).toHaveBeenCalledWith(1, "which", '{"status":["todo"],"sort":"due"}');
  });

  it("holds the order back while no row is pressed, there being nothing to order", async () => {
    const one = detail({
      placements: [spot({ settings: [{ name: "which", kind: "taskfilter", required: true }] })],
    });
    await render({ automation: one, placementId: 1 });
    expect(container.querySelector<HTMLSelectElement>(".autostep__rows select")!.disabled).toBe(true);
  });

  it("fills an input from what fits, and empties it back to nothing reaching it", async () => {
    const one = detail({
      placements: [
        spot({ id: 1, exits: [{ id: 10, outputs: [{ name: "note", kind: "value", required: true }] }] }),
        spot({
          id: 2,
          name: "work",
          actionId: 5,
          stepId: 12,
          inputs: [{ name: "note", kind: "value", required: true }],
        }),
      ],
      wires: [
        { id: 3, fromId: 1, fromPortName: "note", toId: 2, toPortName: "note" },
      ],
    });
    await render({ automation: one, placementId: 2 });
    const wire = selects().find((s) => [...s.options].some((o) => o.textContent?.includes("note")))!;
    await act(async () => {
      wire.value = "";
      wire.dispatchEvent(new Event("change", { bubbles: true }));
    });
    expect(hoisted.clearWire).toHaveBeenCalledWith(3);
  });

  it("draws the error way out last, and offers neither a rename nor a delete on it", async () => {
    await render({ automation: detail(), placementId: 1 });
    const ways = [...container.querySelectorAll(".autostep__exits li")];
    expect(ways).toHaveLength(2);
    expect(ways[0]!.textContent).toContain(t("auto.step.exitUnnamed"));
    expect(ways[1]!.className).toContain("autostep__exiterr");
    expect(ways[1]!.textContent).toContain(t("auto.pic.errorExit"));
    // It takes an edge like any other way out — what it does not take is a rename or a delete.
    expect(ways[1]!.querySelectorAll("input, button")).toHaveLength(0);
    expect(nextFor(t("auto.pic.errorExit"))).toBeDefined();
  });






  /// Where a run opens is the automation's, not the spot's — so the tick box writes on the
  /// definition, and unticking it leaves the automation with no entry at all rather than refusing.
  it("names this spot as where a run opens, and gives the entry back", async () => {
    await render({ automation: detail({ entryPlacementId: undefined }), placementId: 1 });
    const entry = checkFor(t("auto.step.entry"));
    expect(entry.checked).toBe(false);
    await act(async () => entry.click());
    expect(hoisted.setEntry).toHaveBeenCalledWith(7, 1);

    await render({ automation: detail(), placementId: 1 });
    await act(async () => checkFor(t("auto.step.entry")).click());
    expect(hoisted.setEntry).toHaveBeenCalledWith(7, null);
  });

  /// One way out decides one thing, so the row writes the one edge on it: adding where nothing was
  /// said, changing the one that is there, and taking it away for "nothing said yet".
  it("says what happens after a way out, changes it, and takes it back", async () => {
    await render({ automation: detail(), placementId: 1 });
    await pick(nextFor(t("auto.step.exitUnnamed")), "done");
    expect(hoisted.addEdge).toHaveBeenCalledWith(
      "automation",
      { boxId: 1, exitName: undefined },
      { ends: "done" },
    );

    const said = detail({
      placements: [spot(), spot({ id: 2, name: "Do it" })],
      edges: [{ id: 8, fromId: 1, ends: "done" }],
    });
    await render({ automation: said, placementId: 1 });
    await pick(nextFor(t("auto.step.exitUnnamed")), "go:2");
    expect(hoisted.editEdge).toHaveBeenCalledWith(8, { ends: "go", to: 2 });

    await render({ automation: said, placementId: 1 });
    await pick(nextFor(t("auto.step.exitUnnamed")), "");
    expect(hoisted.removeEdge).toHaveBeenCalledWith(8);
  });

  /// The boxes are offered as the picture numbers them, under the heading of what picking one does,
  /// and the one this way out leaves is not among them. A line back up the picture says so.
  it("offers the other spots numbered as the picture draws them, and says which go back", async () => {
    const three = detail({
      placements: [spot(), spot({ id: 2, name: "Do it" }), spot({ id: 3, name: "Check it" })],
      edges: [
        { id: 8, fromId: 1, ends: "go", toId: 2 },
        { id: 9, fromId: 2, ends: "go", toId: 3 },
      ],
    });
    await render({ automation: three, placementId: 2 });
    const next = nextFor(t("auto.step.exitUnnamed"));
    const groups = [...next.querySelectorAll("optgroup")].map((one) => one.label);
    expect(groups).toEqual([t("auto.step.nextGroupPlacement"), t("auto.step.nextGroupEnd")]);
    const offered = [...next.querySelectorAll("optgroup")[0]!.querySelectorAll("option")].map(
      (one) => one.textContent,
    );
    expect(offered).toEqual([
      t("auto.step.nextGoBack").replace("{name}", "1. Take the next task"),
      t("auto.step.nextGo").replace("{name}", "3. Check it"),
    ]);
  });

  /// The limit caps a loop, so it is drawn on a line that goes back within one task and nowhere
  /// else: a line on down is taken once, and an edge that closes the task or stops the run carries
  /// none, which core refuses.
  it("writes the limit of a line that goes back, and draws none on one that goes on", async () => {
    const two = [spot({ exits: [{ id: 10, outputs: [] }] }), spot({ id: 2, name: "Do it" })];
    const looping = detail({
      placements: two,
      edges: [
        { id: 7, fromId: 1, ends: "go", toId: 2 },
        { id: 8, fromId: 2, ends: "go", toId: 1, maxTimes: 10 },
      ],
    });
    await render({ automation: looping, placementId: 2 });
    const limit = boxes().find((b) => b.type === "number")!;
    expect(limit.value).toBe("10");
    await typeInto(limit, "");
    await act(async () => limit.dispatchEvent(new FocusEvent("focusout", { bubbles: true })));
    expect(hoisted.editEdge).toHaveBeenCalledWith(8, { maxTimes: null });

    // Core writes the standing limit on every line that goes on to a box, the ones going on down too.
    await render({
      automation: detail({ ...looping, edges: [{ ...looping.edges[0]!, maxTimes: 10 }] }),
      placementId: 1,
    });
    expect(boxes().some((b) => b.type === "number")).toBe(false);

    await render({
      automation: detail({ edges: [{ id: 8, fromId: 1, ends: "done" }] }),
      placementId: 1,
    });
    expect(boxes().some((b) => b.type === "number")).toBe(false);
  });

  /// Going back to the spot that takes a task starts the next task rather than trying this one
  /// again, so there is no loop to cap there.
  it("draws no limit on a line back to the spot that takes the next task", async () => {
    const taking = spot({
      exits: [{ id: 10, outputs: [{ name: "task", kind: "task_take", required: true }] }],
    });
    await render({
      automation: detail({
        placements: [taking, spot({ id: 2, name: "Do it" })],
        edges: [
          { id: 7, fromId: 1, ends: "go", toId: 2 },
          { id: 8, fromId: 2, ends: "go", toId: 1 },
        ],
      }),
      placementId: 2,
    });
    expect(boxes().some((b) => b.type === "number")).toBe(false);
  });

  /// The panel is drawn from the spot, so the screen has to be told to stop showing it — and what
  /// stays behind is the library action, which outlives any one picture.
  it("takes the spot off and tells the screen there is nothing left to draw", async () => {
    const onRemoved = vi.fn();
    await render({ automation: detail(), placementId: 1, onRemoved });
    const press = [...container.querySelectorAll<HTMLButtonElement>("button")].find(
      (one) => one.textContent === t("auto.step.placementRemove"),
    )!;
    await act(async () => press.click());
    expect(hoisted.removePlacement).toHaveBeenCalledWith(1);
    expect(onRemoved).toHaveBeenCalled();
  });
});
