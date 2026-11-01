// @vitest-environment jsdom
// What is written in one pane's box is not a reason to draw the others (`./TerminalPane`).
//
// The window holds every pane's draft (`../talk/layout`), so a key pressed in one box draws the whole
// face again, with every handler and every `start` made afresh. What is pinned here is that the pane
// next to it is not drawn for that — and that the pane written in still calls the newest handler it
// was handed, though it was not drawn again for being handed it.
import { act, createElement, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PaneEvents } from "../talk/terminal";
import { samePane, TerminalPane } from "./TerminalPane";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const hoisted = vi.hoisted(() => ({
  /** What each frame was handed, by the folder it was started in, so the test can open a session. */
  events: new Map<string, PaneEvents>(),
  /** Puts a draft in the window, as a key pressed in a pane's box would. */
  write: null as ((frame: string, text: string) => void) | null,
  /** Which render of the window handed the handler that was called, as each call landed. */
  calls: [] as Array<{ frame: string; text: string; render: number }>,
  /** How many times each pane was drawn, by frame. */
  drawn: {} as Record<string, number>,
}));

// Drawn by the pane on each of its own renders once a session is open, and by nothing else — so it
// counts the pane's drawings and not the window's, which a profiler round the pane would also count.
vi.mock("./PaneModel", () => ({
  PaneModel: ({ frame }: { frame: string }) => {
    hoisted.drawn[frame] = (hoisted.drawn[frame] ?? 0) + 1;
    return null;
  },
}));

vi.mock("../talk/agent", () => ({
  mountAgentFrame: (host: HTMLElement, _lang: string, on: PaneEvents, start: { cwd?: string }) => {
    hoisted.events.set(start.cwd ?? "", on);
    host.append(document.createElement("textarea"));
    return Promise.resolve(() => {});
  },
}));
vi.mock("../core/hostDrop", () => ({ watchHostDrop: vi.fn(async () => () => {}) }));
vi.mock("../core/notice", () => ({ pushNotice: vi.fn() }));
vi.mock("../core/ipc", () => ({ invoke: vi.fn(async () => {}) }));
vi.mock("../talk/plate", () => ({
  mountPlate: () => ({
    opened: () => {}, closed: () => {}, named: () => {}, took: () => {}, stated: () => {}, numbered: () => {}, timed: () => {},
    focused: () => {}, stop: () => {},
  }),
}));

let container: HTMLDivElement;
let root: Root;
/** How many times the window has drawn. */
let renders: number;

beforeEach(() => {
  hoisted.events.clear();
  hoisted.calls = [];
  hoisted.drawn = {};
  renders = 0;
  localStorage.clear();
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

const names = new Map<string, string>();

/** Two panes side by side, drawn the way the face draws them: every prop made afresh on each render. */
function Window() {
  const [written, setWritten] = useState<Record<string, string>>({ a: "", b: "" });
  renders += 1;
  const render = renders;
  hoisted.write = (frame, text) => setWritten((was) => ({ ...was, [frame]: text }));
  return createElement("div", null, ...["a", "b"].map((frame) => createElement(
    TerminalPane, {
      key: frame,
      frame,
      at: { gridColumn: frame === "a" ? "1 / span 1" : "2 / span 1" },
      hue: 199,
      project: 3,
      names,
      start: { frame, cwd: `/work/${frame}` },
      autoStart: true,
      focused: frame === "a",
      written: written[frame],
      onWrite: (id: string, text: string) => {
        hoisted.calls.push({ frame: id, text, render });
        setWritten((was) => ({ ...was, [id]: text }));
      },
      composeOpen: true,
      onFold: () => {},
      onOpened: () => {},
      onSaid: () => {},
      onPath: () => {},
      onClosed: () => {},
      onDrop: () => {},
      onName: () => {},
      onFocus: () => {},
    },
  )));
}

async function opened(frame: string): Promise<void> {
  await act(async () => { hoisted.events.get(`/work/${frame}`)?.opened(`session-${frame}`, `/work/${frame}`, null); });
  await act(async () => { await Promise.resolve(); });
}

/** Type into a pane's box, as the browser hands the change over. */
async function type(frame: string, text: string): Promise<void> {
  const field = container.querySelectorAll<HTMLTextAreaElement>(".compose__box")[frame === "a" ? 0 : 1];
  await act(async () => {
    const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")?.set;
    setter?.call(field, text);
    field.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

describe("a pane is drawn again only for what it draws", () => {
  it("leaves the pane next to it alone while one pane's draft changes", async () => {
    await act(async () => { root.render(createElement(Window)); });
    await opened("a");
    await opened("b");
    const before = { ...hoisted.drawn };

    await act(async () => { hoisted.write?.("a", "hello"); });
    await act(async () => { hoisted.write?.("a", "hello there"); });

    expect(hoisted.drawn.a).toBeGreaterThan(before.a);
    expect(hoisted.drawn.b).toBe(before.b);
  });

  it("draws the pane whose own draft changed", async () => {
    await act(async () => { root.render(createElement(Window)); });
    await opened("a");
    await opened("b");
    const before = { ...hoisted.drawn };

    await act(async () => { hoisted.write?.("b", "a line"); });

    expect(hoisted.drawn.b).toBeGreaterThan(before.b);
  });

  it("calls the newest handler the face handed, though the pane was not drawn for it", async () => {
    await act(async () => { root.render(createElement(Window)); });
    await opened("a");
    await opened("b");
    // Pane b is not drawn for these, though the window hands it a newer handler on each of them.
    await act(async () => { hoisted.write?.("a", "one"); });
    await act(async () => { hoisted.write?.("a", "two"); });

    const newest = renders;

    await type("b", "from b");

    expect(hoisted.calls[hoisted.calls.length - 1]).toEqual({ frame: "b", text: "from b", render: newest });
  });
});

describe("samePane", () => {
  const props = (over: Record<string, unknown> = {}) => ({
    frame: "a",
    hue: 199,
    project: 3,
    names,
    start: { frame: "a", cwd: "/work/a" },
    autoStart: true,
    focused: false,
    written: "",
    onWrite: () => {},
    composeOpen: false,
    onFold: () => {},
    onOpened: () => {},
    onSaid: () => {},
    onPath: () => {},
    onClosed: () => {},
    onDrop: () => {},
    onName: () => {},
    onFocus: () => {},
    ...over,
  }) as unknown as Parameters<typeof samePane>[0];

  it("takes new handlers and new objects holding the same values as the same pane", () => {
    expect(samePane(props(), props())).toBe(true);
  });

  it("tells a changed draft, a changed start and a changed run apart", () => {
    expect(samePane(props(), props({ written: "x" }))).toBe(false);
    expect(samePane(props(), props({ start: { frame: "a", cwd: "/work/b" } }))).toBe(false);
    expect(samePane(props({ run: { run: 1, task: { ref: "T-1" } } }), props({ run: { run: 1, task: { ref: "T-2" } } })))
      .toBe(false);
  });

  it("tells a handler there from one left out", () => {
    expect(samePane(props(), props({ onGrab: () => {} }))).toBe(false);
  });

  it("compares a map as it is", () => {
    expect(samePane(props(), props({ names: new Map() }))).toBe(false);
  });
});
