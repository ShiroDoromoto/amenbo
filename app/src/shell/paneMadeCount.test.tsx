// @vitest-environment jsdom
// What the pane does with a note a create left it (`AMB-D-897`): the band under the terminal counts
// the records this session has filed, and the count is the session's rather than the place's.
//
// The band itself is `./paneMade.test`. What is held here is the wiring — that a note arriving from
// the drop box reaches the band at all, that one record arriving twice is still one record, and that
// a pane opened again starts the count over. The last is the one a reader would be misled by: a
// number left standing from the session before says this session filed work it never touched.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SessionSaidDto } from "../bindings/bindings";
import type { PaneEvents } from "../talk/terminal";
import { TerminalPane } from "./TerminalPane";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const hoisted = vi.hoisted(() => ({
  /** What the frame was handed, so the test can play the host that watches the drop box. */
  events: null as PaneEvents | null,
}));

vi.mock("../talk/agent", () => ({
  mountAgentFrame: (host: HTMLElement, _lang: string, on: PaneEvents) => {
    hoisted.events = on;
    host.append(document.createElement("textarea"));
    return Promise.resolve(() => {});
  },
}));
vi.mock("../talk/terminal", async (actual) => ({
  ...(await actual<typeof import("../talk/terminal")>()),
  endTerminal: vi.fn(async () => {}),
  showRef: vi.fn(),
}));
vi.mock("../core/hostDrop", () => ({ watchHostDrop: vi.fn(async () => () => {}) }));
vi.mock("../core/dialog", () => ({
  confirmDialog: vi.fn(async () => true),
  pickFiles: vi.fn(async () => []),
  pickFolders: vi.fn(async () => []),
}));
vi.mock("../core/notice", () => ({ pushNotice: vi.fn() }));
vi.mock("../core/ipc", () => ({ invoke: vi.fn(async () => {}) }));
vi.mock("../talk/plate", () => ({
  mountPlate: () => ({
    opened: () => {}, closed: () => {}, named: () => {}, took: () => {}, stated: () => {},
    focused: () => {}, stop: () => {},
  }),
}));

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  hoisted.events = null;
  localStorage.clear();
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

/** A pane on frame 1, working in `/work/here`. */
async function pane(): Promise<void> {
  await act(async () => {
    root.render(createElement(TerminalPane, {
      frame: "1",
      hue: 199,
      project: 3,
      names: new Map(),
      start: { cwd: "/work/here" },
      autoStart: true,
      focused: true,
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
    }));
  });
}

/** The host opens a terminal in the pane. */
async function opened(session = "session-7"): Promise<void> {
  await act(async () => { hoisted.events?.opened(session, "/work/here", null); });
  await act(async () => { await Promise.resolve(); });
}

/** A note the create left in the drop box, on its way through the host to the pane. */
async function filed(kind: "task" | "decision", id: number, session = "session-7"): Promise<void> {
  const statement: SessionSaidDto = {
    session, verb: "made", at: "2026-09-15T10:00:00Z", made: { kind, id },
  };
  await act(async () => { hoisted.events?.said(statement); });
  await act(async () => { await Promise.resolve(); });
}

/** The count the band is showing, or nothing where the band is not drawn at all. */
function count(): string | null {
  return container.querySelector(".maderow__count")?.textContent ?? null;
}

describe("the count under the pane is this session's own", () => {
  it("counts what the session filed, and says nothing until it has filed something", async () => {
    await pane();
    await opened();
    expect(count(), "a session that has filed nothing has no count to show").toBe(null);

    await filed("task", 4849);
    await filed("decision", 897);
    expect(count()).toContain("1 task");
    expect(count()).toContain("1 decision");
  });

  it("counts one record once, however many times the note arrives", async () => {
    await pane();
    await opened();
    await filed("task", 4849);
    await filed("task", 4849);
    expect(count()).toContain("1 task");
  });

  it("starts over when the pane is opened again, the count belonging to the session", async () => {
    await pane();
    await opened();
    await filed("task", 4849);
    expect(count()).toContain("1 task");

    await opened("session-8");
    expect(count(), "what the last session filed is not this one's").toBe(null);
  });
});
