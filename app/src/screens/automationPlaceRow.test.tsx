// @vitest-environment jsdom
// The row that puts an action on a picture (`AMB-T-5316`).
//
// What these guard: **the library is what is offered**, both reaches in one list; **the press sends
// the action that was picked**, and is held shut until one is; **an empty library says where actions
// are made** rather than offering a pulldown with nothing in it; and **a refusal lands on the row**
// rather than in the console.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AutomationActionCardDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  actions: [] as AutomationActionCardDto[],
  place: vi.fn(() => Promise.resolve()),
}));

vi.mock("../core/automations", () => ({
  useAutomationActions: () => hoisted.actions,
  placeAutomationAction: hoisted.place,
}));

import { t } from "../core/i18n";
import { AutomationPlaceRow } from "./AutomationPlaceRow";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

function card(over: Partial<AutomationActionCardDto> & { id: number; name: string }) {
  return { prompt: "", steps: 1, global: false, usedBy: 0, ...over };
}

async function render() {
  await act(async () => {
    root.render(createElement(AutomationPlaceRow, { automationId: 7, projectId: 1 }));
  });
}

const select = () => container.querySelector<HTMLSelectElement>("select")!;
const press = () => container.querySelector<HTMLButtonElement>("button")!;

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  hoisted.place.mockReset();
  hoisted.place.mockResolvedValue(undefined);
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
    expect(container.textContent).toBe(t("auto.pic.libraryEmpty"));
    expect(container.querySelector("select")).toBeNull();
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
