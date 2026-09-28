// @vitest-environment jsdom
// A test run on the build screen (`./AutomationTestPane`, `AMB-T-5804`), driven through a bare press
// and the pane it opens, so what is guarded is the press and the stepping rather than the build
// screen's drawing of them.
//
// What these guard: **the press asks in the launch's own dialog, less the files**, and hands the test
// run what the dialog was given; **the pane steps through what came back** — the step, who would do it,
// the prompt an agent would get, the way out it leaves by — marking the box the step was opened from;
// **past the last step it says how the walk ended**; and **a refusal is core's sentence**, with no
// pane opened.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AutomationLaunchAsksDto, AutomationTestRunDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  asks: { reads: "task", axes: [] } as AutomationLaunchAsksDto | null,
  walk: vi.fn(async (..._args: unknown[]): Promise<AutomationTestRunDto | null> => null),
}));

vi.mock("../core/automations", () => ({
  useLaunchAsks: () => hoisted.asks,
  NOTHING_HANDED: { files: [], title: "", notes: "", classification: [] },
  testRunAutomation: hoisted.walk,
}));
vi.mock("../core/dialog", () => ({ pickFiles: async () => [] }));

import { errText, t, tf } from "../core/i18n";
import { AutomationTestPane, useAutomationTestRun } from "./AutomationTestPane";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const WALKED: AutomationTestRunDto = {
  steps: [
    { placement: 11, name: "make_task", builtin: "make_task", exit: "done" },
    {
      placement: 12,
      name: "Write the article",
      agent: "claude",
      model: "opus",
      prompt: "Write about the rainy season.",
      folder: "/w/blog",
      exit: "done",
    },
  ],
  status: "completed",
  missing: [],
};

let container: HTMLDivElement;
let root: Root;
const walked = vi.fn();

/** A bare build screen: the press, the refusal, the dialog, the pane and the box it marks. */
function Screen() {
  const test = useAutomationTestRun(1, walked);
  return createElement("div", null,
    createElement("button", { type: "button", className: "press", onClick: () => test.test(7, "Article", ["/w/blog"]) }, "press"),
    test.refused !== null && createElement("p", { className: "refused" }, test.refused),
    test.handing,
    createElement("span", { className: "marked" }, String(test.marked ?? "")),
    test.testing !== null && createElement(AutomationTestPane, { testing: test.testing, onMove: test.move }),
  );
}

function button(label: string): HTMLButtonElement {
  const found = [...document.body.querySelectorAll("button")].find((b) => b.textContent === label);
  if (!found) throw new Error(`no button labelled ${label}`);
  return found;
}

async function click(b: HTMLButtonElement) {
  await act(async () => {
    b.click();
    await new Promise((r) => setTimeout(r, 0));
  });
}

async function type(el: HTMLInputElement | HTMLTextAreaElement, value: string) {
  await act(async () => {
    const proto = Object.getPrototypeOf(el) as object;
    Object.getOwnPropertyDescriptor(proto, "value")!.set!.call(el, value);
    el.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

beforeEach(async () => {
  hoisted.walk.mockReset();
  walked.mockReset();
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  await act(async () => root.render(createElement(Screen)));
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

/** Press, write the title and notes, and send the dialog. */
async function walk() {
  await click(container.querySelector<HTMLButtonElement>(".press")!);
  await type(document.body.querySelector<HTMLInputElement>(".launchhand__title")!, "Rainy season");
  await type(document.body.querySelector<HTMLTextAreaElement>("textarea")!, "chores");
  await click(button(t("auto.test.run")));
}

describe("a test run on the build screen", () => {
  it("asks in the launch's dialog without the files, and hands the test run what it was given", async () => {
    hoisted.walk.mockResolvedValue(WALKED);
    await click(container.querySelector<HTMLButtonElement>(".press")!);
    expect(document.body.textContent).toContain(tf("auto.test.title", { name: "Article" }));
    expect(document.body.textContent).not.toContain(t("auto.hand.files"));
    await type(document.body.querySelector<HTMLInputElement>(".launchhand__title")!, "Rainy season");
    await type(document.body.querySelector<HTMLTextAreaElement>("textarea")!, "chores");
    await click(button(t("auto.test.run")));
    expect(hoisted.walk).toHaveBeenCalledWith(7, 1, ["/w/blog"], {
      files: [], title: "Rainy season", notes: "chores", classification: [],
    });
    expect(walked).toHaveBeenCalledOnce();
  });

  it("steps through what came back, marking the box of the step it stands on", async () => {
    hoisted.walk.mockResolvedValue(WALKED);
    await walk();
    expect(container.textContent).toContain(tf("auto.test.step", { n: 1, total: 2 }));
    expect(container.textContent).toContain(t("auto.test.builtin"));
    expect(container.querySelector(".marked")!.textContent).toBe("11");
    expect(button(t("auto.test.prev")).disabled).toBe(true);

    await click(button(t("auto.test.next")));
    expect(container.textContent).toContain(tf("auto.test.step", { n: 2, total: 2 }));
    expect(container.textContent).toContain("claude · opus");
    expect(container.textContent).toContain("/w/blog");
    expect(container.querySelector(".autotest__prompt pre")!.textContent).toBe("Write about the rainy season.");
    expect(container.querySelector(".marked")!.textContent).toBe("12");

    await click(button(t("auto.test.next")));
    expect(container.textContent).toContain(t("auto.test.completed"));
    expect(button(t("auto.test.next")).disabled).toBe(true);
    expect(container.querySelector(".marked")!.textContent).toBe("");
  });

  it("says where a run would stop", async () => {
    hoisted.walk.mockResolvedValue({ steps: [], status: "failed", stoppedReason: "no_input", missing: ["theme"] });
    await walk();
    expect(container.textContent).toContain(tf("auto.test.missing", { inputs: "theme" }));
  });

  it("puts a refusal in core's words and opens no pane", async () => {
    const refusal = { code: "not_ready_automation_no_entry", message_en: "no entry" };
    hoisted.walk.mockRejectedValue(refusal);
    await walk();
    expect(container.querySelector(".refused")!.textContent).toBe(errText(refusal));
    expect(container.querySelector(".autotest")).toBeNull();
    expect(walked).not.toHaveBeenCalled();
  });
});
