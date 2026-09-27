// @vitest-environment jsdom
// The press every entrance to a launch makes (`useAutomationStart`), driven here through a bare row of
// buttons so that what is guarded is the press and not any one screen's drawing of it.
//
// What these guard: **the press carries the automation and what the person hands over, and nothing
// else** — no task, no folder choice, no narrowing; **every press asks what to hand over first**
// (`AMB-D-970`), and putting that dialog away starts nothing; **the dialog asks for what the entry
// reads and nothing else** — a title, notes, a value per axis and files for the built-in that files a
// task, nothing for any other — and waits for what a task cannot be filed without; **what the press
// comes back with is said where the press was made**, core's refusal in core's words; and **a run that
// starts is gone to** (`AMB-T-5530`), while a refused press moves nothing.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AutomationLaunchAsksDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  asks: { reads: "nothing", axes: [] } as AutomationLaunchAsksDto | null,
  launch: vi.fn(async (..._args: unknown[]) => ({ run: 1 })),
}));

vi.mock("../core/automations", () => ({
  useLaunchAsks: () => hoisted.asks,
  NOTHING_HANDED: { files: [], title: "", notes: "", classification: [] },
  launchAutomation: hoisted.launch,
}));
vi.mock("../core/dialog", () => ({ pickFiles: async () => ["/w/brief.md", "/w/shot.png"] }));

import { errText, t } from "../core/i18n";
import { RefNavProvider } from "../core/refNav";
import { useAutomationStart } from "./StartAutomation";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

/** A bare entrance: one automation's button, and what the press comes back with under them. */
function Presses({ folders, workspaceOpen }: { folders: readonly string[]; workspaceOpen: boolean }) {
  const { start, refused, starting, handing } = useAutomationStart(1, workspaceOpen, goToRun);
  return createElement("div", null,
    createElement("button", { type: "button", disabled: starting, onClick: () => start(7, "Morning round", folders) }, "Morning round"),
    refused !== null && createElement("p", null, refused),
    handing,
  );
}

const goToRun = vi.fn();
const openWorkspace = vi.fn();

