// @vitest-environment jsdom
// The line the "Edit" panel hands over to the terminal is one a reader can type as it stands: the CLI
// this build installs, and the facet the CLI refuses to go without. The seam to core (the CLI's name)
// and the writes are stubbed, so what runs for real is the line the panel draws and copies.
//
// The delete press says what goes with the automation before it goes: an archived one names the runs
// and rounds it takes with it, one not archived with runs behind it is refused with why, and while a
// run is going the press stands disabled. The count, the confirm and the delete are stubbed.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const hoisted = vi.hoisted(() => ({
  /** The CLI this build installs; null where it installs none a reader can run. */
  cli: "amenbo" as string | null,
  /** What the automation has behind it, as core counts it. */
  history: { runs: 0, rounds: 0 },
  /** The sentence each confirm asked, and what the reader answers. */
  asked: [] as string[],
  answer: true,
  deleted: [] as number[],
}));

vi.mock("../core/mutations", () => ({
  fetchCliCommandName: () => Promise.resolve(hoisted.cli),
}));

vi.mock("../core/automations", () => ({
  editAutomation: () => Promise.resolve(),
  deleteAutomation: (id: number) => { hoisted.deleted.push(id); return Promise.resolve(); },
  fetchAutomationHistory: () => Promise.resolve(hoisted.history),
}));

vi.mock("../core/dialog", () => ({
  confirmDialog: (message: string) => { hoisted.asked.push(message); return Promise.resolve(hoisted.answer); },
}));

import { AutomationAboutPanel } from "./AutomationAboutPanel";
import { t, tf } from "../core/i18n";
import type { AutomationDetailDto, AutomationRunCardDto } from "../bindings/bindings";

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
  triggers: [],
  roles: [],
  placements: [],
  edges: [],
  wires: [],
  maxHeadcount: 10,
  failureValues: [],
  heldBy: [],
  unsaved: false,
  saveBlocks: [],
};

beforeEach(() => {
  hoisted.cli = "amenbo";
  hoisted.history = { runs: 0, rounds: 0 };
  hoisted.asked = [];
  hoisted.answer = true;
  hoisted.deleted = [];
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

describe("deleting the automation", () => {
  const press = () =>
    Array.from(container.querySelectorAll("button")).find(
      (b) => (b.textContent ?? "") === t("auto.about.remove"),
    )!;
  const refusal = () => container.querySelector('[role="alert"]')?.textContent ?? null;

  it("names the runs and rounds an archived one takes with it before deleting it", async () => {
    hoisted.history = { runs: 3, rounds: 7 };
    await render({ ...automation, archived: true });
    await act(async () => { press().click(); });
    expect(hoisted.asked).toEqual([tf("auto.about.removeConfirmWithRuns", { runs: 3, rounds: 7 })]);
    expect(hoisted.deleted).toEqual([9]);
  });

  it("asks as before where nothing was ever run", async () => {
    await render();
    await act(async () => { press().click(); });
    expect(hoisted.asked).toEqual([t("auto.about.removeConfirm")]);
    expect(hoisted.deleted).toEqual([9]);
  });

  it("deletes nothing when the confirm is declined", async () => {
    hoisted.history = { runs: 1, rounds: 1 };
    hoisted.answer = false;
    await render({ ...automation, archived: true });
    await act(async () => { press().click(); });
    expect(hoisted.deleted).toEqual([]);
  });

  it("refuses one not archived with runs behind it, saying to archive it first", async () => {
    hoisted.history = { runs: 2, rounds: 4 };
    await render();
    await act(async () => { press().click(); });
    expect(hoisted.asked).toEqual([]);
    expect(hoisted.deleted).toEqual([]);
    expect(refusal()).toBe(t("auto.about.removeNotArchived"));
  });

  it("stands the press disabled, with why, while a run is running or paused", async () => {
    const going = { run: 5, status: "running" } as unknown as AutomationRunCardDto;
    await render({ ...automation, archived: true, heldBy: [going] });
    expect(press().disabled).toBe(true);
    expect(container.textContent).toContain(t("auto.about.removeRunning"));
  });
});
