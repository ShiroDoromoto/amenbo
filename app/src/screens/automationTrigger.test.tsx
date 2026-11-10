// @vitest-environment jsdom
// An automation's trigger, written on its card. The trigger writes and the confirm are stubbed; every
// field, radio, chip and press runs for real.
//
// What these guard: **the card stands in the page, not in a dialog, its head naming the trigger,
// whether it is on and when it last filed**, with "On" switching it off at once; **it asks in three
// numbered parts**; **a new trigger, opened on none even with others written, starts as one that files every hour, and the save sends what was
// typed** as a trigger added; **the trigger it is opened on opens as it stands and goes back
// unchanged**, rewritten under its id, the others left alone; **"while fewer than N tasks match"
// always carries its condition, and "when a script says so" its script**, while with the other kinds
// each is sent only when asked for; **the note, the list and the preview say what will be filed** —
// its title, role, who works it and the trigger's name, **a trigger that last could not start a run
// saying under its line when and why**, as could not file or could not start by the role it is toward;
// **a refusal is drawn and the card stays open**; **"Remove trigger" takes that one away by its id**;
// and **the condition is picked as a status, a role and a count**, a filter the boxes cannot show
// kept as it was written, with how many tasks match now and what the next look does said under it;
// and **"Try it now" tries what is typed, unsaved**, what it says standing in the preview until
// something is typed again, the button held while it runs and the try stopped when the card closes;
// **fixed content carries data and files**, and **the role's way of starting decides what is asked** —
// a task filed gets a classification and a priority, the save held back while a required axis is
// empty, and a run started gets a key, its own words in every part, and no classification.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AutomationDetailDto, AutomationTriggerDto, DimensionDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  add: vi.fn(),
  update: vi.fn(),
  remove: vi.fn(),
  enable: vi.fn(),
  confirm: vi.fn(),
  count: vi.fn(),
  try: vi.fn(),
  stop: vi.fn(),
  axes: vi.fn(),
  /** The project's axes, as the snapshot holds them. */
  dimensions: [] as DimensionDto[],
}));

vi.mock("../core/automations", () => ({
  addAutomationTrigger: hoisted.add,
  updateAutomationTrigger: hoisted.update,
  removeAutomationTrigger: hoisted.remove,
  setAutomationTriggerEnabled: hoisted.enable,
  useTriggerCount: hoisted.count,
  tryAutomationTrigger: hoisted.try,
  stopTriggerTry: hoisted.stop,
  useTriggerAxesEmpty: hoisted.axes,
}));
vi.mock("../core/dialog", () => ({ confirmDialog: hoisted.confirm }));
vi.mock("../core/snapshot", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../core/snapshot")>()),
  getSnapshot: () => ({ projects: [{ id: 1, dimensions: hoisted.dimensions }] }),
}));

import { dueLabel, errText, priorityLabel, t, tf } from "../core/i18n";
import { en } from "../core/i18n/locales/en";
import { filterValueLabel } from "./automationParts";
import { AutomationTrigger, formOf, inputOf, joinFilter, nowThen, splitFilter, startsRun } from "./AutomationTrigger";
import { RoleIcon } from "./AutomationRoles";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

function detail(...triggers: AutomationTriggerDto[]): AutomationDetailDto {
  return {
    id: 7,
    projectId: 1,
    name: "Morning round",
    notes: "",
    archived: false,
    triggers,
    roles: [
      { id: 1, name: "Alice", headcount: 1, startsBy: "task", passes: [] },
      { id: 2, name: "Bob", headcount: 1, startsBy: "task", passes: [] },
      { id: 3, name: "Carol", headcount: 2, startsBy: "trigger", passes: [] },
    ],
    placements: [],
    edges: [],
    wires: [],
    maxHeadcount: 10,
    failureValues: [],
    heldBy: [],
    unsaved: false,
    saveBlocks: [],
  };
}

const written: AutomationTriggerDto = {
  id: 31,
  enabled: true,
  name: "Top up the reviews",
  roleId: 2,
  when: { kind: "idle" },
  condition: { filter: "status:todo", below: 3 },
  task: { kind: "file", path: "/Users/alice/next.md" },
  values: [],
  nextAt: "2026-11-07T00:00:00Z",
};

const page = () => document.body;

/** The card, opened on `trigger`, or on a new one without it. */
async function draw(
  automation: AutomationDetailDto,
  trigger?: AutomationTriggerDto,
  onSaved = vi.fn(),
  onClose = vi.fn(),
) {
  await act(async () => root.render(createElement(AutomationTrigger, { automation, trigger, onSaved, onClose })));
  return { onSaved, onClose };
}

const buttonNamed = (words: string) =>
  [...page().querySelectorAll<HTMLButtonElement>("button")].find((one) => one.textContent === words);

async function press(button: HTMLElement | undefined) {
  expect(button).toBeDefined();
  await act(async () => {
    button!.click();
    // The press may wait on the confirm and then on the write; let both settle.
    await new Promise((settle) => setTimeout(settle, 0));
  });
}

/** The box a label names, by the words on its label — before the mark saying whether it is required
 *  — or its `aria-label`. */
function box(words: string): HTMLInputElement | HTMLTextAreaElement {
  const named = page().querySelector<HTMLInputElement>(`[aria-label="${words}"]`);
  if (named !== null) return named;
  const label = [...page().querySelectorAll("label")].find(
    (one) => one.textContent === words || (one.firstChild?.nodeType === Node.TEXT_NODE && one.firstChild.textContent === words),
  );
  const forId = label?.getAttribute("for");
  const inside =
    forId === null || forId === undefined
      ? label?.querySelector<HTMLInputElement | HTMLTextAreaElement>("input, textarea")
      : document.getElementById(forId);
  expect(inside).toBeTruthy();
  return inside as HTMLInputElement | HTMLTextAreaElement;
}

/** The radio button whose words are these. */
function radio(words: string): HTMLInputElement | undefined {
  return [...page().querySelectorAll("label")]
    .find((one) => one.textContent === words)
    ?.querySelector<HTMLInputElement>("input[type=radio]") ?? undefined;
}