async function render(over: { workspaceOpen?: boolean; folders?: string[] } = {}) {
  await act(async () => {
    root.render(createElement(RefNavProvider, {
      value: { openWorkspace },
      children: createElement(Presses, {
        folders: over.folders ?? ["/w/one"],
        workspaceOpen: over.workspaceOpen ?? true,
      }),
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
  hoisted.asks = { reads: "nothing", axes: [] };
  hoisted.launch.mockClear();
  hoisted.launch.mockResolvedValue({ run: 1 });
  goToRun.mockClear();
  openWorkspace.mockClear();
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the press", () => {
  it("carries the automation, the project, the folders and the workspace, and hands over nothing where nothing was given", async () => {
    await render({ folders: ["/w/one", "/w/two"] });
    await act(async () => { button("Morning round").click(); });
    await handOver();
    expect(hoisted.launch).toHaveBeenCalledWith(7, 1, ["/w/one", "/w/two"], true, { files: [], title: "", notes: "", classification: [] });
  });

  it("hands over the files picked in the dialog the press opens, with the task it files", async () => {
    hoisted.asks = { reads: "task", axes: [] };
    await render({ folders: ["/w/one"] });
    await act(async () => { button("Morning round").click(); });
    expect(hoisted.launch).not.toHaveBeenCalled();
    const box = document.body.querySelector<HTMLInputElement>(".launchhand__title")!;
    await act(async () => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(box, "fix the login page");
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
      files: ["/w/brief.md"],
      title: "fix the login page",
      notes: "",
      classification: [],
    });
  });

  it("starts nothing where the dialog is put away", async () => {
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
    await render();
    await act(async () => { button("Morning round").click(); });
    await handOver();
    expect(container.textContent).toContain("the workspace is closed");
    expect(goToRun).not.toHaveBeenCalled();
  });

  it("goes to the pane of the run it started", async () => {
    hoisted.launch.mockResolvedValue({ run: 31 });
    await render();
    await act(async () => { button("Morning round").click(); });
    await handOver();
    expect(goToRun).toHaveBeenCalledWith(1, 31);
  });

  it("clears the last refusal when the next press is made", async () => {
    hoisted.launch.mockRejectedValue({ code: "invalid", message_en: "the workspace is closed" });
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

describe("what the dialog asks for", () => {
  /** Type into the field `selector` finds in the dialog. */
  async function typeInto(selector: string, value: string) {
    const box = document.body.querySelector<HTMLInputElement | HTMLTextAreaElement>(`.modal__card ${selector}`)!;
    const proto = box instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
    await act(async () => {
      Object.getOwnPropertyDescriptor(proto, "value")!.set!.call(box, value);
      box.dispatchEvent(new Event("input", { bubbles: true }));
    });
  }

  async function choose(index: number, value: string) {
    const select = document.body.querySelectorAll<HTMLSelectElement>(".modal__card .launchhand__axis")[index]!;
    await act(async () => {
      select.value = value;
      select.dispatchEvent(new Event("change", { bubbles: true }));
    });
  }

  const startButton = () => document.body.querySelector<HTMLButtonElement>(".modal__card .btn--primary")!;

  it("asks an entry that files a task for its title, notes and a value per axis, and waits for what it needs", async () => {
    hoisted.asks = {
      reads: "task",
      axes: [
        { name: "職能", values: ["実装", "設計"], required: false },
        { name: "種別", values: ["不具合"], required: true },
      ],
    };
    await render();
    await act(async () => { button("Morning round").click(); });
    const labels = [...document.body.querySelectorAll(".modal__card .autostep__label")].map((one) => one.textContent);
    expect(labels).toEqual([
      t("auto.hand.taskTitle") + t("auto.hand.required"),
      t("auto.hand.taskNotes"),
      "職能",
      "種別" + t("auto.hand.required"),
      t("auto.hand.files"),
    ]);

    // No title, and no value on the axis the project requires: the press waits.
    expect(startButton().disabled).toBe(true);
    await typeInto(".launchhand__title", "  the login page loses a field  ");
    expect(startButton().disabled).toBe(true);
    await choose(1, "不具合");
    expect(startButton().disabled).toBe(false);
    await typeInto("textarea", "seen on a phone");

    await handOver();
    expect(hoisted.launch).toHaveBeenCalledWith(7, 1, ["/w/one"], true, {
      files: [],
      title: "the login page loses a field",
      notes: "seen on a phone",
      classification: [["種別", "不具合"]],
    });
  });

  it("asks an entry that reads nothing for nothing", async () => {
    hoisted.asks = { reads: "nothing", axes: [] };
    await render();
    await act(async () => { button("Morning round").click(); });
    expect(document.body.querySelector(".modal__card textarea")).toBeNull();
    expect(document.body.querySelector(".modal__card input")).toBeNull();
    expect(document.body.querySelector(".modal__card")!.textContent).toContain(t("auto.hand.nothing"));
    await handOver();
    expect(hoisted.launch).toHaveBeenCalledWith(7, 1, ["/w/one"], true, {
      files: [],
      title: "",
      notes: "",
      classification: [],
    });
  });

  it("offers no start until what the entry reads is answered", async () => {
    hoisted.asks = null;
    await render();
    await act(async () => { button("Morning round").click(); });
    expect(startButton().disabled).toBe(true);
  });
});

describe("a press while the workspace is closed", () => {
  // Core refuses it too, but only once the launch is sent — after the dialog has been answered, which
  // throws away whatever was written there (`AMB-T-5590`).
  const closedLine = errText({ code: "invalid_automation_workspace_closed", message_en: "" });

  it("is refused at the press, before the dialog asks anything", async () => {
    expect(closedLine).not.toBe(""); // a blank line would be in every screen
    await render({ workspaceOpen: false });
    await act(async () => { button("Morning round").click(); });
    expect(document.body.querySelector(".modal__card")).toBeNull();
    expect(hoisted.launch).not.toHaveBeenCalled();
    expect(container.textContent).toContain(closedLine);
  });

  it("offers the way to the workspace beside the refusal", async () => {
    await render({ workspaceOpen: false });
    await act(async () => { button("Morning round").click(); });
    await act(async () => { button(t("auto.launch.openWorkspace")).click(); });
    expect(openWorkspace).toHaveBeenCalledTimes(1);
  });

  it("takes the refusal away once the workspace opens", async () => {
    await render({ workspaceOpen: false });
    await act(async () => { button("Morning round").click(); });
    await render({ workspaceOpen: true });
    expect(container.textContent).not.toContain(closedLine);
  });
});
