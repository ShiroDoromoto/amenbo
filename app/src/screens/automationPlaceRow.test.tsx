// @vitest-environment jsdom
// The row that puts an action on a picture (`AMB-T-5316`), and the press beside it that writes one
// first (`AMB-T-5317`).
//
// What these guard: **the library is what is offered**, both reaches in one list; **the press sends
// the action that was picked**, and is held shut until one is; **an empty library says where actions
// are made** rather than offering a pulldown with nothing in it, **and still offers the press that
// writes one** — a project with nothing on its shelf is exactly where that road is needed; **what
// the dialog writes is placed on this picture, with the library it was told to land in**; and **a
// refusal lands on the row** rather than in the console.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AutomationActionCardDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  actions: [] as AutomationActionCardDto[],
  place: vi.fn(() => Promise.resolve()),
  write: vi.fn(() => Promise.resolve()),
}));

vi.mock("../core/automations", () => ({
  useAutomationActions: () => hoisted.actions,
  placeAutomationAction: hoisted.place,
  placeAutomationActionFromPrompt: hoisted.write,
  // What the dialog this row opens reaches for on its other roads, none of them taken here.
  insertAutomationStep: vi.fn(),
  insertAutomationActionStep: vi.fn(),
  addAutomationStep: vi.fn(),
}));

import { t } from "../core/i18n";
import { AutomationPlaceRow } from "./AutomationPlaceRow";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

function card(over: Partial<AutomationActionCardDto> & { id: number; name: string }) {
  return { note: "", steps: 1, global: false, usedBy: 0, ...over };
}

async function render() {
  await act(async () => {
    root.render(createElement(AutomationPlaceRow, { automationId: 7, projectId: 1 }));
  });
}

const select = () => container.querySelector<HTMLSelectElement>("select")!;
const press = () => container.querySelector<HTMLButtonElement>("button")!;
/** The dialog draws into `document.body`, so what it holds is looked for on the page. */
const button = (label: string) =>
  [...document.body.querySelectorAll<HTMLButtonElement>("button")].find(
    (one) => one.textContent === label,
  )!;

/** Type into a controlled box the way React hears it — its own value tracker has to be moved. */
async function typeInto(box: HTMLInputElement | HTMLTextAreaElement, value: string) {
  const proto = box instanceof HTMLTextAreaElement ? HTMLTextAreaElement : HTMLInputElement;
  await act(async () => {
    Object.getOwnPropertyDescriptor(proto.prototype, "value")!.set!.call(box, value);
    box.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  hoisted.place.mockReset();
  hoisted.place.mockResolvedValue(undefined);
  hoisted.write.mockReset();
  hoisted.write.mockResolvedValue(undefined);
  hoisted.actions = [card({ id: 4, name: "Take one" }), card({ id: 5, name: "Review", global: true })];
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the row that places an action", () => {
  it("offers the library, and places the one that was picked", async () => {
    await render();
    expect([...select().options].map((one) => one.textContent)).toEqual(["—", "Take one", "Review"]);
    expect(press().disabled).toBe(true);

    await act(async () => {
      select().value = "5";
      select().dispatchEvent(new Event("change", { bubbles: true }));
    });
    await act(async () => press().click());
    expect(hoisted.place).toHaveBeenCalledWith(7, 5);
  });

  /// A pulldown with nothing in it says the press is there and does not work. The row says where an
  /// action is made instead.
  it("says where actions are made while the library holds none", async () => {
    hoisted.actions = [];
    await render();
    expect(container.textContent).toContain(t("auto.pic.libraryEmpty"));
    expect(container.querySelector("select")).toBeNull();
    expect(press().textContent).toBe(t("auto.pic.writeDo"));
  });

  it("places what the dialog wrote, on the library it was told to keep it in", async () => {
    await render();
    await act(async () => button(t("auto.pic.writeDo")).click());

    const name = document.body.querySelector<HTMLInputElement>("input")!;
    await typeInto(name, "下ごしらえ");
    await typeInto(document.body.querySelector<HTMLTextAreaElement>("textarea")!, "やる");
    const shelf = [...document.body.querySelectorAll<HTMLSelectElement>("select")].find(
      (one) => one.value === "project",
    )!;
    await act(async () => {
      shelf.value = "device";
      shelf.dispatchEvent(new Event("change", { bubbles: true }));
    });
    await act(async () => button(t("auto.add.put")).click());

    expect(hoisted.write).toHaveBeenCalledWith(7, {
      name: "下ごしらえ",
      prompt: "やる",
      shelf: "device",
      agent: "claude-code",
      interactive: false,
      exits: [],
      inputs: [],
    });
    // The picture it was opened over is not offered a library to pick from: the pulldown beside the
    // press is that road, and two of them would be one question asked twice.
    expect(hoisted.place).not.toHaveBeenCalled();
  });

  it("draws what core refused", async () => {
    hoisted.place.mockRejectedValue("action '5' is in another project's library");
    await render();
    await act(async () => {
      select().value = "5";
      select().dispatchEvent(new Event("change", { bubbles: true }));
    });
    await act(async () => press().click());
    expect(container.querySelector('[role="alert"]')!.textContent).toContain("another project");
  });
});
