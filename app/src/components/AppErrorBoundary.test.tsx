// @vitest-environment jsdom
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { AppErrorBoundary } from "./AppErrorBoundary";

// React 18's act() requires this environment flag to be set.
(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  // React and componentDidCatch log the caught error to console.error; that is expected here, so silence it.
  vi.spyOn(console, "error").mockImplementation(() => {});
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
  vi.restoreAllMocks();
});

function Boom(): never {
  throw new Error("kaboom");
}

describe("AppErrorBoundary", () => {
  it("renders children straight through when they are fine (no false trip)", () => {
    act(() => {
      root.render(createElement(AppErrorBoundary, null, createElement("span", null, "ok content")));
    });
    expect(container.textContent).toContain("ok content");
    expect(container.querySelector('[role="alert"]')).toBeNull();
  });

  it("falls back when a child throws during render, instead of going blank", () => {
    act(() => {
      root.render(createElement(AppErrorBoundary, null, createElement(Boom)));
    });
    const alert = container.querySelector('[role="alert"]');
    expect(alert).not.toBeNull();
    expect(alert?.querySelector("button")).not.toBeNull();
    expect(alert?.textContent).toContain("kaboom");
  });
});
