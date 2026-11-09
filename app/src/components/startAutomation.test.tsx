// @vitest-environment jsdom
// The press every entrance to a launch makes (`useAutomationStart`), driven here through a bare row of
// buttons so that what is guarded is the press and not any one screen's drawing of it.
//
// What these guard: **the press only opens the run, and carries nothing** (`AMB-D-1123`) — the
// automation, the project, the folders and whether the workspace is standing, and no title, notes,
// value or file; **no dialog opens on the press**, which starts the run at once; **what the press
// comes back with is said where the press was made**, core's refusal in core's words; and **a launch
// that starts is gone to** (`AMB-T-5530`) — with the run it started in front, or with none where the
// automation waits for a task — while a refused press moves nothing.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const hoisted = vi.hoisted(() => ({
  launch: vi.fn(async (..._args: unknown[]): Promise<{ run: number | null; launch?: number }> => ({ run: 1 })),
}));

vi.mock("../core/automations", () => ({ launchAutomation: hoisted.launch }));

import { errText, t } from "../core/i18n";
import { RefNavProvider } from "../core/refNav";
import { useAutomationStart } from "./StartAutomation";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

/** A bare entrance: one automation's button, and what the press comes back with under them. */
function Presses({ folders, workspaceOpen }: { folders: readonly string[]; workspaceOpen: boolean }) {
  const { start, refused, starting } = useAutomationStart(1, workspaceOpen, goToRun);
  return createElement("div", null,
    createElement("button", { type: "button", disabled: starting, onClick: () => void start(7, folders) }, "Morning round"),
    refused !== null && createElement("p", null, refused),
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

/** Press the automation's button, and let the launch it sends land. */
async function press() {
  await act(async () => {
    button("Morning round").click();
    await new Promise((r) => setTimeout(r, 0));
  });
}

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
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
  it("starts at once, carrying the automation, the project, the folders and the workspace, and nothing else", async () => {
    await render({ folders: ["/w/one", "/w/two"] });
    await press();
    expect(document.body.querySelector(".modal__card")).toBeNull();
    expect(hoisted.launch).toHaveBeenCalledTimes(1);
    expect(hoisted.launch.mock.calls[0]).toEqual([7, 1, ["/w/one", "/w/two"], true]);
  });

  it("puts a refusal in front of the reader, in the words core refused with", async () => {
    hoisted.launch.mockRejectedValue({
      code: "invalid",
      message_en: "the workspace is closed — a run draws its steps in its panes",
    });
    await render();
    await press();
    expect(container.textContent).toContain("the workspace is closed");
    expect(goToRun).not.toHaveBeenCalled();
  });

  it("goes to the pane of the launch it started, with the run it started in front", async () => {
    hoisted.launch.mockResolvedValue({ run: 31, launch: 5 });
    await render();
    await press();
    expect(goToRun).toHaveBeenCalledWith(1, 31, 5);
  });

  it("goes to the launch's pane, with no run in front, when the automation waits for a task to take", async () => {
    hoisted.launch.mockResolvedValue({ run: null, launch: 5 });
    await render();
    await press();
    expect(hoisted.launch).toHaveBeenCalledTimes(1);
    expect(goToRun).toHaveBeenCalledWith(1, null, 5);
  });

  it("goes nowhere when the answer names neither a run nor a launch", async () => {
    hoisted.launch.mockResolvedValue({ run: null });
    await render();
    await press();
    expect(goToRun).not.toHaveBeenCalled();
  });

  it("clears the last refusal when the next press is made", async () => {
    hoisted.launch.mockRejectedValue({ code: "invalid", message_en: "the workspace is closed" });
    await render();
    await press();
    expect(container.textContent).toContain("the workspace is closed");
    hoisted.launch.mockResolvedValue({ run: 4 });
    await press();
    expect(container.textContent).not.toContain("the workspace is closed");
  });
});

describe("a press while the workspace is closed", () => {
  // Core refuses it too, but in its sentence alone; the press says it with the way to the workspace
  // beside it (`AMB-T-5590`).
  const closedLine = errText({ code: "invalid_automation_workspace_closed", message_en: "" });

  it("is refused at the press, before the launch is sent", async () => {
    expect(closedLine).not.toBe(""); // a blank line would be in every screen
    await render({ workspaceOpen: false });
    await press();
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