async function type(control: HTMLInputElement | HTMLTextAreaElement, value: string) {
  await act(async () => {
    const proto = control instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
    Object.getOwnPropertyDescriptor(proto, "value")!.set!.call(control, value);
    control.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

/** Pick `value` in the select a label names. */
async function pick(words: string, value: string) {
  const select = page().querySelector<HTMLSelectElement>(`select[aria-label="${words}"]`);
  expect(select).not.toBeNull();
  await act(async () => {
    select!.value = value;
    select!.dispatchEvent(new Event("change", { bubbles: true }));
  });
}

const selectNamed = (words: string) => page().querySelector<HTMLSelectElement>(`select[aria-label="${words}"]`);

/** The checkbox on the line whose words are these. */
function tick(words: string): HTMLInputElement | undefined {
  return [...page().querySelectorAll("label")]
    .find((one) => one.textContent === words)
    ?.querySelector<HTMLInputElement>("input[type=checkbox]") ?? undefined;
}

const previewTitle = () => page().querySelector("[data-preview=title]")?.textContent;
const previewLine = () => page().querySelector("[data-preview=line]")?.textContent;

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  for (const write of [hoisted.add, hoisted.update, hoisted.remove, hoisted.enable]) {
    write.mockReset();
    write.mockResolvedValue(undefined);
  }
  hoisted.confirm.mockReset();
  hoisted.confirm.mockResolvedValue(true);
  hoisted.count.mockReset();
  hoisted.count.mockReturnValue({ count: 1, error: undefined });
  hoisted.try.mockReset();
  hoisted.stop.mockReset();
  hoisted.stop.mockResolvedValue(undefined);
  hoisted.axes.mockReset();
  hoisted.axes.mockReturnValue([]);
  hoisted.dimensions = [];
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the form and what it sends", () => {
  it("starts a new trigger as one that files fixed content every hour, with no role", () => {
    expect(inputOf(formOf(undefined))).toEqual({
      name: "",
      roleId: undefined,
      when: { kind: "every", minutes: 60 },
      condition: undefined,
      script: undefined,
      task: { kind: "fixed", title: "", notes: "" },
    });
  });

  it("sends a trigger already written back as it was", () => {
    expect(inputOf(formOf(written))).toEqual({
      name: written.name,
      roleId: 2,
      when: written.when,
      condition: written.condition,
      task: written.task,
    });
  });

  it("always sends the condition of a tasks trigger and the script of a script one", () => {
    const form = formOf(undefined);
    expect(inputOf({ ...form, when: "tasks", below: "2" }).condition).toEqual({
      filter: "status:todo",
      roleId: undefined,
      below: 2,
    });
    const judge = { program: "/usr/bin/python3", args: "judge.py\n\n--quiet", minutes: "" };
    expect(inputOf({ ...form, when: "script", judge }).script).toEqual({
      program: "/usr/bin/python3",
      args: ["judge.py", "--quiet"],
      timeoutMinutes: undefined,
    });
    expect(inputOf({ ...form, when: "every", judge }).script).toBeUndefined();
    expect(inputOf({ ...form, when: "every", asked: true, judge }).script?.program).toBe("/usr/bin/python3");
  });

  it("splits a filter into its status and the rest, and joins them back", () => {
    expect(splitFilter("status:todo")).toEqual({ status: "todo", rest: "" });
    expect(splitFilter(" dim:area:ui  status:todo,in_progress ")).toEqual({
      status: "todo,in_progress",
      rest: "dim:area:ui",
    });
    expect(splitFilter("")).toEqual({ status: "", rest: "" });
    expect(joinFilter("todo", "")).toBe("status:todo");
    expect(joinFilter("", " dim:area:ui ")).toBe("dim:area:ui");
    expect(joinFilter("todo,in_progress", "dim:area:ui")).toBe("status:todo,in_progress dim:area:ui");
  });

  it("sends a filter the boxes cannot show back as it was, with the role it counts", () => {
    const kept = { ...written, condition: { filter: "status:todo,in_progress dim:area:ui", roleId: 1, below: 4 } };
    expect(inputOf(formOf(kept)).condition).toEqual({
      filter: "status:todo,in_progress dim:area:ui",
      roleId: 1,
      below: 4,
    });
  });

  it("says what the next look does, in the order core weighs it", () => {
    const form = { ...formOf(written), below: "3" };
    expect(nowThen(form, true, 1)).toBe(t("auto.trigger.nowIdle"));
    expect(nowThen(form, true, 3)).toBe(tf("auto.trigger.nowFull", { below: 3 }));
    expect(nowThen(form, false, 1)).toBe(t("auto.trigger.nowOff"));
    expect(nowThen({ ...form, when: "every" }, true, 0)).toBe(t("auto.trigger.nowEvery"));
    expect(nowThen({ ...form, when: "tasks" }, true, 0)).toBe(t("auto.trigger.nowTasks"));
    expect(nowThen({ ...form, asked: true }, true, 0)).toBe(t("auto.trigger.nowScript"));
    expect(nowThen({ ...form, below: "" }, true, 0)).toBe(t("auto.trigger.nowNoBelow"));
  });

  it("reads a script task's program back into its boxes", () => {
    const form = formOf({
      ...written,
      task: { kind: "script", script: { program: "/bin/sh", args: ["a.sh", "b"], timeoutMinutes: 30 } },
    });
    expect(inputOf(form).task).toEqual({
      kind: "script",
      script: { program: "/bin/sh", args: ["a.sh", "b"], timeoutMinutes: 30 },
    });
  });
});

describe("the card's head", () => {
  it("stands in the page rather than in a dialog of its own", async () => {
    await draw(detail(written), written);
    expect(page().querySelector("[role=dialog]")).toBeNull();
    expect(container.querySelector("form.trigcard")).not.toBeNull();
  });

  it("names the trigger in place, says it is on, and when it last filed", async () => {
    const filed = { ...written, lastFiledAt: "2026-11-07T12:40:00Z" };
    await draw(detail(filed), filed);
    const head = container.querySelector(".trigcard__head")!;
    expect((head.querySelector(`[aria-label="${t("auto.trigger.name")}"]`) as HTMLInputElement).value).toBe(
      "Top up the reviews",
    );
    expect(buttonNamed(t("auto.trigger.enabled"))?.getAttribute("aria-pressed")).toBe("true");
    expect(head.querySelector("[data-head=filed]")?.textContent).toBe(
      tf("auto.trigger.lastFiled", { at: dueLabel({ kind: "at", at: "2026-11-07T12:40:00Z" }) }),
    );
  });

  it("says a trigger that has filed nothing has not filed yet", async () => {
    await draw(detail(written), written);
    expect(page().querySelector("[data-head=filed]")?.textContent).toBe(t("auto.trigger.neverFiled"));
  });

  it("switches the trigger off at once, and a stopped one back on", async () => {
    await draw(detail(written), written);
    await press(buttonNamed(t("auto.trigger.enabled")));
    expect(hoisted.enable).toHaveBeenCalledWith(31, false);

    const stopped = { ...written, enabled: false };
    await draw(detail(stopped), stopped);
    expect(buttonNamed(t("auto.trigger.disabled"))?.getAttribute("aria-pressed")).toBe("false");
    await press(buttonNamed(t("auto.trigger.disabled")));
    expect(hoisted.enable).toHaveBeenLastCalledWith(31, true);
    expect(hoisted.update).not.toHaveBeenCalled();
  });

  it("gives a new trigger neither the chip nor the line, until it is saved", async () => {
    await draw(detail());
    expect(buttonNamed(t("auto.trigger.enabled"))).toBeUndefined();
    expect(page().querySelector("[data-head=filed]")).toBeNull();
    expect(box(t("auto.trigger.name")).getAttribute("placeholder")).toBe(t("auto.trigger.newName"));
  });

  it("asks in three numbered parts", async () => {
    await draw(detail());
    const parts = [...container.querySelectorAll(".trigcard__part")];
    expect(parts.map((one) => one.querySelector(".trigcard__no")?.textContent)).toEqual(["1", "2", "3"]);
    expect(parts.map((one) => one.querySelector("h3")?.textContent)).toEqual([
      t("auto.trigger.when"),
      t("auto.trigger.task"),
      t("auto.trigger.role"),
    ]);
  });
});

describe("the card", () => {
  it("saves what was typed through the trigger's write, then closes", async () => {
    const { onSaved, onClose } = await draw(detail());
    await type(box(t("auto.trigger.name")), "Morning check");
    await type(box(t("auto.trigger.minutes")), "1440");
    await type(box(t("auto.trigger.taskTitle")), "Check the reports");
    await type(box(t("auto.trigger.taskNotes")), "Read the new ones.");
    await pick(t("auto.trigger.role"), "1");
    await press(buttonNamed(t("auto.trigger.save")));

    expect(hoisted.add).toHaveBeenCalledWith(7, {
      name: "Morning check",
      roleId: 1,
      when: { kind: "every", minutes: 1440 },
      condition: undefined,
      script: undefined,
      task: { kind: "fixed", title: "Check the reports", notes: "Read the new ones." },
    });
    expect(onClose).toHaveBeenCalled();
    expect(onSaved).toHaveBeenCalled();
  });

  it("opens a trigger already written as it stands", async () => {
    await draw(detail(written), written);
    expect((box(t("auto.trigger.name")) as HTMLInputElement).value).toBe("Top up the reviews");
    expect(radio(t("auto.trigger.whenIdle"))?.checked).toBe(true);
    expect(radio(t("auto.trigger.taskFile"))?.checked).toBe(true);
    expect(tick(t("auto.trigger.counted"))?.checked).toBe(true);
    expect(selectNamed(t("auto.trigger.condStatusPick"))?.value).toBe("todo");
    expect(selectNamed(t("auto.trigger.condRolePick"))?.value).toBe("");
    expect((box(t("auto.trigger.below")) as HTMLInputElement).value).toBe("3");
    expect((box(t("auto.trigger.taskPath")) as HTMLInputElement).value).toBe("/Users/alice/next.md");
    expect(selectNamed(t("auto.trigger.role"))?.value).toBe("2");
  });

  it("picks the role from a list, the picked role's icon before it", async () => {
    const automation = detail();
    await draw(automation);
    const roles = [...selectNamed(t("auto.trigger.role"))!.options].map((one) => one.textContent);
    expect(roles).toEqual(["Alice", "Bob", "Carol", t("auto.trigger.roleNone")]);
    const row = () => selectNamed(t("auto.trigger.role"))!.parentElement!;
    const iconOf = (name: string) => {
      const drawn = document.createElement("div");
      const role = automation.roles.find((one) => one.name === name)!;
      const inner = createRoot(drawn);
      act(() => inner.render(createElement(RoleIcon, { role, size: 16 })));
      const html = drawn.innerHTML;
      act(() => inner.unmount());
      return html;
    };

    // None is picked on a new trigger: no icon, an empty space of the icon's size in its place.
    expect(selectNamed(t("auto.trigger.role"))?.value).toBe("");
    expect(row().querySelector(".identicon")).toBeNull();
    expect(row().querySelector(".trigcard__noicon")).not.toBeNull();

    await pick(t("auto.trigger.role"), "1");
    expect(row().querySelector(".identicon")?.outerHTML).toBe(iconOf("Alice"));
    expect(row().querySelector(".trigcard__noicon")).toBeNull();

    await pick(t("auto.trigger.role"), "2");
    expect(row().querySelector(".identicon")?.outerHTML).toBe(iconOf("Bob"));
  });

  it("keeps a role taken away since picked under its number, with no icon", async () => {
    const gone: AutomationTriggerDto = { ...written, roleId: 9 };
    await draw(detail(gone), gone);
    const select = selectNamed(t("auto.trigger.role"))!;
    expect(select.value).toBe("9");
    const roles = [...select.options].map((one) => one.textContent);
    expect(roles).toEqual(["Alice", "Bob", "Carol", "9", t("auto.trigger.roleNone")]);
    expect(select.parentElement!.querySelector(".identicon")).toBeNull();
  });

  it("rewrites the trigger it is opened on under its id, leaving the others", async () => {
    const other: AutomationTriggerDto = { ...written, id: 32, name: "Daily check" };
    await draw(detail(written, other), other);
    expect((box(t("auto.trigger.name")) as HTMLInputElement).value).toBe("Daily check");
    await type(box(t("auto.trigger.name")), "Daily check again");
    await press(buttonNamed(t("auto.trigger.save")));
    expect(hoisted.update).toHaveBeenCalledWith(32, expect.objectContaining({ name: "Daily check again" }));
    expect(hoisted.add).not.toHaveBeenCalled();
  });

  it("keeps the condition on for a tasks trigger, with no box to turn it off", async () => {
    await draw(detail());
    expect(selectNamed(t("auto.trigger.condStatusPick"))).toBeNull();
    await press(radio(t("auto.trigger.whenTasks")));
    expect(tick(t("auto.trigger.counted"))).toBeUndefined();
    await pick(t("auto.trigger.condStatusPick"), "in_progress");
    await type(box(t("auto.trigger.below")), "5");
    await press(buttonNamed(t("auto.trigger.save")));
    expect(hoisted.add.mock.calls[0]?.[1]).toMatchObject({
      when: { kind: "tasks" },
      condition: { filter: "status:in_progress", below: 5 },
    });
  });

  it("picks the condition's status and role from the automation's own, and sends both", async () => {
    await draw(detail());
    await press(radio(t("auto.trigger.whenIdle")));
    await press(tick(t("auto.trigger.counted")));
    const roles = [...selectNamed(t("auto.trigger.condRolePick"))!.options].map((one) => one.textContent);
    expect(roles).toEqual(["Alice", "Bob", "Carol", t("auto.trigger.anyRole")]);
    const statuses = [...selectNamed(t("auto.trigger.condStatusPick"))!.options].map((one) => one.value);
    expect(statuses).toEqual(["todo", "in_progress", "blocked", "done", "rejected", ""]);
    await pick(t("auto.trigger.condRolePick"), "1");
    await type(box(t("auto.trigger.below")), "3");
    await type(box(t("auto.trigger.taskTitle")), "Check the reports");
    await press(buttonNamed(t("auto.trigger.save")));
    expect(hoisted.add.mock.calls[0]?.[1]).toMatchObject({
      when: { kind: "idle" },
      condition: { filter: "status:todo", roleId: 1, below: 3 },
    });
  });

  it("keeps a filter the boxes cannot show, says so, and sends it back unchanged", async () => {
    const kept = { ...written, condition: { filter: "status:todo,in_progress dim:area:ui", below: 3 } };
    await draw(detail(kept), kept);
    const status = selectNamed(t("auto.trigger.condStatusPick"))!;
    expect(status.value).toBe("todo,in_progress");
    expect(status.selectedOptions[0]?.textContent).toBe(
      `${filterValueLabel("status", "todo")}/${filterValueLabel("status", "in_progress")}`,
    );
    expect(page().querySelector("[data-cond=kept]")?.textContent).toBe(
      tf("auto.trigger.condKept", { rest: "dim:area:ui" }),
    );
    await press(buttonNamed(t("auto.trigger.save")));
    expect(hoisted.update).toHaveBeenCalledWith(
      31,
      expect.objectContaining({ condition: { filter: "status:todo,in_progress dim:area:ui", roleId: undefined, below: 3 } }),
    );
  });

  it("says how many tasks match now and what the next look does, counted as typed", async () => {
    await draw(detail(written), written);
    expect(hoisted.count).toHaveBeenLastCalledWith(7, "status:todo", null);
    expect(page().querySelector("[data-cond=now]")?.textContent).toBe(
      tf("auto.trigger.now", { count: 1, then: t("auto.trigger.nowIdle") }),
    );
    hoisted.count.mockReturnValue({ count: 3, error: undefined });
    await pick(t("auto.trigger.condRolePick"), "2");
    expect(hoisted.count).toHaveBeenLastCalledWith(7, "status:todo", 2);
    expect(page().querySelector("[data-cond=now]")?.textContent).toBe(
      tf("auto.trigger.now", { count: 3, then: tf("auto.trigger.nowFull", { below: 3 }) }),
    );
  });

  it("says when core cannot count the condition", async () => {
    hoisted.count.mockReturnValue({ count: null, error: new Error("the filter does not read") });
    await draw(detail(written), written);
    expect(page().querySelector("[data-cond=now]")?.textContent).toBe(
      tf("auto.trigger.nowRefused", { why: errText(new Error("the filter does not read")) }),
    );
  });

  it("asks for a script's boxes when the script decides", async () => {
    await draw(detail());
    expect(page().querySelector(`[aria-label="${t("auto.step.program")}"]`)).toBeNull();
    await press(radio(t("auto.trigger.whenScript")));
    expect(tick(t("auto.trigger.asked"))).toBeUndefined();
    await type(box(t("auto.step.program")), "/usr/bin/python3");
    await press(buttonNamed(t("auto.trigger.save")));
    expect(hoisted.add.mock.calls[0]?.[1]).toMatchObject({
      when: { kind: "script" },
      script: { program: "/usr/bin/python3", args: [] },
    });
  });

  it("says what is recorded, under the trigger's name", async () => {
    await draw(detail(written), written);
    expect(page().querySelector("[data-note=recorded]")?.textContent).toBe(
      tf("auto.trigger.recorded", { name: "Top up the reviews" }),
    );
  });

  it("previews the task it files: title, role, who works it and the trigger", async () => {
    await draw(detail());
    expect(previewTitle()).toBe(t("auto.trigger.previewNoTitle"));
    expect(previewLine()).toBe(tf("auto.trigger.previewLine", { role: t("auto.trigger.roleNone"), trigger: "" }));

    await type(box(t("auto.trigger.name")), "Morning check");
    await type(box(t("auto.trigger.taskTitle")), "Check the reports");
    await pick(t("auto.trigger.role"), "2");
    expect(previewTitle()).toBe("Check the reports");
    expect(previewLine()).toBe(tf("auto.trigger.previewLine", { role: "Bob", trigger: "Morning check" }));

    await press(radio(t("auto.trigger.taskFile")));
    expect(previewTitle()).toBe(t("auto.trigger.previewFromFile"));
    await press(radio(t("auto.trigger.taskScript")));
    expect(previewTitle()).toBe(t("auto.trigger.previewFromScript"));
  });

  it("lists the automation's triggers beside the card, a line each", async () => {
    const other: AutomationTriggerDto = {
      ...written,
      id: 32,
      name: "Daily check",
      roleId: undefined,
      when: { kind: "every", minutes: 1440 },
      condition: undefined,
      task: { kind: "fixed", title: "Look around", notes: "" },
    };
    await draw(detail(written, other));
    const lines = [...page().querySelectorAll("[data-list=triggers] li")].map((one) => one.textContent);
    expect(lines).toHaveLength(2);
    expect(lines[0]).toContain("Top up the reviews");
    expect(lines[0]).toContain("/Users/alice/next.md");
    expect(lines[0]).toContain(tf("auto.side.triggerRole", { role: "Bob" }));
    expect(lines[1]).toContain("Daily check");
    expect(lines[1]).toContain(tf("auto.side.everyDays", { days: 1 }));
    expect(lines[1]).toContain(t("auto.side.triggerRoleNone"));
  });

  it("reads the condition's status and role by name in the trigger's line", async () => {
    const counted = { ...written, condition: { filter: "status:todo", roleId: 1, below: 3 } };
    await draw(detail(counted));
    const line = page().querySelector("[data-list=triggers] li")?.textContent ?? "";
    expect(line).toContain(
      tf("auto.side.belowOf", { what: `${filterValueLabel("status", "todo")} Alice`, below: 3 }),
    );
  });

  it("says under a trigger that could not file when and why, in red, the rest of its line kept", async () => {
    const lastRefused = {
      at: "2026-11-10T08:30:00Z",
      code: "invalid_trigger_file_missing",
      message_en: "There is no file at /Users/alice/next.md",
      fields: { path: "/Users/alice/next.md" },
      parts: [],
    };
    await draw(detail({ ...written, lastRefused }));
    const line = page().querySelector("[data-list=triggers] li")!;
    expect(line.classList.contains("is-refused")).toBe(true);
    expect([...line.children].map((one) => one.tagName)).toEqual(["B", "SPAN", "SPAN"]);
    expect(line.children[1].textContent).toContain(tf("auto.side.triggerRole", { role: "Bob" }));
    expect(line.querySelector(".trigstage__refused")?.textContent).toBe(
      tf("auto.trigger.refusedFiling", {
        at: dueLabel({ kind: "at", at: "2026-11-10T08:30:00Z" }),
        why: errText(lastRefused),
      }),
    );
    expect(errText(lastRefused)).toContain("/Users/alice/next.md");
  });

  it("says a trigger toward a role started by a trigger could not start, not file", async () => {
    const lastRefused = {
      at: "2026-11-10T09:00:00Z",
      code: "invalid_trigger_file_missing",
      message_en: "There is no file at /Users/alice/next.md",
      fields: { path: "/Users/alice/next.md" },
      parts: [],
    };
    const automation = detail({ ...written, roleId: 1, lastRefused });
    automation.roles[0] = { ...automation.roles[0], startsBy: "trigger" };
    await draw(automation);
    expect(page().querySelector("[data-list=triggers] .trigstage__refused")?.textContent).toBe(
      tf("auto.trigger.refusedStarting", {
        at: dueLabel({ kind: "at", at: "2026-11-10T09:00:00Z" }),
        why: errText(lastRefused),
      }),
    );
  });

  it("says nothing under a trigger that has not failed since it last started one", async () => {
    await draw(detail(written));
    const line = page().querySelector("[data-list=triggers] li")!;
    expect(line.classList.contains("is-refused")).toBe(false);
    expect(line.querySelector(".trigstage__refused")).toBeNull();
  });

  it("draws what core refused and stays open with what was typed", async () => {
    hoisted.add.mockRejectedValue(new Error("a trigger files every 1 minute or more"));
    const { onClose } = await draw(detail());
    await type(box(t("auto.trigger.minutes")), "0");
    await press(buttonNamed(t("auto.trigger.save")));
    expect(page().querySelector("[role=alert]")?.textContent).toContain("a trigger files every 1 minute or more");
    expect(onClose).not.toHaveBeenCalled();
    expect((box(t("auto.trigger.minutes")) as HTMLInputElement).value).toBe("0");
  });

  it("removes a trigger already written once the reader confirms", async () => {
    const { onClose } = await draw(detail(written), written);
    await press(buttonNamed(t("auto.trigger.remove")));
    expect(hoisted.confirm).toHaveBeenCalledWith(tf("auto.trigger.removeConfirm", { name: "Top up the reviews" }));
    expect(hoisted.remove).toHaveBeenCalledWith(31);
    expect(onClose).toHaveBeenCalled();
  });

  it("adds a new one when opened on none, even with others written", async () => {
    await draw(detail(written));
    expect((box(t("auto.trigger.name")) as HTMLInputElement).value).toBe("");
    await type(box(t("auto.trigger.taskTitle")), "Check the reports");
    await press(buttonNamed(t("auto.trigger.save")));
    expect(hoisted.add).toHaveBeenCalledWith(7, expect.objectContaining({ name: "" }));
    expect(hoisted.update).not.toHaveBeenCalled();
  });

  it("offers no removal while opened on a new one", async () => {
    await draw(detail(written));
    expect(buttonNamed(t("auto.trigger.remove"))).toBeUndefined();
  });
});

describe("trying it as it is typed", () => {
  const previewNotes = () => page().querySelector("[data-preview=notes]")?.textContent;

  it("tries what was typed without saving, and previews the task it would file", async () => {
    hoisted.try.mockResolvedValue({ kind: "files", title: "Read the new reports", notes: "Three came in.", data: null, key: null, files: [] });
    const { onClose } = await draw(detail());
    await press(radio(t("auto.trigger.taskFile")));
    await type(box(t("auto.trigger.taskPath")), "/Users/alice/next.md");
    await press(buttonNamed(t("auto.trigger.try")));

    expect(hoisted.try).toHaveBeenCalledWith(
      7,
      expect.objectContaining({ task: { kind: "file", path: "/Users/alice/next.md" } }),
      expect.any(Number),
    );
    expect(hoisted.add).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
    expect(previewTitle()).toBe("Read the new reports");
    expect(previewNotes()).toBe("Three came in.");

    await type(box(t("auto.trigger.taskPath")), "/Users/alice/other.md");
    expect(previewTitle()).toBe(t("auto.trigger.previewFromFile"));
    expect(previewNotes()).toBeUndefined();
  });

  it("says why it would file none", async () => {
    await draw(detail());
    await type(box(t("auto.trigger.taskTitle")), "Check the reports");
    const tried = [
      [{ kind: "condition" }, t("auto.trigger.triedCondition")],
      [{ kind: "program_said_no" }, t("auto.trigger.triedSaidNo")],
      [{ kind: "said_nothing", why: "the program ended with exit code 2" }, tf("auto.trigger.triedNothing", { why: "the program ended with exit code 2" })],
    ] as const;
    for (const [said, words] of tried) {
      hoisted.try.mockResolvedValueOnce(said);
      await press(buttonNamed(t("auto.trigger.try")));
      expect(previewTitle()).toBe(words);
      expect(previewNotes()).toBeUndefined();
    }
  });

  it("draws what core refused at the top, and the preview stays as typed", async () => {
    hoisted.try.mockRejectedValue(new Error("a trigger files every 1 minute or more"));
    await draw(detail());
    await type(box(t("auto.trigger.minutes")), "0");
    await type(box(t("auto.trigger.taskTitle")), "Check the reports");
    await press(buttonNamed(t("auto.trigger.try")));
    expect(page().querySelector("[role=alert]")?.textContent).toContain("a trigger files every 1 minute or more");
    expect(previewTitle()).toBe("Check the reports");
  });

  it("holds the button while it runs, and stops the try when the card closes", async () => {
    hoisted.try.mockReturnValue(new Promise(() => {}));
    await draw(detail());
    await press(buttonNamed(t("auto.trigger.try")));
    const running = buttonNamed(t("auto.trigger.trying"));
    expect(running?.disabled).toBe(true);
    const tag = hoisted.try.mock.calls[0][2] as number;

    act(() => root.unmount());
    expect(hoisted.stop).toHaveBeenCalledWith(tag);
    root = createRoot(container);
  });
});

describe("fixed content", () => {
  it("opens and sends back its data and files, and a key only where the trigger starts a run", () => {
    const fixed: AutomationTriggerDto = {
      ...written,
      task: { kind: "fixed", title: "Next boss theme", notes: "", data: '{"stage": 5}', files: ["/Users/alice/brief.md"], key: "stage-5" },
      values: [11],
      priority: "medium",
    };
    const form = formOf(fixed);
    expect(form).toMatchObject({ data: '{"stage": 5}', files: "/Users/alice/brief.md", key: "stage-5", values: [11], priority: "medium" });
    expect(inputOf(form)).toMatchObject({
      task: { kind: "fixed", title: "Next boss theme", notes: "", data: '{"stage": 5}', files: ["/Users/alice/brief.md"] },
      values: [11],
      priority: "medium",
    });
    expect(inputOf(form).task).not.toHaveProperty("key");
    const started = inputOf(form, true);
    expect(started.task).toMatchObject({ key: "stage-5" });
    expect(started).not.toHaveProperty("values");
    expect(started).not.toHaveProperty("priority");
  });

  it("sends the data as typed and the files one a line, the blank lines left out", async () => {
    await draw(detail());
    await type(box(t("auto.trigger.taskTitle")), "Next boss theme");
    await type(box(t("auto.trigger.taskData")), '{"bpm": 140}');
    await type(box(t("auto.trigger.taskFiles")), "/Users/alice/brief.md\n\n /Users/alice/notes.md \n");
    await press(buttonNamed(t("auto.trigger.save")));
    expect(hoisted.add.mock.calls[0]?.[1].task).toEqual({
      kind: "fixed",
      title: "Next boss theme",
      notes: "",
      data: '{"bpm": 140}',
      files: ["/Users/alice/brief.md", "/Users/alice/notes.md"],
    });
  });

  it("marks the title required where a task is filed, and every field optional where a run starts", async () => {
    await draw(detail());
    const mark = (words: string) =>
      [...page().querySelectorAll("label")].find((one) => one.firstChild?.textContent === words)?.lastChild?.textContent;
    expect(mark(t("auto.trigger.taskTitle"))).toBe(t("auto.side.required"));
    expect(mark(t("auto.trigger.taskNotes"))).toBe(t("auto.trigger.optional"));
    expect(mark(t("auto.trigger.taskData"))).toBe(t("auto.trigger.optional"));
    expect(mark(t("auto.trigger.taskFiles"))).toBe(t("auto.trigger.optional"));
    expect(mark(t("auto.trigger.taskKey"))).toBeUndefined();

    await pick(t("auto.trigger.role"), "3");
    expect(mark(t("auto.trigger.taskTitle"))).toBe(t("auto.trigger.optional"));
    expect(mark(t("auto.trigger.taskKey"))).toBe(t("auto.trigger.optional"));
  });

  it("previews the data and the names of the files attached", async () => {
    await draw(detail());
    await type(box(t("auto.trigger.taskTitle")), "Next boss theme");
    await type(box(t("auto.trigger.taskData")), '{"stage": 5}');
    await type(box(t("auto.trigger.taskFiles")), "/Users/alice/music/brief.md\n/Users/alice/music/ref.mp3");
    expect(page().querySelector("[data-preview=data]")?.textContent).toBe('{"stage": 5}');
    expect(page().querySelector("[data-preview=files]")?.textContent).toBe(
      tf("auto.trigger.previewFiles", { files: "brief.md, ref.mp3" }),
    );
    expect(page().querySelector("[data-preview=key]")).toBeNull();
  });

  it("previews what a try would hand on: data, files and key", async () => {
    hoisted.try.mockResolvedValue({ kind: "files", title: "issue #123", notes: "", data: '{"issue":123}', key: "issue-123", files: ["log.txt"] });
    await draw(detail());
    await pick(t("auto.trigger.role"), "3");
    await press(buttonNamed(t("auto.trigger.try")));
    expect(previewTitle()).toBe("issue #123");
    expect(page().querySelector("[data-preview=data]")?.textContent).toBe('{"issue":123}');
    expect(page().querySelector("[data-preview=files]")?.textContent).toBe(tf("auto.trigger.previewFiles", { files: "log.txt" }));
    expect(page().querySelector("[data-preview=key]")?.textContent).toBe(tf("auto.trigger.previewKey", { key: "issue-123" }));
  });
});

describe("what is handed on", () => {
  const options = () =>
    [...page().querySelectorAll(`[role=radiogroup][aria-label="${t("auto.trigger.taskPick")}"] label`)].map((one) => one.textContent);

  it("offers the deciding script's output only where a script decides", async () => {
    await draw(detail());
    expect(options()).toEqual([t("auto.trigger.taskFixed"), t("auto.trigger.taskFile"), t("auto.trigger.taskScript")]);
    await press(radio(t("auto.trigger.whenScript")));
    expect(options()).toContain(t("auto.trigger.taskAsked"));
    await press(radio(t("auto.trigger.taskAsked")));
    expect(page().querySelector("[data-hint=asked]")?.textContent).toBe(t("auto.trigger.askedHint"));
    await type(box(t("auto.step.program")), "/usr/bin/python3");
    await press(buttonNamed(t("auto.trigger.save")));
    expect(hoisted.add.mock.calls[0]?.[1]).toMatchObject({ when: { kind: "script" }, task: { kind: "asked" } });
  });

  it("describes version 2 of what a script and a file write", async () => {
    await draw(detail());
    await press(radio(t("auto.trigger.whenScript")));
    const codes = () => [...page().querySelectorAll(".trigcard__code")].map((one) => one.textContent);
    expect(codes()).toEqual(['{"version":2,"start":true,"title":"…","notes":"…","data":{…}}']);
    await pick(t("auto.trigger.role"), "3");
    expect(codes()[0]).toBe('{"version":2,"start":true,"title":"issue #123","notes":"…","data":{"issue":123},"key":"issue-123"}');
    await press(radio(t("auto.trigger.taskScript")));
    expect(codes()[1]).toBe('{"version":2,"title":"…","notes":"…","data":{…},"key":"…"}');
    await press(radio(t("auto.trigger.taskFile")));
    expect(codes()[1]).toContain('"files":["/…"]');
  });
});

describe("a trigger that files a task: its classification and priority", () => {
  const axis = (over: Partial<DimensionDto> & Pick<DimensionDto, "id" | "name" | "values">): DimensionDto => ({
    notes: "",
    cardinality: "single",
    role: "none",
    ordered: false,
    showOnCard: false,
    required: false,
    appliesTo: "both",
    sequential: false,
    ...over,
  });
  const value = (id: number, name: string, closed = false) => ({ id, name, closed });
  const frame = () => page().querySelector<HTMLElement>("[data-part=class]");
  const chip = (label: string) =>
    [...(frame()?.querySelectorAll<HTMLButtonElement>(".autostep__chip") ?? [])].find((one) => one.textContent === label);
  const priority = (label: string) =>
    [...page().querySelectorAll<HTMLButtonElement>(`[aria-label="${t("auto.trigger.priority")}"] button`)].find(
      (one) => one.textContent === label,
    );
  const saveButton = () => buttonNamed(t("auto.trigger.save"));

  beforeEach(() => {
    hoisted.dimensions = [
      axis({ id: 3, name: "Product", required: true, values: [value(11, "Game"), value(12, "Site")] }),
      axis({ id: 4, name: "Kind", required: true, values: [value(21, "Build"), value(22, "Old", true)] }),
    ];
  });

  it("asks a row per axis and the priority, under the role, and sends what is pressed", async () => {
    await draw(detail());
    expect(frame()?.textContent).toContain(t("auto.trigger.values"));
    expect(chip("Old")).toBeUndefined();
    await press(chip("Game"));
    await press(chip("Build"));
    expect(chip("Game")?.getAttribute("aria-pressed")).toBe("true");
    expect(hoisted.axes).toHaveBeenLastCalledWith(7, [11, 21]);
    expect(priority(t("detail.priorityNone"))?.getAttribute("aria-pressed")).toBe("true");
    await press(priority(priorityLabel("high")));
    expect(priority(priorityLabel("high"))?.getAttribute("aria-pressed")).toBe("true");
    await type(box(t("auto.trigger.taskTitle")), "Next boss theme");
    await press(saveButton());
    expect(hoisted.add.mock.calls[0]?.[1]).toMatchObject({ values: [11, 21], priority: "high" });
  });

  it("names each required axis core finds empty, and holds the save back until it has a value", async () => {
    hoisted.axes.mockReturnValue(["Kind"]);
    await draw(detail());
    expect(frame()?.querySelector("[role=status]")?.textContent).toBe(tf("auto.trigger.axisEmpty", { axis: "Kind" }));
    expect(saveButton()?.disabled).toBe(true);
    hoisted.axes.mockReturnValue([]);
    await press(chip("Build"));
    expect(frame()?.querySelector("[role=status]")).toBeNull();
    expect(saveButton()?.disabled).toBe(false);
  });

  it("keeps a closed value the trigger already names on its row", async () => {
    await draw(detail({ ...written, values: [22] }), { ...written, values: [22] });
    expect(chip("Old")?.getAttribute("aria-pressed")).toBe("true");
  });

  it("files a task aimed at no role as at a role started by a task", async () => {
    await draw(detail());
    expect(selectNamed(t("auto.trigger.role"))?.value).toBe("");
    expect(frame()).not.toBeNull();
    expect(page().querySelector("[data-role=starts]")).toBeNull();
    await pick(t("auto.trigger.role"), "1");
    expect(page().querySelector("[data-role=starts]")?.textContent).toBe(t("auto.build.roleStartsTask"));
  });
});

describe("a trigger aimed at a role started by a trigger", () => {
  const aimed: AutomationTriggerDto = {
    ...written,
    roleId: 3,
    when: { kind: "every", minutes: 60 },
    condition: undefined,
    task: { kind: "fixed", title: "", notes: "", key: "issue-123" },
    lastFiledAt: "2026-11-10T00:12:00Z",
  };

  it("knows a run is started only for a role started by a trigger", () => {
    const automation = detail();
    expect(startsRun(automation, 3)).toBe(true);
    expect(startsRun(automation, 1)).toBe(false);
    expect(startsRun(automation, null)).toBe(false);
    expect(startsRun(automation, 9)).toBe(false);
  });

  it("says it starts a run in every part, and when it last started one", async () => {
    await draw(detail(aimed), aimed);
    expect(page().querySelector(".trigstage > .autostep__hint")?.textContent).toBe(t("auto.trigger.leadStart"));
    expect(page().querySelector("[data-head=filed]")?.textContent).toBe(
      tf("auto.trigger.lastFiledStart", { at: dueLabel({ kind: "at", at: "2026-11-10T00:12:00Z" }) }),
    );
    const parts = [...container.querySelectorAll(".trigcard__part h3")].map((one) => one.textContent);
    expect(parts).toEqual([t("auto.trigger.whenStart"), t("auto.trigger.taskStart"), t("auto.trigger.role")]);
    expect(page().querySelector("[data-role=starts]")?.textContent).toBe(t("auto.build.roleStartsTrigger"));
    expect(page().querySelector("[data-note=starts]")?.textContent).toBe(tf("auto.trigger.startsNote", { role: "Carol" }));
    expect(page().querySelector("[data-hint=receive]")?.textContent).toBe(t("auto.trigger.receiveHint"));
    expect(page().querySelector("[data-note=recorded]")?.textContent).toBe(
      tf("auto.trigger.recordedStart", { name: "Top up the reviews" }),
    );
    expect(page().querySelector("[data-part=class]")).toBeNull();
  });

  it("says it has started none yet", async () => {
    const fresh = { ...aimed, lastFiledAt: undefined };
    await draw(detail(fresh), fresh);
    expect(page().querySelector("[data-head=filed]")?.textContent).toBe(t("auto.trigger.neverFiledStart"));
  });

  it("sends its key and no classification, even where one was pressed for another role", async () => {
    hoisted.dimensions = [
      {
        id: 3,
        name: "Product",
        notes: "",
        cardinality: "single",
        role: "none",
        ordered: false,
        showOnCard: false,
        required: true,
        appliesTo: "both",
        sequential: false,
        values: [{ id: 11, name: "Game", closed: false }],
      },
    ];
    hoisted.axes.mockReturnValue(["Product"]);
    await draw(detail(aimed), aimed);
    expect((box(t("auto.trigger.taskKey")) as HTMLInputElement).value).toBe("issue-123");
    expect(buttonNamed(t("auto.trigger.save"))?.disabled).toBe(false);
    await pick(t("auto.trigger.role"), "1");
    expect(buttonNamed(t("auto.trigger.save"))?.disabled).toBe(true);
    await pick(t("auto.trigger.role"), "3");
    await press(buttonNamed(t("auto.trigger.save")));
    const sent = hoisted.update.mock.calls[0]?.[1];
    expect(sent.task).toEqual({ kind: "fixed", title: "", notes: "", key: "issue-123" });
    expect(sent).not.toHaveProperty("values");
    expect(sent).not.toHaveProperty("priority");
  });

  it("says the key comes from the file or the script where they hand it on", async () => {
    await draw(detail(aimed), aimed);
    expect(page().querySelector("[data-key=from]")).toBeNull();
    await press(radio(t("auto.trigger.taskFile")));
    expect(page().querySelector("[data-key=from]")?.textContent).toContain(t("auto.trigger.keyFromFile"));
    await press(radio(t("auto.trigger.taskScript")));
    expect(page().querySelector("[data-key=from]")?.textContent).toContain(t("auto.trigger.keyFromScript"));
  });

  it("previews the run it starts, with its key and no one to work it", async () => {
    await draw(detail(aimed), aimed);
    expect([...page().querySelectorAll(".trigstage__notes h3")].map((one) => one.textContent)).toContain(
      t("auto.trigger.previewStart"),
    );
    expect(page().querySelector("[data-preview=key]")?.textContent).toBe(tf("auto.trigger.previewKey", { key: "issue-123" }));
    expect(previewLine()).toBe(tf("auto.trigger.previewLineStart", { role: "Carol", trigger: "Top up the reviews" }));
  });

  it("says what the next look starts, and why a try would start none", async () => {
    const form = { ...formOf(aimed), counted: true, below: "3" };
    expect(nowThen(form, true, 0, true)).toBe(t("auto.trigger.nowEveryStart"));
    expect(nowThen(form, true, 3, true)).toBe(tf("auto.trigger.nowFullStart", { below: 3 }));
    hoisted.try.mockResolvedValue({ kind: "program_said_no" });
    await draw(detail(aimed), aimed);
    await press(buttonNamed(t("auto.trigger.try")));
    expect(previewTitle()).toBe(t("auto.trigger.triedSaidNoStart"));
  });

  it("has a word for starting wherever it has one for filing", () => {
    const filing = [
      "lead", "when", "whenPick", "belowUnit", "nowFull", "nowEvery", "nowIdle", "nowTasks", "nowScript", "nowOff",
      "nowNoBelow", "judgeHint", "task", "taskPick", "taskPathHint", "writerHint", "askedHint", "taskDataHint",
      "taskFilesHint", "recorded", "preview", "previewLine", "triedCondition", "triedSaidNo", "triedNothing",
      "removeConfirm", "lastFiled", "neverFiled",
    ];
    const missing = filing.filter((key) => !(`auto.trigger.${key}Start` in en.ui) || !(`auto.trigger.${key}` in en.ui));
    expect(missing).toEqual([]);
  });
});
