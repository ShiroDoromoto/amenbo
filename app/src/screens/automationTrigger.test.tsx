// @vitest-environment jsdom
// An automation's first trigger, written on its dialog. The trigger writes and the confirm are stubbed;
// every field, chip and press runs for real.
//
// What these guard: **a new trigger starts as one that files every hour, and the save sends what was
// typed** as a trigger added; **a trigger already written opens as it stands and goes back unchanged**,
// rewritten under its id, the others left alone; **"while fewer
// than N tasks match" always carries its condition, and "when a script says so" its script**, while
// with the other kinds each is sent only when asked for; **the note and the preview say what will be
// filed** — its title, role, who works it and the trigger's name; **a refusal is drawn and the dialog
// stays open**; and **"Remove trigger" takes that one away by its id**.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AutomationDetailDto, AutomationTriggerDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({ add: vi.fn(), update: vi.fn(), remove: vi.fn(), confirm: vi.fn() }));

vi.mock("../core/automations", () => ({
  addAutomationTrigger: hoisted.add,
  updateAutomationTrigger: hoisted.update,
  removeAutomationTrigger: hoisted.remove,
}));
vi.mock("../core/dialog", () => ({ confirmDialog: hoisted.confirm }));

import { t, tf } from "../core/i18n";
import { AutomationTrigger, formOf, inputOf } from "./AutomationTrigger";

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

async function draw(automation: AutomationDetailDto, onSaved = vi.fn(), onClose = vi.fn()) {
  await act(async () => root.render(createElement(AutomationTrigger, { automation, onSaved, onClose })));
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
  const label = [...page().querySelectorAll("label")].find(
    (one) => one.querySelector(".autostep__label")?.textContent === words,
  );
  const inside = label?.querySelector<HTMLInputElement | HTMLTextAreaElement>("input, textarea");
  expect(inside).toBeTruthy();
  return inside!;
}

async function type(control: HTMLInputElement | HTMLTextAreaElement, value: string) {
  await act(async () => {
    const proto = control instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
    Object.getOwnPropertyDescriptor(proto, "value")!.set!.call(control, value);
    control.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

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
  for (const write of [hoisted.add, hoisted.update, hoisted.remove]) {
    write.mockReset();
    write.mockResolvedValue(undefined);
  }
  hoisted.confirm.mockReset();
  hoisted.confirm.mockResolvedValue(true);
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
    expect(inputOf({ ...form, when: "tasks", filter: " status:todo ", below: "2" }).condition).toEqual({
      filter: "status:todo",
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

describe("the dialog", () => {
  it("saves what was typed through the trigger's write, then closes", async () => {
    const { onSaved, onClose } = await draw(detail());
    await type(box(t("auto.trigger.name")), "Morning check");
    await type(box(t("auto.trigger.minutes")), "1440");
    await type(box(t("auto.trigger.taskTitle")), "Check the reports");
    await type(box(t("auto.trigger.taskNotes")), "Read the new ones.");
    await press(buttonNamed("Alice"));
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
    await draw(detail(written));
    expect((box(t("auto.trigger.name")) as HTMLInputElement).value).toBe("Top up the reviews");
    expect(buttonNamed(t("auto.trigger.whenIdle"))?.getAttribute("aria-pressed")).toBe("true");
    expect(tick(t("auto.trigger.counted"))?.checked).toBe(true);
    expect((box(t("auto.trigger.filter")) as HTMLInputElement).value).toBe("status:todo");
    expect((box(t("auto.trigger.below")) as HTMLInputElement).value).toBe("3");
    expect((box(t("auto.trigger.taskPath")) as HTMLInputElement).value).toBe("/Users/alice/next.md");
    expect(buttonNamed("Bob")?.getAttribute("aria-pressed")).toBe("true");
  });

  it("rewrites the first trigger under its id, leaving the others", async () => {
    const other: AutomationTriggerDto = { ...written, id: 32, name: "Daily check" };
    await draw(detail(written, other));
    await type(box(t("auto.trigger.name")), "Top up the reviews again");
    await press(buttonNamed(t("auto.trigger.save")));
    expect(hoisted.update).toHaveBeenCalledWith(31, expect.objectContaining({ name: "Top up the reviews again" }));
    expect(hoisted.add).not.toHaveBeenCalled();
  });

  it("keeps the condition on for a tasks trigger, with no box to turn it off", async () => {
    await draw(detail());
    expect(page().querySelector(`[aria-label="${t("auto.trigger.filter")}"]`)).toBeNull();
    await press(buttonNamed(t("auto.trigger.whenTasks")));
    expect(tick(t("auto.trigger.counted"))).toBeUndefined();
    await type(box(t("auto.trigger.filter")), "status:todo");
    await type(box(t("auto.trigger.below")), "5");
    await press(buttonNamed(t("auto.trigger.save")));
    expect(hoisted.add.mock.calls[0]?.[1]).toMatchObject({
      when: { kind: "tasks" },
      condition: { filter: "status:todo", below: 5 },
    });
  });

  it("asks for a script's boxes when the script decides", async () => {
    await draw(detail());
    expect(page().querySelector(`[aria-label="${t("auto.step.program")}"]`)).toBeNull();
    await press(buttonNamed(t("auto.trigger.whenScript")));
    expect(tick(t("auto.trigger.asked"))).toBeUndefined();
    await type(box(t("auto.step.program")), "/usr/bin/python3");
    await press(buttonNamed(t("auto.trigger.save")));
    expect(hoisted.add.mock.calls[0]?.[1]).toMatchObject({
      when: { kind: "script" },
      script: { program: "/usr/bin/python3", args: [] },
    });
  });

  it("says what is recorded, under the trigger's name", async () => {
    await draw(detail(written));
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
    await press(buttonNamed("Bob"));
    expect(previewTitle()).toBe("Check the reports");
    expect(previewLine()).toBe(tf("auto.trigger.previewLine", { role: "Bob", trigger: "Morning check" }));

    await press(buttonNamed(t("auto.trigger.taskFile")));
    expect(previewTitle()).toBe(t("auto.trigger.previewFromFile"));
    await press(buttonNamed(t("auto.trigger.taskScript")));
    expect(previewTitle()).toBe(t("auto.trigger.previewFromScript"));
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
    const { onClose } = await draw(detail(written));
    await press(buttonNamed(t("auto.trigger.remove")));
    expect(hoisted.confirm).toHaveBeenCalledWith(tf("auto.trigger.removeConfirm", { name: "Top up the reviews" }));
    expect(hoisted.remove).toHaveBeenCalledWith(31);
    expect(onClose).toHaveBeenCalled();
  });

  it("offers no removal while there is no trigger", async () => {
    await draw(detail());
    expect(buttonNamed(t("auto.trigger.remove"))).toBeUndefined();
  });
});
