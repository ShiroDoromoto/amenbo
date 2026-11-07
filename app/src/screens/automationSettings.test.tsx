// @vitest-environment jsdom
// What the automation holds as a whole, down the build screen's right. The writes are stubbed; every
// press and pulldown runs for real.
//
// What these guard: **the column holds three boxes — who failures are filed to, the triggers, and how
// many each role has — and no count of runs at once for the automation as a whole**; **every role
// has a row, in order, with the actions placed for it and a stepper held between 1 and the cap, which
// writes the role's headcount; the sum is the most tasks at once; a row whose draft differs from the
// saved version says what the version holds; and the foot saves or discards the draft with the head's
// presses, or says it is the saved version**; **who failures are filed to is required, offers the automation's roles
// and the person, and writes the one picked**; **every trigger is listed in order — when, what it
// files and under which role, the wait in the largest unit it divides into and the condition by what
// it counts — and a press opens the dialog on that one**; **"+ Add a trigger" is
// always there, and opens the dialog on a new one**; and **a refusal is drawn in core's words**.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AutomationDetailDto, AutomationTriggerDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  /** Each patch the column wrote, in order. */
  edits: [] as unknown[],
  /** What the next write is refused with, if anything. */
  refuse: null as string | null,
}));

vi.mock("../core/automations", () => ({
  editAutomation: (_id: number, patch: unknown) => {
    hoisted.edits.push(patch);
    return hoisted.refuse === null ? Promise.resolve() : Promise.reject(new Error(hoisted.refuse));
  },
  editAutomationRole: (id: number, patch: unknown) => {
    hoisted.edits.push({ role: id, ...(patch as object) });
    return hoisted.refuse === null ? Promise.resolve() : Promise.reject(new Error(hoisted.refuse));
  },
  addAutomationTrigger: () => Promise.resolve(),
  updateAutomationTrigger: () => Promise.resolve(),
  removeAutomationTrigger: () => Promise.resolve(),
}));
vi.mock("../core/dialog", () => ({ confirmDialog: () => Promise.resolve(false) }));

import { statusLabel, t, tf } from "../core/i18n";
import { AutomationSettings } from "./AutomationSettings";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

