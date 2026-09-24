// @vitest-environment jsdom
// What a pane is handed as it takes a terminal up: the name that session gave itself, and every
// record filed from it (`../talk/terminal`).
//
// A statement is heard only where a pane is drawing that session, so one on another page or in the
// other window hears none of it — the name and the count were gone the moment they were said
// (`AMB-T-5196`). The terminal keeps both now and hands them over on the attach, and what is pinned
// here is that they land where the live ones do: the name on the frame, the records on the band.
//
// **And that the provider is not told the name.** It is the one that said it, so a `/rename` typed
// back into it is a line the agent never asked for (`AMB-T-5118`, `AMB-T-5074`).
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SessionMadeDto, SessionSaidDto } from "../bindings/bindings";
import type { NamedBy } from "../talk/frames";
import type { PaneEvents } from "../talk/terminal";
import { TerminalPane } from "./TerminalPane";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const hoisted = vi.hoisted(() => ({
  /** What the frame was handed, so the test can play the host. */
  events: null as PaneEvents | null,
  /** The namings the pane asked for, in the order it asked. */
  named: [] as Array<{ name: string; by: NamedBy; carried?: boolean }>,
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
  pasteIntoTerminal: vi.fn(async () => {}),
  showRef: vi.fn(),
}));
vi.mock("../core/hostDrop", () => ({ watchHostDrop: vi.fn(async () => () => {}) }));
vi.mock("../core/dialog", () => ({
  confirmDialog: vi.fn(async () => true),
  pickFiles: vi.fn(async () => []),
  pickFolders: vi.fn(async () => []),
}));
vi.mock("../core/notice", () => ({ pushNotice: vi.fn() }));
vi.mock("../core/ipc", () => ({ invoke: vi.fn(async () => undefined) }));
vi.mock("../talk/plate", () => ({
  mountPlate: () => ({
    opened: () => {}, closed: () => {}, named: () => {}, took: () => {},
    focused: () => {}, stop: () => {},
  }),
}));

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  hoisted.events = null;
  hoisted.named = [];
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

/** A pane on frame 1, with a terminal opened in it — the shape everything below starts from. */
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
      onOpened: () => {},
      onSaid: () => {},
      onPath: () => {},
      onClosed: () => {},
      onDrop: () => {},
      onName: (_frame: string, name: string, by: NamedBy, carried?: boolean) => {
        hoisted.named.push({ name, by, carried });
      },
      written: "",
      onWrite: () => {},
      composeOpen: false,
      onFold: () => {},
      onFocus: () => {},
    }));
  });
  await act(async () => { hoisted.events?.opened("session-7", "/work/here", null); });
  await act(async () => { await Promise.resolve(); });
}

/** The host hands over what the session said while nothing was drawing it. */
async function handOver(name: string | null, made: SessionMadeDto[]): Promise<void> {
  await act(async () => { hoisted.events?.carried?.(name, made); });
  await act(async () => { await Promise.resolve(); });
}

/** One record going past live, the way it does while the pane is on the screen. */
async function filed(made: SessionMadeDto): Promise<void> {
  const statement: SessionSaidDto = {
    session: "session-7", verb: "made", at: "2026-09-20T00:00:00Z", made,
  };
  await act(async () => { hoisted.events?.said(statement); });
  await act(async () => { await Promise.resolve(); });
}

/** The way into the band's list — the count itself, or nothing where the band draws none. */
const count = () => container.querySelector<HTMLButtonElement>(".maderow__count");

/** The refs on the band's list, once it is open. */
async function listed(): Promise<string[]> {
  await act(async () => { count()?.click(); });
  return [...document.querySelectorAll<HTMLElement>(".menu__item")].map((one) => one.textContent ?? "");
}

describe("the name a session gave itself while nothing was drawing it", () => {
  it("goes on the frame, marked as carried so the provider is left alone", async () => {
    await pane();

    await handOver("the migration", []);

    expect(hoisted.named).toEqual([{ name: "the migration", by: "session", carried: true }]);
  });

  it("names nothing where the session never named itself", async () => {
    await pane();

    await handOver(null, [{ kind: "task", id: 4849 }]);

    expect(hoisted.named).toEqual([]);
  });

  it("is a naming of its own, and the next `talk name` is still told to the provider", async () => {
    await pane();
    await handOver("the migration", []);

    await act(async () => { hoisted.events?.name("what it is on now", "session"); });

    expect(hoisted.named).toEqual([
      { name: "the migration", by: "session", carried: true },
      { name: "what it is on now", by: "session", carried: undefined },
    ]);
  });
});

describe("the records filed while nothing was drawing the pane", () => {
  it("are on the band once the pane takes the terminal up", async () => {
    await pane();

    await handOver(null, [{ kind: "task", id: 4849 }, { kind: "decision", id: 897 }]);

    expect(count()).not.toBe(null);
    expect(await listed()).toHaveLength(2);
  });

  it("are one entry each, however many ways the same record arrives", async () => {
    await pane();
    await filed({ kind: "task", id: 4849 });

    // The same record again: it went past live, and the hand-over carries everything the session
    // filed rather than everything it filed unheard.
    await handOver(null, [{ kind: "task", id: 4849 }, { kind: "task", id: 4850 }]);

    expect(await listed()).toHaveLength(2);
  });

  it("go when the terminal goes, because the next one filed none of them", async () => {
    await pane();
    await handOver(null, [{ kind: "task", id: 4849 }]);

    await act(async () => { hoisted.events?.opened("session-8", "/work/here", null); });
    await act(async () => { await Promise.resolve(); });

    expect(count()).toBe(null);
  });
});
