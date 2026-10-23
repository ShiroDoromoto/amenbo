// @vitest-environment jsdom
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { DueChip } from "./atoms";
import type { Schedule } from "../mock/types";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

const chip = () => container.querySelector(".chip.due");
const render = (due: string | Schedule | null) =>
  act(() => root.render(createElement(DueChip, { due: typeof due === "string" ? { kind: "day", day: due } : due })));
// An instant at a local time, so the day it falls on is the same whatever timezone the test runs in.
const at = (d: number, h: number, m = 0): Schedule => ({ kind: "at", at: new Date(2026, 5, d, h, m).toISOString() });

beforeEach(() => {
  vi.useFakeTimers();
  // A local noon, so that the day is the same one whatever timezone the test runs in.
  vi.setSystemTime(new Date(2026, 5, 21, 12, 0, 0));
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
  vi.useRealTimers();
});

describe("DueChip", () => {
  it("colours against the real today, not a day baked in at build time", () => {
    render("2026-06-20");
    expect(chip()?.className).toContain("due--overdue");
    render("2026-06-21");
    expect(chip()?.className).toContain("due--today");
    render("2026-06-22");
    expect(chip()?.className).toContain("due--tomorrow");
    render("2026-06-23");
    expect(chip()?.className).toContain("due--future");
  });

  it("moves with the clock — one due date walks down every step as the days pass", () => {
    render("2026-06-23");
    expect(chip()?.className).toContain("due--future");
    act(() => { vi.setSystemTime(new Date(2026, 5, 22, 12, 0, 0)); });
    render("2026-06-23");
    expect(chip()?.className).toContain("due--tomorrow");
    act(() => { vi.setSystemTime(new Date(2026, 5, 23, 12, 0, 0)); });
    render("2026-06-23");
    expect(chip()?.className).toContain("due--today");
    act(() => { vi.setSystemTime(new Date(2026, 5, 24, 12, 0, 0)); });
    render("2026-06-23");
    expect(chip()?.className).toContain("due--overdue");
  });

  it("judges an instant by the reader's day it falls on, until the instant has passed", () => {
    render(at(21, 23));
    expect(chip()?.className).toContain("due--today");
    render(at(22, 0, 30));
    expect(chip()?.className).toContain("due--tomorrow");
    // Noon has come and gone: the day is still today, but the deadline is behind.
    render(at(21, 11));
    expect(chip()?.className).toContain("due--overdue");
  });

  it("carries the instant's time beside the day", () => {
    render(at(22, 9, 30));
    expect(chip()?.textContent).toMatch(/tomorrow 09:30\sAM/);
  });

  // The wording is `Intl`'s, in the locale dates are written in — English here, nothing being set.
  it("words the date rather than printing it", () => {
    render("2026-06-22");
    expect(chip()?.textContent).toContain("tomorrow");
    render("2026-06-21");
    expect(chip()?.textContent).toContain("today");
    render("2026-06-25");
    expect(chip()?.textContent).toContain("in 4 days");
  });

  it("draws nothing without a due date", () => {
    render(null);
    expect(chip()).toBeNull();
  });
});
