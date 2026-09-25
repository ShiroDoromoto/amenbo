// @vitest-environment jsdom
// The way into a launch that is not the build screen: the empty frame (`AMB-T-5260`).
//
// What these guard: **the press carries the automation and what the person hands over, and nothing
// else** — no task, no folder choice, no narrowing — so it is the same run the automations tab would
// start; **every press asks what to hand over first** (`AMB-D-970`), and putting that dialog away
// starts nothing;
// **a project with no automations draws no entrance**, since a heading over an empty row would put
// the subject in front of a reader who has never met it; **an archived one is not offered**, which
// is what archiving is for; and **what the press comes back with is said where the press was made**,
// core's refusal in core's words and a queue in the reader's; and **a run that starts is gone to**
// (`AMB-T-5530`), while a refused press moves nothing.
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
vi.mock("../core/dialog", () => ({ pickFiles: async () => ["/w/brief.md", "/w/shot.png"] }));

import { t } from "../core/i18n";
import { StartAutomation } from "./StartAutomation";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

function card(over: Partial<AutomationCardDto> = {}): AutomationCardDto {
  return { id: 7, name: "Morning round", notes: "", placements: 3, archived: false, ...over };
}

const goToRun = vi.fn();

async function render(over: { workspaceOpen?: boolean; folders?: string[] } = {}) {
  await act(async () => {
    root.render(createElement(StartAutomation, {
      projectId: 1,
      folders: over.folders ?? ["/w/one"],
      workspaceOpen: over.workspaceOpen ?? true,
      onGoToRun: goToRun,
    }));
  });
}

function button(label: string): HTMLButtonElement {
  const found = [...container.querySelectorAll("button")].find((b) => b.textContent?.includes(label));
  if (!found) throw new Error(`no button labelled ${label}`);
  return found;
}


/** Answer the dialog every start opens (`../components/LaunchHanding`) with nothing handed over. */
async function handOver() {
  await act(async () => {
    document.body.querySelector<HTMLButtonElement>(".modal__card .btn--primary")!.click();
    await new Promise((r) => setTimeout(r, 0));
  });
}

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  hoisted.automations = [];
  hoisted.launch.mockClear();
  hoisted.launch.mockResolvedValue({ run: 1 });
  goToRun.mockClear();
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
  it("carries the automation, the project, the folders and the workspace, and hands over nothing where nothing was given", async () => {
    hoisted.automations = [card()];
    await render({ workspaceOpen: false, folders: ["/w/one", "/w/two"] });
    await act(async () => { button("Morning round").click(); });
    await handOver();
    expect(hoisted.launch).toHaveBeenCalledWith(7, 1, ["/w/one", "/w/two"], false, { text: "", files: [] });
  });

  it("hands over the text typed and the files picked in the dialog the press opens", async () => {
    hoisted.automations = [card()];
    await render({ folders: ["/w/one"] });
    await act(async () => { button("Morning round").click(); });
    expect(hoisted.launch).not.toHaveBeenCalled();
    const box = document.body.querySelector<HTMLTextAreaElement>(".modal__card textarea")!;
    await act(async () => {
      Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")!.set!.call(box, "  fix the login page  ");
      box.dispatchEvent(new Event("input", { bubbles: true }));
    });
    await act(async () => {
      document.body.querySelector<HTMLButtonElement>(".launchhand__add")!.click();
      await new Promise((r) => setTimeout(r, 0));
    });
    // Each picked file is listed by its name, and one can be taken off again before the start.
    const names = () => [...document.body.querySelectorAll(".launchhand__name")].map((one) => one.textContent);
    expect(names()).toEqual(["brief.md", "shot.png"]);
    await act(async () => {
      document.body.querySelectorAll<HTMLButtonElement>(".launchhand__drop")[1]!.click();
    });
    expect(names()).toEqual(["brief.md"]);
    await handOver();
    expect(hoisted.launch).toHaveBeenCalledWith(7, 1, ["/w/one"], true, {
      text: "fix the login page",
      files: ["/w/brief.md"],
    });
  });

  it("starts nothing where the dialog is put away", async () => {
    hoisted.automations = [card()];
    await render();
    await act(async () => { button("Morning round").click(); });
    const cancel = [...document.body.querySelectorAll<HTMLButtonElement>(".modal__card .btn")]
      .find((one) => one.textContent === t("auto.add.cancel"))!;
    await act(async () => { cancel.click(); });
    expect(document.body.querySelector(".modal__card")).toBeNull();
    expect(hoisted.launch).not.toHaveBeenCalled();
  });

  it("puts a refusal in front of the reader, in the words core refused with", async () => {
    hoisted.launch.mockRejectedValue({
      code: "invalid",
      message_en: "the workspace is closed — a run draws its steps in its panes",
    });
    hoisted.automations = [card()];
    await render();
    await act(async () => { button("Morning round").click(); });
    await handOver();
    expect(container.textContent).toContain("the workspace is closed");
    expect(goToRun).not.toHaveBeenCalled();
  });

  it("goes to the pane of the run it started", async () => {
    hoisted.launch.mockResolvedValue({ run: 31 });
    hoisted.automations = [card()];
    await render();
    await act(async () => { button("Morning round").click(); });
    await handOver();
    expect(goToRun).toHaveBeenCalledWith(1, 31);
  });

  it("clears the last refusal when the next press is made", async () => {
    hoisted.launch.mockRejectedValue({ code: "invalid", message_en: "the workspace is closed" });
    hoisted.automations = [card()];
    await render();
    await act(async () => { button("Morning round").click(); });
    await handOver();
    expect(container.textContent).toContain("the workspace is closed");
    hoisted.launch.mockResolvedValue({ run: 4 });
    await act(async () => { button("Morning round").click(); });
    await handOver();
    expect(container.textContent).not.toContain("the workspace is closed");
  });
});
