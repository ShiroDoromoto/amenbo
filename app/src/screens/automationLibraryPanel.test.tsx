// @vitest-environment jsdom
// The library in the build screen's panel (`AMB-T-5360`).
//
// What these guard: **both reaches are offered, each under its own head**; **the box narrows by what
// the name and the note say**; **a row opens in place with the press that places it**, and that
// press **goes through the door the target names** — a line, or a picture with none yet; **a refusal
// is drawn and leaves the panel standing**, rather than closing on a placement that did not happen;
// and **making one here is handed back to the screen**, which opens the dialog.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AutomationActionCardDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  actions: [] as AutomationActionCardDto[],
  insert: vi.fn((..._args: unknown[]) => Promise.resolve()),
  place: vi.fn((..._args: unknown[]) => Promise.resolve()),
}));

vi.mock("../core/automations", () => ({
  useAutomationActions: () => hoisted.actions,
  // What an opened row reads. Its chips are the declaration band's (`./AutomationActionBuildScreen`);
  // here it is the press under them that matters.
  useAutomationAction: () => null,
  insertAutomationAction: hoisted.insert,
  placeAutomationAction: hoisted.place,
}));

import { t } from "../core/i18n";
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
        where: "after",
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

describe("the library in the panel", () => {
  it("lists this project's and the device's under their own heads", async () => {
    await render();
    const heads = [...container.querySelectorAll(".autolib__head")].map((one) => one.textContent);
    expect(heads).toEqual([t("auto.actions.reachProject"), t("auto.lib.global")]);
    const groups = [...container.querySelectorAll(".autolib__group")];
    expect(groups[0]!.textContent).toContain("Review");
    expect(groups[1]!.textContent).toContain("Publish");
  });

  it("narrows by what the name and the note say", async () => {
    await render();
    await typeInto(container.querySelector<HTMLInputElement>("input")!, "draft");
    expect(rows().some((one) => one.includes("Review"))).toBe(true);
    expect(rows().some((one) => one.includes("Publish"))).toBe(false);
    expect(container.textContent).toContain(t("auto.actions.noMatch"));
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
    await act(async () => { button(t("auto.lib.make")).click(); });
    expect(make).toHaveBeenCalledTimes(1);
  });
});