const automation: AutomationDetailDto = {
  id: 9,
  projectId: 1,
  name: "Morning round",
  notes: "",
  archived: false,
  triggers: [],
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

const written: AutomationTriggerDto = {
  id: 31,
  enabled: true,
  name: "Top up the reviews",
  roleId: 2,
  when: { kind: "every", minutes: 30 },
  task: { kind: "fixed", title: "Look at the reviews", notes: "" },
  nextAt: "2026-11-07T00:00:00Z",
};

beforeEach(() => {
  hoisted.edits = [];
  hoisted.refuse = null;
  Object.assign(draft, { saved: 0, discarded: 0, canSave: true, canDiscard: true });
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

/** The head's two presses, as the build screen hands them down: each press counted. */
const draft = { saved: 0, discarded: 0, canSave: true, canDiscard: true };

const render = (shown: AutomationDetailDto = automation) =>
  act(async () => {
    root.render(
      createElement(AutomationSettings, {
        automation: shown,
        draft: {
          save: () => draft.saved++,
          discard: () => draft.discarded++,
          canSave: draft.canSave,
          canDiscard: draft.canDiscard,
        },
      }),
    );
  });

const button = (label: string) =>
  Array.from(document.body.querySelectorAll("button")).find(
    (b) => b.getAttribute("aria-label") === label || (b.textContent ?? "") === label,
  );
const press = (b: HTMLButtonElement | undefined) => act(async () => { b!.click(); });
const select = () => container.querySelector("select")!;
const pick = (value: string) =>
  act(async () => {
    select().value = value;
    select().dispatchEvent(new Event("change", { bubbles: true }));
  });
const dialog = () => document.body.querySelector('[role="dialog"]');

describe("the column", () => {
  it("holds who failures are filed to, the triggers and the headcounts, each in a box of its own", async () => {
    await render();
    const boxes = Array.from(container.querySelectorAll(".autoside > .autoside__group"));
    expect(boxes).toHaveLength(3);
    expect(boxes[0]!.textContent).toContain(t("auto.side.failuresTo"));
    expect(boxes[0]!.querySelector("select")).not.toBeNull();
    expect(boxes[1]!.textContent).toContain(t("auto.side.trigger"));
    expect(boxes[2]!.textContent).toContain(t("auto.side.headcount"));
    // A number stepped is a role's alone: there is no count for the automation as a whole.
    expect(container.querySelectorAll("output")).toHaveLength(automation.roles.length);
  });
});

/** One placement of an action, put down for the role `roleId`. */
const placed = (id: number, name: string, roleId: number) =>
  ({ id, name, roleId }) as unknown as AutomationDetailDto["placements"][number];

const rows = () => Array.from(container.querySelectorAll(".autoside__role"));
const stepperOf = (role: string) =>
  container.querySelector<HTMLElement>(`[role="group"][aria-label="${tf("auto.side.headcountOf", { role })}"]`)!;
const stepButton = (role: string, label: string) =>
  Array.from(stepperOf(role).querySelectorAll("button")).find((b) => b.getAttribute("aria-label") === label)!;

describe("the headcounts", () => {
  it("give every role a row, in order, with the actions placed for it and its headcount", async () => {
    await render({
      ...automation,
      roles: [
        { id: 1, name: "Alice", headcount: 3, passes: [] },
        { id: 2, name: "Bob", headcount: 1, passes: [] },
      ],
      placements: [placed(11, "Read the context", 1), placed(12, "Implement", 1), placed(13, "Review", 2)],
    });
    expect(rows().map((row) => row.querySelector(".autoside__rolename")!.textContent)).toEqual(["Alice", "Bob"]);
    expect(Array.from(rows()[0]!.querySelectorAll(".autoside__act")).map((one) => one.textContent)).toEqual([
      "Read the context",
      "Implement",
    ]);
    expect(stepperOf("Alice").querySelector("output")!.textContent).toBe(tf("auto.side.headcountValue", { n: 3 }));
    expect(container.querySelector(".autoside__total")!.textContent).toContain("4");
    expect(container.textContent).toContain(tf("auto.side.headcountRange", { min: 1, max: 10 }));
  });

  it("step one at a time, held between 1 and the cap, and write the role's headcount", async () => {
    await render({
      ...automation,
      roles: [
        { id: 1, name: "Alice", headcount: 1, passes: [] },
        { id: 2, name: "Bob", headcount: 10, passes: [] },
      ],
    });
    const less = t("auto.side.headcountLess");
    const more = t("auto.side.headcountMore");
    expect(stepButton("Alice", less).disabled).toBe(true);
    expect(stepButton("Bob", more).disabled).toBe(true);
    await press(stepButton("Alice", more));
    await press(stepButton("Bob", less));
    expect(hoisted.edits).toEqual([
      { role: 1, headcount: 2 },
      { role: 2, headcount: 9 },
    ]);
    expect(document.activeElement).toBe(stepButton("Bob", less));
  });

  it("say what the saved version holds where the draft differs from it, and nothing where it does not", async () => {
    await render({
      ...automation,
      unsaved: true,
      roles: [
        { id: 1, name: "Alice", headcount: 3, savedHeadcount: 2, passes: [] },
        { id: 2, name: "Bob", headcount: 1, savedHeadcount: 1, passes: [] },
        { id: 3, name: "Carol", headcount: 2, passes: [] },
      ],
    });
    expect(rows()[0]!.textContent).toContain(tf("auto.side.headcountSaved", { n: 2 }));
    expect(rows()[1]!.textContent).not.toContain(tf("auto.side.headcountSaved", { n: 1 }));
    expect(rows()[2]!.querySelector(".autostep__hint")).toBeNull();
  });

  it("end on the head's save and discard while the draft holds changes", async () => {
    await render({ ...automation, unsaved: true });
    expect(container.textContent).toContain(t("auto.side.draftUnsaved"));
    await press(button(t("auto.side.draftSave")));
    await press(button(t("auto.side.draftDiscard")));
    expect([draft.saved, draft.discarded]).toEqual([1, 1]);
  });

  it("hold the two presses back on the head's terms", async () => {
    Object.assign(draft, { canSave: false, canDiscard: false });
    await render({ ...automation, unsaved: true });
    expect(button(t("auto.side.draftSave"))!.disabled).toBe(true);
    expect(button(t("auto.side.draftDiscard"))!.disabled).toBe(true);
  });

  it("say the draft is the saved version where nothing is unsaved", async () => {
    await render();
    expect(container.textContent).toContain(t("auto.side.draftSame"));
    expect(button(t("auto.side.draftSave"))).toBeUndefined();
  });

  it("draw a refused step in core's words", async () => {
    hoisted.refuse = "how many are in a role is from 1 to 10, not 11";
    await render();
    await press(stepButton("Alice", t("auto.side.headcountMore")));
    expect(container.textContent).toContain("how many are in a role is from 1 to 10, not 11");
  });
});

describe("who failures are filed to", () => {
  it("is marked required and opens on choosing one while nobody is chosen", async () => {
    await render();
    expect(container.textContent).toContain(t("auto.side.required"));
    expect(select().value).toBe("");
    expect(Array.from(select().options).map((o) => o.textContent)).toEqual([
      t("auto.side.failuresNone"),
      tf("auto.side.failuresRole", { role: "Alice" }),
      tf("auto.side.failuresRole", { role: "Bob" }),
      t("auto.side.failuresHuman"),
    ]);
  });

  it("writes the role or the person picked", async () => {
    await render();
    await pick("role:2");
    await pick("human");
    expect(hoisted.edits).toEqual([
      { failuresTo: { kind: "role", roleId: 2 } },
      { failuresTo: { kind: "human" } },
    ]);
  });

  it("shows the one chosen, with nobody no longer offered", async () => {
    await render({ ...automation, failuresTo: { kind: "role", roleId: 1 } });
    expect(select().value).toBe("role:1");
    expect(Array.from(select().options).some((o) => o.value === "")).toBe(false);
  });
});

const other: AutomationTriggerDto = {
  id: 32,
  enabled: true,
  name: "Daily check",
  when: { kind: "idle" },
  condition: { filter: "status:todo", below: 3 },
  task: { kind: "file", path: "/Users/alice/next.md" },
  nextAt: "2026-11-07T00:00:00Z",
};

/** The dialog's first box, which holds the trigger's name. */
const nameBox = () => dialog()!.querySelector("input")!;

describe("the triggers", () => {
  it("say there are none, and offer to add one, whose press opens a new trigger's dialog", async () => {
    await render();
    expect(container.textContent).toContain(t("auto.side.triggerNone"));
    expect(container.querySelector(".autoside__trigger")).toBeNull();
    expect(dialog()).toBeNull();
    await press(button(t("auto.side.triggerAdd")));
    expect(dialog()).not.toBeNull();
    expect(nameBox().value).toBe("");
    expect(button(t("auto.trigger.remove"))).toBeUndefined();
  });

  it("are listed in order, each by when, what it files and its role", async () => {
    await render({ ...automation, triggers: [written, other] });
    const drawn = Array.from(container.querySelectorAll<HTMLButtonElement>(".autoside__trigger"));
    expect(drawn).toHaveLength(2);
    expect(drawn[0]!.querySelector(".autoside__when")!.textContent).toBe(tf("auto.side.every", { minutes: 30 }));
    expect(drawn[0]!.querySelector(".autoside__what")!.textContent).toBe("Look at the reviews");
    expect(drawn[0]!.textContent).toContain(tf("auto.side.triggerRole", { role: "Bob" }));
    expect(drawn[1]!.querySelector(".autoside__when")!.textContent).toBe(
      `${t("auto.trigger.whenIdle")} ・ ${tf("auto.side.belowOf", { what: statusLabel("todo"), below: 3 })}`,
    );
    expect(drawn[1]!.querySelector(".autoside__what")!.textContent).toBe("/Users/alice/next.md");
    expect(drawn[1]!.textContent).toContain(t("auto.side.triggerRoleNone"));
    expect(container.textContent).not.toContain(t("auto.side.triggerNone"));
  });

  it("give the wait in days or hours where the minutes divide into them", async () => {
    const every = (id: number, minutes: number): AutomationTriggerDto => ({
      ...written,
      id,
      when: { kind: "every", minutes },
    });
    await render({ ...automation, triggers: [every(41, 1440), every(42, 120), every(43, 90)] });
    const whens = Array.from(container.querySelectorAll(".autoside__when")).map((one) => one.textContent);
    expect(whens).toEqual([
      tf("auto.side.everyDays", { days: 1 }),
      tf("auto.side.everyHours", { hours: 2 }),
      tf("auto.side.every", { minutes: 90 }),
    ]);
  });

  it("name what the condition counts — the filter's values and the role — or count every task", async () => {
    const counting = (id: number, condition: AutomationTriggerDto["condition"]): AutomationTriggerDto => ({
      ...other,
      id,
      when: { kind: "tasks" },
      condition,
    });
    await render({
      ...automation,
      triggers: [
        counting(51, { filter: "status:todo,in_progress", roleId: 1, below: 3 }),
        counting(52, { filter: "", below: 2 }),
      ],
    });
    const whens = Array.from(container.querySelectorAll(".autoside__when")).map((one) => one.textContent);
    expect(whens).toEqual([
      tf("auto.side.belowOf", { what: `${statusLabel("todo")}/${statusLabel("in_progress")} Alice`, below: 3 }),
      tf("auto.side.below", { below: 2 }),
    ]);
  });

  it("keep offering to add one however many there are", async () => {
    await render({ ...automation, triggers: [written, other] });
    await press(button(t("auto.side.triggerAdd")));
    expect(nameBox().value).toBe("");
  });

  it("open the dialog on the one pressed", async () => {
    await render({ ...automation, triggers: [written, other] });
    const drawn = Array.from(container.querySelectorAll<HTMLButtonElement>(".autoside__trigger"));
    await press(drawn[1]);
    expect(nameBox().value).toBe("Daily check");
    expect(button(t("auto.trigger.remove"))).toBeDefined();
  });
});

it("draws a refusal in core's words", async () => {
  hoisted.refuse = "role '5' belongs to another automation";
  await render();
  await pick("role:1");
  expect(container.textContent).toContain("role '5' belongs to another automation");
});
