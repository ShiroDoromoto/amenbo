// @vitest-environment jsdom
// The wiring behind the flourish that shows an external (AI/CLI) write landing: subscribe to the reflect
// notification (notifyStoreChangeReflected), add the transient class `topbar__ws--reflect`, and drop it a few
// hundred ms later. A burst of writes folds into a single flash. (How the flash *looks* is the CSS's business.)
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { TopBar } from "./TopBar";
import { notifyStoreChangeReflected } from "../core/snapshot";

// React 18's act demands the async-environment flag.
(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

const props = { onBack: () => {}, onForward: () => {}, canBack: false, canForward: false, sidebarCollapsed: false, onToggleSidebar: () => {} };
const ws = () => container.querySelector(".topbar__ws")!;

beforeEach(() => {
  vi.useFakeTimers();
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  act(() => root.render(createElement(TopBar, props)));
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
  vi.useRealTimers();
});

describe("TopBar reflect flash", () => {
  it("a reflect notification adds the reflect class, and it drops a few hundred ms later", () => {
    expect(ws().classList.contains("topbar__ws--reflect")).toBe(false);

    act(() => notifyStoreChangeReflected({ reason: "live", at: 1 }));
    expect(ws().classList.contains("topbar__ws--reflect")).toBe(true);

    act(() => vi.advanceTimersByTime(700));
    expect(ws().classList.contains("topbar__ws--reflect")).toBe(false);
  });

  it("consecutive reflections during the flash collapse into one (debounce)", () => {
    act(() => notifyStoreChangeReflected({ reason: "live", at: 1 }));
    // A second one arriving mid-flash does not extend the timer; the class still drops at the end of the first window.
    act(() => vi.advanceTimersByTime(400));
    act(() => notifyStoreChangeReflected({ reason: "live", at: 2 }));
    expect(ws().classList.contains("topbar__ws--reflect")).toBe(true);

    act(() => vi.advanceTimersByTime(300)); // 700ms in total since the first one.
    expect(ws().classList.contains("topbar__ws--reflect")).toBe(false);
  });
});
