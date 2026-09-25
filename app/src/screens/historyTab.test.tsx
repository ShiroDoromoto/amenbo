// @vitest-environment jsdom
// The "history" tab (`AMB-D-955`). Only the read is stubbed; the rows, the narrowing and the pager
// run for real.
//
// What these guard: **one page is read at a time, and the pager says where in the whole it is** —
// "21–40 of 45", with the numbers that reach the rest; **narrowing to one ending reads that ending and
// goes back to the first page**, since the page a reader was on counted rows of another list; and
// **a long history is still one row of numbers**, the far pages folded behind an ellipsis.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AutomationRunCardDto, AutomationRunHistoryDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  /** Every page the tab asked for, in order, as `<filter> <page>`, and whose runs, in the same order. */
  asked: [] as string[],
  total: 45,
  projects: [] as (number | null)[],
}));

vi.mock("../core/automations", () => ({
  // The rows number their step by the automation's picture; none is read here.
  useAutomation: () => null,
  useRunHistory: (filter: string, project: number | null, page: number): AutomationRunHistoryDto => {
    hoisted.asked.push(`${filter} ${page}`);
    hoisted.projects.push(project);
    const from = page * 20;
    const count = Math.max(0, Math.min(20, hoisted.total - from));
    return {
      runs: Array.from({ length: count }, (_, i) => run(1000 - from - i)),
      total: hoisted.total,
      pageSize: 20,
    };
  },
}));

function run(id: number): AutomationRunCardDto {
  return {
    run: id,
    project: 1,
    projectName: "amenbo",
    automation: 7,
    automationName: "Morning round",
    status: "completed",
    pauseRequested: false,
    waiting: false,
    stepsDone: 3,
    acknowledged: false,
    endedAt: "2026-09-23T00:00:00Z",
  };
}

import { formatNumber } from "../core/i18n/format";
import { t, tf } from "../core/i18n";
import { HistoryTab, pageNumbers } from "./HistoryTab";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

async function render(projectId: number | null = null) {
  await act(async () => { root.render(createElement(HistoryTab, { projectId })); });
}

function button(label: string): HTMLButtonElement {
  const found = [...container.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === label);
  if (!found) throw new Error(`no button labelled ${label}`);
  return found;
}
const count = () => container.querySelector(".autohist__count")?.textContent ?? "";
const said = (from: number, to: number, total: number) =>
  tf("auto.history.count", { from: formatNumber(from), to: formatNumber(to), total: formatNumber(total) });

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  hoisted.asked = [];
  hoisted.projects = [];
  hoisted.total = 45;
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the history tab", () => {
  // Every run here is over, and how it ended is what a row is read for — so it is on a chip.
  it("says on each row how the run ended", async () => {
    await render();
    expect(container.querySelector(".autorun__chip")?.textContent).toBe(t("auto.run.completed"));
  });

  it("reads one page at a time and says where in the whole it is", async () => {
    await render();
    expect(container.querySelectorAll(".autorun")).toHaveLength(20);
    expect(count()).toBe(said(1, 20, 45));
    expect(button(t("auto.history.prev")).disabled).toBe(true);

    await act(async () => { button(t("auto.history.next")).click(); });
    expect(count()).toBe(said(21, 40, 45));
    await act(async () => { button(formatNumber(3)).click(); });
    expect(container.querySelectorAll(".autorun")).toHaveLength(5);
    expect(count()).toBe(said(41, 45, 45));
    expect(button(t("auto.history.next")).disabled).toBe(true);
    expect(hoisted.asked[hoisted.asked.length - 1]).toBe("all 2");
  });

  it("narrows to one ending and goes back to the first page", async () => {
    await render();
    await act(async () => { button(t("auto.history.next")).click(); });
    await act(async () => { button(t("auto.run.failed")).click(); });
    expect(hoisted.asked[hoisted.asked.length - 1]).toBe("failed 0");
  });

  it("says so when nothing matches", async () => {
    hoisted.total = 0;
    await render();
    expect(container.textContent).toContain(t("auto.history.empty"));
    expect(container.querySelector(".autohist__pager")).toBeNull();
  });

  it("folds the far pages of a long history behind an ellipsis", () => {
    expect(pageNumbers(0, 3)).toEqual([0, 1, 2]);
    expect(pageNumbers(5, 12)).toEqual([0, null, 3, 4, 5, 6, 7, null, 11]);
    expect(pageNumbers(11, 12)).toEqual([0, null, 9, 10, 11]);
  });
});

// The two entrances (`AMB-D-954`): from a project the store is asked for that project's runs, and the
// rows leave the project off; from the sidebar it is asked for every project's, each row naming one.
describe("the history's reach", () => {
  it("asks for one project's runs from that project, and leaves the project off the rows", async () => {
    await render(1);
    expect(new Set(hoisted.projects)).toEqual(new Set([1]));
    expect(container.querySelector(".autorun__project")).toBeNull();
  });

  it("asks for every project's runs from the sidebar, each row naming its project", async () => {
    await render(null);
    expect(new Set(hoisted.projects)).toEqual(new Set([null]));
    expect(container.querySelector(".autorun__project")?.textContent).toBe("amenbo");
  });
});
