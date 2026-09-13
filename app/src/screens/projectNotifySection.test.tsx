// @vitest-environment jsdom
// Project settings > Notifications — what one project does with the device's shelf (`AMB-D-885`).
//
// What these hold the screen to is the shape the decision turned on: a project **selects** rather than
// describing a connection of its own, the switch is apart from that selection, and the mail address — the
// one field that looks like a connection setting — appears only where it means anything.
//
// Only the two seams that talk to core are replaced. The screen's own rendering and branching are real.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { NotifyTarget } from "../core/notifyTargets";
import type { ProjectNotify } from "../core/projectNotify";

/** The thirteen, as core hands them over — the order the checkboxes are drawn in. */
const REPORTABLE = [
  "task.created", "task.status_changed", "task.done", "task.rejected", "task.assigned",
  "task.moved", "task.deleted", "decision.accepted", "decision.rejected", "comment.added",
  "comment.removed", "task.due", "task.due_tomorrow",
];

const hoisted = vi.hoisted(() => ({
  targets: [] as import("../core/notifyTargets").NotifyTarget[],
  notify: null as ProjectNotify | null,
  /** Every switch move, in order. */
  switched: [] as boolean[],
  /** Every target put on or taken off, in order. */
  selected: [] as { targetId: number; selected: boolean }[],
  /** Every tick, in order. */
  ticked: [] as { event: string; on: boolean }[],
  /** Every mail address written, in order. */
  addressed: [] as string[],
}));

vi.mock("../core/projectNotify", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/projectNotify")>();
  return {
    ...orig,
    useProjectNotify: () => ({ notify: hoisted.notify, loading: false }),
    setProjectNotifyEnabled: (_p: number, enabled: boolean) => {
      hoisted.switched.push(enabled);
      return Promise.resolve();
    },
    selectProjectTarget: (_p: number, targetId: number, selected: boolean) => {
      hoisted.selected.push({ targetId, selected });
      return Promise.resolve();
    },
    setProjectNotifyEvent: (_p: number, event: string, on: boolean) => {
      hoisted.ticked.push({ event, on });
      return Promise.resolve();
    },
    setProjectMailTo: (_p: number, mailTo: string) => {
      hoisted.addressed.push(mailTo);
      return Promise.resolve();
    },
  };
});

vi.mock("../core/notifyTargets", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/notifyTargets")>();
  return { ...orig, useNotifyTargets: () => ({ targets: hoisted.targets, loading: false, error: undefined }) };
});

import { ProjectNotifySection } from "./ProjectNotifySection";
import { KIND_GLYPH } from "../components/NotifyKind";
import { t } from "../core/i18n";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

/** One target on the device's shelf. */
const target = (over: Partial<NotifyTarget> & { id: number; name: string }): NotifyTarget => ({
  kind: "slack",
  isDefault: false,
  secretSet: true,
  projectsUsing: 1,
  ...over,
});

/** One project's row: on, carrying nothing, reporting nothing, until a test says otherwise. */
const row = (over: Partial<ProjectNotify> = {}): ProjectNotify => ({
  enabled: true,
  mailTo: "",
  targetIds: [],
  events: [],
  reportable: REPORTABLE,
  ...over,
});

const boxes = () =>
  Array.from(container.querySelectorAll<HTMLInputElement>("input[type=checkbox]"));
const chips = () => Array.from(container.querySelectorAll(".chip--dest"));
const pick = (label: string) =>
  container.querySelector<HTMLSelectElement>(`select[aria-label="${label}"]`)!;
const select = (el: HTMLSelectElement, value: string) => {
  el.value = value;
  el.dispatchEvent(new Event("change", { bubbles: true }));
};
const type = (el: HTMLInputElement, value: string) => {
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
  setter.call(el, value);
  el.dispatchEvent(new Event("input", { bubbles: true }));
};

