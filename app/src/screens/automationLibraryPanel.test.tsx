// @vitest-environment jsdom
// The library in the build screen's panel (`AMB-T-5360`).
//
// What these guard: **both reaches are offered, each under its own head**; **the box narrows by what
// the name and the note say, and a group with nothing left is not drawn at all**; **a row opens in
// place with what it receives and its ways out, the error one included**, and the press that places
// it, and that
// press **goes through the door the target names** — a line, or a picture with none yet; **a refusal
// is drawn and leaves the panel standing**, rather than closing on a placement that did not happen;
// and **making one here is the list's last row, named after what was typed**, and is handed back to
// the screen with that name, which opens the dialog.
//
// And the built-ins (`AMB-D-964`): **they come third under their own head, and not at all while the
// build carries none**; **a picked one shows what it does, and declares it the way an action of one's
// own does**; and **placing one names it by
// its key** through the built-in's own doors, on a line or on an empty picture.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AutomationActionCardDto, AutomationBuiltinDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  actions: [] as AutomationActionCardDto[],
  detail: null as unknown,
  builtins: [] as AutomationBuiltinDto[],
  insert: vi.fn((..._args: unknown[]) => Promise.resolve()),
  place: vi.fn((..._args: unknown[]) => Promise.resolve()),
  insertBuiltin: vi.fn((..._args: unknown[]) => Promise.resolve()),
  placeBuiltin: vi.fn((..._args: unknown[]) => Promise.resolve()),
}));

vi.mock("../core/automations", () => ({
  useAutomationActions: () => hoisted.actions,
  // What an opened row reads.
  useAutomationAction: () => hoisted.detail,
  insertAutomationAction: hoisted.insert,
  placeAutomationAction: hoisted.place,
  useAutomationBuiltins: () => hoisted.builtins,
  insertAutomationBuiltin: hoisted.insertBuiltin,
  placeAutomationBuiltin: hoisted.placeBuiltin,
}));

import { t, tf } from "../core/i18n";
import { AutomationLibraryPanel, type PlaceTarget } from "./AutomationLibraryPanel";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
const placed = vi.fn();
const make = vi.fn();

function card(over: Partial<AutomationActionCardDto> & { id: number; name: string }): AutomationActionCardDto {
  return { note: "", steps: 1, global: false, usedBy: 0, ...over };
}

async function render(target: PlaceTarget = { edgeId: 9 }) {
  await act(async () => {
    root.render(
      createElement(AutomationLibraryPanel, {
        target,
        projectId: 1,
        where: { box: "Take a task", exit: "taken", next: "Build it" },
        onPlaced: placed,
        onMake: make,
      }),
    );
  });
}

const rows = () => [...container.querySelectorAll(".autolib__row")].map((one) => one.textContent ?? "");
function button(label: string): HTMLButtonElement {
  const found = [...container.querySelectorAll("button")].find((b) => b.textContent?.includes(label));
  if (!found) throw new Error(`no button labelled ${label}`);
  return found;
}

