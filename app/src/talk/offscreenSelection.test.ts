// @vitest-environment jsdom
import { Terminal } from "@xterm/xterm";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { holdSelectionOffscreen, renderServiceOf } from "./offscreenSelection";

// These open a real xterm.js terminal, because what they stand guard over is xterm.js's own private
// fields: a release that moves any of them fails here rather than quietly drawing every row again.

/** What the renderer behind the render service is asked to draw. */
type Renderer = {
  handleSelectionChanged(start: unknown, end: unknown, columnSelectMode: boolean): void;
  renderRows(start: number, end: number): void;
};

let term: Terminal | undefined;

beforeAll(() => {
  // jsdom has no `matchMedia`, and xterm.js watches the screen's pixel ratio with one.
  if (typeof window.matchMedia !== "function") {
    window.matchMedia = (query: string) =>
      ({
        matches: false,
        media: query,
        onchange: null,
        addEventListener: () => {},
        removeEventListener: () => {},
        addListener: () => {},
        removeListener: () => {},
        dispatchEvent: () => false,
      }) as MediaQueryList;
  }
});

afterEach(() => {
  term?.dispose();
  term = undefined;
  document.body.replaceChildren();
});

function opened() {
  const host = document.createElement("div");
  document.body.append(host);
  term = new Terminal({ cols: 20, rows: 5 });
  term.open(host);
  const render = renderServiceOf(term);
  if (!render) throw new Error("xterm.js no longer keeps its render service at `_core._renderService`");
  const renderer = (render as unknown as { _renderer: { value?: Renderer } })._renderer.value;
  if (!renderer) throw new Error("xterm.js no longer keeps its renderer at `_renderer.value`");
  return { term, render, renderer };
}

const frame = () => new Promise<void>((done) => requestAnimationFrame(() => done()));

describe("holdSelectionOffscreen", () => {
  it("takes on the xterm.js this app ships with", () => {
    const { term } = opened();
    expect(holdSelectionOffscreen(term)).toBe(true);
  });

  it("keeps a paused terminal's selection without drawing it, and owes the drawing", async () => {
    const { term, render, renderer } = opened();
    holdSelectionOffscreen(term);
    const drawn = vi.spyOn(renderer, "handleSelectionChanged").mockImplementation(() => {});
    render._isPaused = true;
    render._needsFullRefresh = false;
    render._needsSelectionRefresh = false;

    // The way output reaches it: the selection asks for a redraw on the next frame.
    term.select(0, 0, 3);
    await frame();

    expect(drawn).not.toHaveBeenCalled();
    expect(render._selectionState.start).toEqual([0, 0]);
    expect(render._selectionState.end).toEqual([3, 0]);
    expect(render._selectionState.columnSelectMode).toBe(false);
    expect(render._needsSelectionRefresh).toBe(true);
    expect(render._needsFullRefresh).toBe(true);
  });

  it("draws the held selection when the terminal is drawn again", () => {
    const { term, render, renderer } = opened();
    holdSelectionOffscreen(term);
    render._isPaused = true;
    render.handleSelectionChanged([0, 0], [3, 0], false);

    const drawn = vi.spyOn(renderer, "handleSelectionChanged").mockImplementation(() => {});
    vi.spyOn(renderer, "renderRows").mockImplementation(() => {});
    render._isPaused = false;
    // What the render service runs for the full redraw it does on coming back.
    (render as unknown as { _renderRows(start: number, end: number): void })._renderRows(0, 4);

    expect(drawn).toHaveBeenCalledWith([0, 0], [3, 0], false);
    expect(render._needsSelectionRefresh).toBe(false);
  });

  it("draws at once while the terminal is on the screen", () => {
    const { term, render, renderer } = opened();
    holdSelectionOffscreen(term);
    const drawn = vi.spyOn(renderer, "handleSelectionChanged").mockImplementation(() => {});
    render._isPaused = false;

    render.handleSelectionChanged([0, 0], [3, 0], false);

    expect(drawn).toHaveBeenCalledWith([0, 0], [3, 0], false);
  });
});
