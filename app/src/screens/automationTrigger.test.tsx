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
// its title, role, who works it and the trigger's name; **a refusal is drawn and the card stays
// open**; **"Remove trigger" takes that one away by its id**; and **the condition is picked as a
// status, a role and a count**, a filter the boxes cannot show kept as it was written, with how many
// tasks match now and what the next look does said under it.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AutomationDetailDto, AutomationTriggerDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  add: vi.fn(),
  update: vi.fn(),
  remove: vi.fn(),
  enable: vi.fn(),
  confirm: vi.fn(),
  count: vi.fn(),
}));

vi.mock("../core/automations", () => ({
  addAutomationTrigger: hoisted.add,
  updateAutomationTrigger: hoisted.update,
  removeAutomationTrigger: hoisted.remove,
  setAutomationTriggerEnabled: hoisted.enable,
  useTriggerCount: hoisted.count,
}));
vi.mock("../core/dialog", () => ({ confirmDialog: hoisted.confirm }));

import { dueLabel, errText, t, tf } from "../core/i18n";
import { filterValueLabel } from "./automationParts";
import { AutomationTrigger, formOf, inputOf, joinFilter, nowThen, splitFilter } from "./AutomationTrigger";
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
      { id: 1, name: "Alice", headcount: 1, passes: [] },
      { id: 2, name: "Bob", headcount: 1, passes: [] },
    ],
    placements: [],
    edges: [],
    wires: [],
    maxHeadcount: 10,
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

/** The box a label names, by the words on its label or its `aria-label`. */
function box(words: string): HTMLInputElement | HTMLTextAreaElement {
  const named = page().querySelector<HTMLInputElement>(`[aria-label="${words}"]`);
  if (named !== null) return named;
  const label = [...page().querySelectorAll("label")].find((one) => one.textContent === words);
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
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the form and what it sends", () => {
  it("starts a new trigger as one that files fixed words every hour, with no role", () => {
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
    expect(roles).toEqual(["Alice", "Bob", t("auto.trigger.roleNone")]);
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
    expect(roles).toEqual(["Alice", "Bob", "9", t("auto.trigger.roleNone")]);
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
    expect(roles).toEqual(["Alice", "Bob", t("auto.trigger.anyRole")]);
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
