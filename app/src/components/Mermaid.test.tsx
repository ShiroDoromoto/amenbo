// @vitest-environment jsdom
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// The real mermaid needs SVG layout (getBBox and friends), which jsdom does not do, so only the IO boundary is
// swapped out. That leaves the async state transitions themselves under test: success → SVG, failure → fallback.
vi.mock("./mermaidRender", () => ({ renderMermaid: vi.fn() }));
import { renderMermaid } from "./mermaidRender";
import { Mermaid } from "./Mermaid";

// act in React 18 demands the async-environment flag.
(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const mockRender = vi.mocked(renderMermaid);
let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
  vi.clearAllMocks();
});

describe("Mermaid rendering", () => {
  it("injects the SVG on a successful render", async () => {
    mockRender.mockResolvedValue('<svg id="ok"><g></g></svg>');
    await act(async () => {
      root.render(createElement(Mermaid, { source: "graph TD\n  A-->B" }));
    });
    expect(mockRender).toHaveBeenCalledWith("graph TD\n  A-->B");
    expect(container.querySelector(".mermaid--ready")).not.toBeNull();
    expect(container.querySelector("svg#ok")).not.toBeNull();
  });

  it("falls back on a render failure (syntax error, etc.) without crashing", async () => {
    mockRender.mockRejectedValue(new Error("Parse error"));
    await act(async () => {
      root.render(createElement(Mermaid, { source: "graph TD\n  A--" }));
    });
    const failed = container.querySelector(".mermaid--failed");
    expect(failed).not.toBeNull();
    expect(failed?.textContent).toContain("図の描画に失敗しました");
    // The raw source is never lost.
    expect(failed?.textContent).toContain("graph TD");
  });
});
