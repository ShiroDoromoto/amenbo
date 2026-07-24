// @vitest-environment jsdom
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { DueChip } from "./atoms";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

const chip = () => container.querySelector(".chip.due");
const render = (due: string | null, label: string | null = null) =>
  act(() => root.render(createElement(DueChip, { due, label })));

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
    expect(chip()?.className).toContain("due--future");
  });

  it("moves with the clock — the same due date reads future today and overdue later", () => {
    render("2026-06-22");
    expect(chip()?.className).toContain("due--future");
    act(() => { vi.setSystemTime(new Date(2026, 5, 23, 12, 0, 0)); });
    render("2026-06-22");
    expect(chip()?.className).toContain("due--overdue");
  });

  it("judges by the day even when a time is attached", () => {
    render("2026-06-21T23:00:00Z");
    expect(chip()?.className).toContain("due--today");
  });

  it("shows the label when there is one, and the raw date otherwise", () => {
    render("2026-06-22", "明日");
    expect(chip()?.textContent).toContain("明日");
    render("2026-06-22");
    expect(chip()?.textContent).toContain("2026-06-22");
  });

  it("draws nothing without a due date", () => {
    render(null);
    expect(chip()).toBeNull();
  });
});
