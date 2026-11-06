// @vitest-environment jsdom
// The line the "Edit" panel hands over to the terminal is one a reader can type as it stands: the CLI
// this build installs, and the facet the CLI refuses to go without. The seam to core (the CLI's name)
// and the writes are stubbed, so what runs for real is the line the panel draws and copies — and the
// patch the most-runs-at-once number hands to the write.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const hoisted = vi.hoisted(() => ({
  /** The CLI this build installs; null where it installs none a reader can run. */
  cli: "amenbo" as string | null,
  /** Each patch the panel wrote, in order. */
  edits: [] as unknown[],
}));

vi.mock("../core/mutations", () => ({
  fetchCliCommandName: () => Promise.resolve(hoisted.cli),
}));

vi.mock("../core/automations", () => ({
  editAutomation: (_id: number, patch: unknown) => {
    hoisted.edits.push(patch);
    return Promise.resolve();
  },
  deleteAutomation: () => Promise.resolve(),
  MAX_CONCURRENT_RUNS: 4,
}));

import { AutomationAboutPanel } from "./AutomationAboutPanel";
import { t } from "../core/i18n";
import type { AutomationDetailDto } from "../bindings/bindings";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
let clipboard: string[];

const automation: AutomationDetailDto = {
  id: 9,
  projectId: 1,
  name: "one at a time",
  notes: "",
  archived: false,
  maxConcurrentRuns: 1,
  placements: [],
  edges: [],
  wires: [],
  heldBy: [],
  unsaved: false,
  saveBlocks: [],
};

beforeEach(() => {
  hoisted.cli = "amenbo";
  hoisted.edits = [];
  clipboard = [];
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: { writeText: (s: string) => { clipboard.push(s); return Promise.resolve(); } },
  });
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

// Awaited, so the build's CLI name — asked for at mount — has landed before anything is read.
const render = (shown: AutomationDetailDto = automation) =>
  act(async () => { root.render(createElement(AutomationAboutPanel, { automation: shown, onDeleted: () => {} })); });

const line = () => container.querySelector(".autoabout__command")?.textContent ?? null;

describe("the line the panel hands to the terminal", () => {
  it("names the CLI this build installs and says the facet, so it runs as typed", async () => {
    hoisted.cli = "amenbo-dev";
    await render();
    expect(line()).toBe("amenbo-dev automation start 9 --actor human");
  });

  it("copies the same line it draws", async () => {
    await render();
    const copy = Array.from(container.querySelectorAll("button")).find(
      (b) => (b.textContent ?? "") === t("auto.about.copy"),
    )!;
    await act(async () => { copy.click(); });
    expect(clipboard).toEqual(["amenbo automation start 9 --actor human"]);
  });

  it("draws no line where the build installs no CLI a reader can run", async () => {
    hoisted.cli = null;
    await render();
    expect(line()).toBeNull();
  });
});

describe("the most runs at once", () => {
  const field = () => container.querySelector<HTMLInputElement>('input[type="number"]')!;

  const type = (input: HTMLInputElement, value: string) => {
    const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
    set.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  };
  const leave = (input: HTMLInputElement) => input.dispatchEvent(new FocusEvent("focusout", { bubbles: true }));

  it("is always drawn, with the stored number and the cap", async () => {
    await render();
    expect(field().value).toBe("1");
    expect(field().max).toBe("4");
  });

  it("writes the number when the caret leaves it", async () => {
    await render({ ...automation, maxConcurrentRuns: 2 });
    await act(async () => { type(field(), "3"); });
    await act(async () => { leave(field()); });
    expect(hoisted.edits).toEqual([{ maxConcurrentRuns: 3 }]);
  });

  it("does not write a number under 1 or past the cap, and puts the stored one back", async () => {
    await render({ ...automation, maxConcurrentRuns: 2 });
    for (const wrong of ["0", "5"]) {
      await act(async () => { type(field(), wrong); });
      await act(async () => { leave(field()); });
      expect(hoisted.edits).toEqual([]);
      expect(field().value).toBe("2");
    }
  });
});