beforeEach(() => {
  hoisted.targets = [];
  hoisted.notify = row();
  hoisted.switched = [];
  hoisted.selected = [];
  hoisted.ticked = [];
  hoisted.addressed = [];
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

const render = () =>
  act(() => root.render(createElement(ProjectNotifySection, { projectId: 1 })));

describe("choosing from the shelf", () => {
  it("draws the chosen targets as chips and offers the rest", () => {
    hoisted.targets = [
      target({ id: 1, name: "dev team" }),
      target({ id: 2, name: "my mail", kind: "mail" }),
      target({ id: 3, name: "my notes" }),
    ];
    hoisted.notify = row({ targetIds: [1, 2] });
    render();

    // The chip carries the target's own name, and the kind as the dot's glyph beside it.
    expect(chips().map((c) => c.textContent)).toEqual(
      [`${KIND_GLYPH.slack}dev team×`, `${KIND_GLYPH.mail}my mail×`],
    );
    // The kind is readable without the colour: the dot says which it is.
    expect(chips()[0].querySelector(".dot")!.getAttribute("aria-label")).toBe(t("notify.kind.slack"));
    // What is already carrying it is not offered again.
    expect(Array.from(pick(t("notify.addTarget")).options).map((o) => o.textContent))
      .toEqual([t("notify.addTarget"), "my notes"]);
  });

  it("asks no connection of its own — a press names a target and nothing else", () => {
    hoisted.targets = [target({ id: 1, name: "dev team" }), target({ id: 3, name: "my notes" })];
    hoisted.notify = row({ targetIds: [1] });
    render();

    act(() => { select(pick(t("notify.addTarget")), "3"); });
    expect(hoisted.selected).toEqual([{ targetId: 3, selected: true }]);

    act(() => { container.querySelector<HTMLButtonElement>(".chip__x")!.click(); });
    expect(hoisted.selected[1]).toEqual({ targetId: 1, selected: false });
  });

  it("offers the shelf's own control where the device holds nothing yet", () => {
    render();
    expect(container.textContent).toContain(t("notify.noneOnDevice"));
    // The shelf section itself, drawn here rather than sending the reader away to find it.
    expect(container.textContent).toContain(t("notify.empty"));
  });
});

describe("the switch", () => {
  it("is apart from the selection — turning it off names no target", () => {
    hoisted.targets = [target({ id: 1, name: "dev team" })];
    hoisted.notify = row({ targetIds: [1] });
    render();

    act(() => { select(pick(t("notify.projectSwitch")), "off"); });

    expect(hoisted.switched).toEqual([false]);
    expect(hoisted.selected).toEqual([]);
    // And the chips are still standing, which is the whole point of the two being apart.
    expect(chips()).toHaveLength(1);
  });
});

describe("what it reports", () => {
  it("draws core's catalog, ticked where this project reports it", () => {
    hoisted.notify = row({ events: ["task.created", "task.done"] });
    render();

    expect(boxes()).toHaveLength(REPORTABLE.length);
    expect(boxes().filter((b) => b.checked)).toHaveLength(2);
    expect(container.textContent).toContain(t("notify.event.task.created"));
    expect(container.textContent).toContain(t("notify.event.task.due_tomorrow"));
  });

  it("writes one tick as it is pressed", () => {
    hoisted.notify = row({ events: ["task.created"] });
    render();

    act(() => { boxes()[0].click(); });
    expect(hoisted.ticked).toEqual([{ event: "task.created", on: false }]);
  });

  it("has a word for every event core can hand over", () => {
    // The screen names these at runtime (`notify.event.${event}`), which the source sweep in
    // `core/i18n/sourceKeys.test.ts` cannot read — so the family is held to English here.
    for (const event of REPORTABLE) {
      expect(t(`notify.event.${event}`)).not.toBe(`notify.event.${event}`);
    }
  });
});

describe("where the mail goes", () => {
  it("is asked only while a mail target is carrying this project", () => {
    hoisted.targets = [target({ id: 1, name: "dev team" }), target({ id: 2, name: "my mail", kind: "mail" })];
    hoisted.notify = row({ targetIds: [1] });
    render();
    expect(container.textContent).not.toContain(t("notify.mailTo"));

    act(() => root.unmount());
    root = createRoot(container);
    hoisted.notify = row({ targetIds: [1, 2] });
    render();
    expect(container.textContent).toContain(t("notify.mailTo"));
  });

  it("writes the address when the box is left", () => {
    hoisted.targets = [target({ id: 2, name: "my mail", kind: "mail" })];
    hoisted.notify = row({ targetIds: [2] });
    render();

    const box = container.querySelector<HTMLInputElement>(`input[aria-label="${t("notify.mailTo")}"]`)!;
    act(() => { type(box, "team@example.com, me@example.com"); });
    // Still unwritten while it is being typed into.
    expect(hoisted.addressed).toEqual([]);

    // React hears a blur as `focusout`, which is the event a real pointer leaving the box raises.
    act(() => { box.dispatchEvent(new FocusEvent("focusout", { bubbles: true })); });
    expect(hoisted.addressed).toEqual(["team@example.com, me@example.com"]);
  });
});
