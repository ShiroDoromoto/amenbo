// @vitest-environment jsdom
// What the automation holds as a whole, down the build screen's right. The writes are stubbed; every
// press and pulldown runs for real.
//
// What these guard: **the column holds two boxes — who failures are filed to, and the trigger — and
// no count of runs at once**; **who failures are filed to is required, offers the automation's roles
// and the person, and writes the one picked**; **the trigger is offered to add while there is none,
// and drawn — what it files, when and under which role — once there is one, each asking the build
// screen to open the trigger's card**; and **a refusal is drawn in core's words**.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AutomationDetailDto, AutomationTriggerDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  /** Each patch the column wrote, in order. */
  edits: [] as unknown[],
  /** What the next write is refused with, if anything. */
  refuse: null as string | null,
  /** Each trigger the column asked to open, `undefined` for a new one. */
  opened: [] as unknown[],
}));

vi.mock("../core/automations", () => ({
  editAutomation: (_id: number, patch: unknown) => {
    hoisted.edits.push(patch);
    return hoisted.refuse === null ? Promise.resolve() : Promise.reject(new Error(hoisted.refuse));
  },
  addAutomationTrigger: () => Promise.resolve(),
  updateAutomationTrigger: () => Promise.resolve(),
  removeAutomationTrigger: () => Promise.resolve(),
}));
vi.mock("../core/dialog", () => ({ confirmDialog: () => Promise.resolve(false) }));

import { t, tf } from "../core/i18n";
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
  hoisted.opened = [];
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

const render = (shown: AutomationDetailDto = automation) =>
  act(async () => {
    root.render(
      createElement(AutomationSettings, {
        automation: shown,
        onOpenTrigger: (trigger: AutomationTriggerDto | undefined) => hoisted.opened.push(trigger),
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

describe("the column", () => {
  it("holds who failures are filed to and the trigger, each in a box of its own, and no count of runs at once", async () => {
    await render();
    const boxes = Array.from(container.querySelectorAll(".autoside > .autoside__group"));
    expect(boxes).toHaveLength(2);
    expect(boxes[0]!.textContent).toContain(t("auto.side.failuresTo"));
    expect(boxes[0]!.querySelector("select")).not.toBeNull();
    expect(boxes[1]!.textContent).toContain(t("auto.side.trigger"));
    expect(container.querySelector("output")).toBeNull();
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

describe("the trigger", () => {
  it("is offered to add while there is none, and the press asks for a new one's card", async () => {
    await render();
    expect(container.textContent).toContain(t("auto.side.triggerNone"));
    await press(button(t("auto.side.triggerAdd")));
    expect(hoisted.opened).toEqual([undefined]);
  });

  it("draws the one written — its name, when, what it files and its role — and opens it on a press", async () => {
    await render({ ...automation, triggers: [written] });
    const drawn = container.querySelector<HTMLButtonElement>(".autoside__trigger")!;
    expect(drawn.textContent).toContain("Top up the reviews");
    expect(drawn.textContent).toContain(tf("auto.side.every", { minutes: 30 }));
    expect(drawn.textContent).toContain("Look at the reviews");
    expect(drawn.textContent).toContain(tf("auto.side.triggerRole", { role: "Bob" }));
    expect(button(t("auto.side.triggerAdd"))).toBeUndefined();
    await press(drawn);
    expect(hoisted.opened).toEqual([written]);
  });
});

it("draws a refusal in core's words", async () => {
  hoisted.refuse = "role '5' belongs to another automation";
  await render();
  await pick("role:1");
  expect(container.textContent).toContain("role '5' belongs to another automation");
});