/** Type into a controlled box the way React hears it — its own value tracker has to be moved. */
async function typeInto(box: HTMLInputElement, value: string) {
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(box, value);
    box.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  hoisted.actions = [
    card({ id: 4, name: "Review", note: "reads the draft back" }),
    card({ id: 5, name: "Publish", global: true, usedBy: 2 }),
  ];
  hoisted.detail = null;
  hoisted.insert.mockClear();
  hoisted.insert.mockResolvedValue(undefined);
  hoisted.place.mockClear();
  placed.mockClear();
  make.mockClear();
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("where the pick goes", () => {
  it("draws the box before it, the way out it hangs on, a dashed here and the box after it, rather than a sentence", async () => {
    await render();
    const where = container.querySelector(".wheremark")!;
    expect([...where.querySelectorAll(".wheremark__box")].map((one) => one.textContent)).toEqual([
      "Take a task",
      "Build it",
    ]);
    expect(where.querySelector(".actport--exit")?.textContent).toBe("taken");
    expect(where.querySelector(".wheremark__here")?.textContent).toBe(t("auto.lib.here"));
  });
});

describe("the library in the panel", () => {
  it("lists this project's and the device's under their own heads", async () => {
    await render();
    const heads = [...container.querySelectorAll(".autolib__head")].map((one) => one.textContent);
    expect(heads).toEqual([t("auto.actions.reachProject"), t("auto.actions.reachGlobal")]);
    const groups = [...container.querySelectorAll(".autolib__group")];
    expect(groups[0]!.textContent).toContain("Review");
    expect(groups[1]!.textContent).toContain("Publish");
  });

  it("narrows by what the name and the note say, and leaves out a group with nothing left", async () => {
    await render();
    await typeInto(container.querySelector<HTMLInputElement>("input")!, "draft");
    expect(rows().some((one) => one.includes("Review"))).toBe(true);
    expect(rows().some((one) => one.includes("Publish"))).toBe(false);
    const heads = [...container.querySelectorAll(".autolib__head")].map((one) => one.textContent);
    expect(heads).toEqual([t("auto.actions.reachProject")]);
  });

  it("leaves only the row that makes one when nothing matches", async () => {
    await render();
    await typeInto(container.querySelector<HTMLInputElement>("input")!, "nowhere");
    expect(container.querySelectorAll(".autolib__group")).toHaveLength(0);
    expect(container.querySelector(".autolib__make")?.textContent).toBe(
      tf("auto.lib.makeNamed", { name: "nowhere" }),
    );
  });

  it("opens a row with what it receives and its ways out, the error one last", async () => {
    hoisted.detail = {
      id: 4,
      name: "Review",
      note: "reads the draft back",
      global: false,
      usedBy: 0,
      inputs: [{ name: "worktree", kind: "file", required: true }],
      settings: [{ name: "depth", kind: "text", required: false }],
      exits: [
        { id: 1, name: "passed", outputs: [] },
        { id: 2, name: "*", outputs: [] },
      ],
    };
    await render();
    await act(async () => { button("Review").click(); });
    const picked = container.querySelector(".autolib__picked")!;
    const keys = [...picked.querySelectorAll(".actdecl__key")].map((one) => one.textContent);
    expect(keys).toEqual([t("auto.pic.actionIn"), t("auto.step.exits")]);
    expect(picked.textContent).toContain("worktree");
    expect(picked.textContent).not.toContain("depth");
    const exits = [...picked.querySelectorAll(".actport--exit, .actport--error")].map((one) => one.textContent);
    expect(exits).toEqual(["passed", t("auto.pic.errorExit")]);
  });

  it("puts the picked action in on the line it was opened from", async () => {
    await render({ edgeId: 9 });
    await act(async () => { button("Review").click(); });
    await act(async () => { button(t("auto.pic.placeDo")).click(); });
    expect(hoisted.insert).toHaveBeenCalledWith(9, 4);
    expect(hoisted.place).not.toHaveBeenCalled();
    expect(placed).toHaveBeenCalledTimes(1);
  });

  it("places it on its own where the picture has no line yet", async () => {
    await render({ automationId: 7 });
    await act(async () => { button("Publish").click(); });
    await act(async () => { button(t("auto.pic.placeDo")).click(); });
    expect(hoisted.place).toHaveBeenCalledWith(7, 5);
    expect(hoisted.insert).not.toHaveBeenCalled();
  });

  it("draws a refusal and stays open", async () => {
    hoisted.insert.mockRejectedValue({ code: "invalid", message_en: "that line is gone" });
    await render();
    await act(async () => { button("Review").click(); });
    await act(async () => { button(t("auto.pic.placeDo")).click(); });
    expect(container.textContent).toContain("that line is gone");
    expect(placed).not.toHaveBeenCalled();
  });

  it("hands making one back to the screen", async () => {
    await render();
    await act(async () => { button(t("auto.lib.makeNew")).click(); });
    expect(make).toHaveBeenCalledWith("");
  });

  it("hands the name typed along with it", async () => {
    await render();
    await typeInto(container.querySelector<HTMLInputElement>("input")!, " Lint ");
    await act(async () => { button(tf("auto.lib.makeNamed", { name: "Lint" })).click(); });
    expect(make).toHaveBeenCalledWith("Lint");
  });
});

describe("the built-ins in the panel", () => {
  const take: AutomationBuiltinDto = {
    key: "task_take",
    name: "Take a task",
    does: "reserves the first task the filter finds",
    settings: [{ name: "filter", kind: "taskfilter", required: true }],
    inputs: [],
    exits: [{ name: "taken", outputs: [{ name: "task", kind: "task_take", required: true }] }, { name: "完了", outputs: [] }],
    usedBy: 1,
  };

  beforeEach(() => {
    hoisted.builtins = [take];
    hoisted.insertBuiltin.mockClear();
    hoisted.placeBuiltin.mockClear();
  });
  afterEach(() => {
    hoisted.builtins = [];
  });

  it("come third, under their own head", async () => {
    await render();
    const heads = [...container.querySelectorAll(".autolib__head")].map((one) => one.textContent);
    expect(heads).toEqual([
      t("auto.actions.reachProject"),
      t("auto.actions.reachGlobal"),
      t("auto.actions.reachBuiltin"),
    ]);
    expect([...container.querySelectorAll(".autolib__group")][2]!.textContent).toContain("Take a task");
  });

  it("have no head while the build carries none", async () => {
    hoisted.builtins = [];
    await render();
    const heads = [...container.querySelectorAll(".autolib__head")].map((one) => one.textContent);
    expect(heads).not.toContain(t("auto.actions.reachBuiltin"));
  });

  it("show what a picked one does, and declare it the way an action of one's own does", async () => {
    await render();
    await act(async () => { button("Take a task").click(); });
    const picked = container.querySelector(".autolib__picked")!;
    expect(picked.textContent).toContain("reserves the first task the filter finds");
    const keys = [...picked.querySelectorAll(".actdecl__key")].map((one) => one.textContent);
    expect(keys).toEqual([t("auto.pic.actionIn"), t("auto.step.exits")]);
    const exits = [...picked.querySelectorAll(".actport--exit, .actport--error")].map((one) => one.textContent);
    expect(exits).toEqual(["takentask", "完了", t("auto.pic.errorExit")]);
  });

  it("put one in on the line by its key", async () => {
    await render({ edgeId: 9 });
    await act(async () => { button("Take a task").click(); });
    await act(async () => { button(t("auto.pic.placeDo")).click(); });
    expect(hoisted.insertBuiltin).toHaveBeenCalledWith(9, "task_take");
    expect(hoisted.insert).not.toHaveBeenCalled();
    expect(placed).toHaveBeenCalledTimes(1);
  });

  it("place one on its own where the picture has no line yet", async () => {
    await render({ automationId: 7 });
    await act(async () => { button("Take a task").click(); });
    await act(async () => { button(t("auto.pic.placeDo")).click(); });
    expect(hoisted.placeBuiltin).toHaveBeenCalledWith(7, "task_take");
    expect(hoisted.place).not.toHaveBeenCalled();
  });
});
