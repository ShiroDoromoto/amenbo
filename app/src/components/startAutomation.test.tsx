// @vitest-environment jsdom
// The way into a launch that is not the build screen: the empty frame (`AMB-T-5260`).
//
// What these guard: **the press carries nothing but the automation** — no task, no folder choice, no
// narrowing — so it is the same run the automations tab would start;
// **a project with no automations draws no entrance**, since a heading over an empty row would put
// the subject in front of a reader who has never met it; **an archived one is not offered**, which
// is what archiving is for; and **what the press comes back with is said where the press was made**,
// core's refusal in core's words and a queue in the reader's.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AutomationCardDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  automations: [] as AutomationCardDto[],
  launch: vi.fn(async (..._args: unknown[]) => ({ run: 1 })),
}));

vi.mock("../core/automations", () => ({
  useAutomations: () => hoisted.automations,
  launchAutomation: hoisted.launch,
}));

import { t } from "../core/i18n";
import { StartAutomation } from "./StartAutomation";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

function card(over: Partial<AutomationCardDto> = {}): AutomationCardDto {
  return { id: 7, name: "Morning round", steps: 3, archived: false, ...over };
}

async function render(over: { workspaceOpen?: boolean; folders?: string[] } = {}) {
  await act(async () => {
    root.render(createElement(StartAutomation, {
      projectId: 1,
      folders: over.folders ?? ["/w/one"],
      workspaceOpen: over.workspaceOpen ?? true,
    }));
  });
}

function button(label: string): HTMLButtonElement {
  const found = [...container.querySelectorAll("button")].find((b) => b.textContent?.includes(label));
  if (!found) throw new Error(`no button labelled ${label}`);
  return found;
}

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  hoisted.automations = [];
  hoisted.launch.mockClear();
  hoisted.launch.mockResolvedValue({ run: 1 });
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the entrance", () => {
  it("draws nothing where the project has no automations", async () => {
    await render();
    expect(container.textContent).toBe("");
  });

  it("offers each live automation by name, under one heading", async () => {
    hoisted.automations = [card(), card({ id: 8, name: "Nightly" })];
    await render();
    expect(container.textContent).toContain(t("auto.startOne"));
    expect(button("Morning round")).toBeTruthy();
    expect(button("Nightly")).toBeTruthy();
  });

  it("does not offer an archived one", async () => {
    hoisted.automations = [card(), card({ id: 8, name: "Nightly", archived: true })];
    await render();
    expect(container.textContent).not.toContain("Nightly");
  });

  it("draws nothing where every one of them is archived", async () => {
    hoisted.automations = [card({ archived: true })];
    await render();
    expect(container.textContent).toBe("");
  });
});

describe("the press", () => {
  it("carries the automation, the project, the folders and the workspace — and nothing else", async () => {
    hoisted.automations = [card()];
    await render({ workspaceOpen: false, folders: ["/w/one", "/w/two"] });
    await act(async () => { button("Morning round").click(); });
    expect(hoisted.launch).toHaveBeenCalledWith(7, 1, ["/w/one", "/w/two"], false);
  });

  it("puts a refusal in front of the reader, in the words core refused with", async () => {
    hoisted.launch.mockRejectedValue({
      code: "invalid",
      message_en: "the workspace is closed — a run draws its steps in its panes",
    });
    hoisted.automations = [card()];
    await render();
    await act(async () => { button("Morning round").click(); });
    expect(container.textContent).toContain("the workspace is closed");
  });

  it("clears the last refusal when the next press is made", async () => {
    hoisted.launch.mockRejectedValue({ code: "invalid", message_en: "the workspace is closed" });
    hoisted.automations = [card()];
    await render();
    await act(async () => { button("Morning round").click(); });
    expect(container.textContent).toContain("the workspace is closed");
    hoisted.launch.mockResolvedValue({ run: 4 });
    await act(async () => { button("Morning round").click(); });
    expect(container.textContent).not.toContain("the workspace is closed");
  });
});
