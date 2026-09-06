// @vitest-environment jsdom
// Naming a pane from the row above it.
//
// A name belongs to the frame and a person's word is the last one on it (`../talk/frames`), so this
// pins where that word is said: the row's own menu, and a box standing in the line's place rather
// than a window over the pane. It is the only way in once the rail's list of panes goes
// (`AMB-D-838`), which is also why an empty box may not take a name off a pane the agent named.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { NamedBy } from "../talk/frames";
import type { PaneEvents } from "../talk/terminal";
import { TerminalPane } from "./TerminalPane";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const hoisted = vi.hoisted(() => ({
  /** What the frame was handed, so the test can play the host and open a session in the pane. */
  events: null as PaneEvents | null,
  /** The namings the pane asked for, in the order it asked. */
  named: [] as Array<{ frame: string; name: string; by: NamedBy }>,
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
}));
vi.mock("../core/hostDrop", () => ({ watchHostDrop: vi.fn(async () => () => {}) }));
vi.mock("../core/dialog", () => ({
  confirmDialog: vi.fn(async () => true),
  pickFiles: vi.fn(async () => []),
  pickFolders: vi.fn(async () => []),
}));
vi.mock("../core/notice", () => ({ pushNotice: vi.fn() }));
vi.mock("../core/ipc", () => ({ invoke: vi.fn(async () => undefined) }));
// The label above the pane is a live thing of its own; what it draws is not what this is about —
// only that it is still there while the box stands in its place.
vi.mock("../talk/plate", () => ({
  mountPlate: () => ({
    opened: () => {}, said: () => {}, closed: () => {}, named: () => {},
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

/** A pane on frame 1, called whatever `names` says. */
async function pane(names: Map<string, string> = new Map()): Promise<void> {
  await act(async () => {
    root.render(createElement(TerminalPane, {
      frame: "1",
      project: 3,
      names,
      start: { cwd: "/work/here" },
      autoStart: true,
      focused: true,
      onOpened: () => {},
      onSaid: () => {},
      onPath: () => {},
      onClosed: () => {},
      onDrop: () => {},
      onName: (frame: string, name: string, by: NamedBy) => {
        hoisted.named.push({ frame, name, by });
      },
      onFocus: () => {},
      onWaiting: () => {},
    }));
  });
}

/** The host opens a terminal in the pane, which is what puts the row's menu up. */
async function opened(): Promise<void> {
  await act(async () => {
    hoisted.events?.opened("session-7", "2026-09-03T00:00:00Z", "/work/here");
  });
  await act(async () => { await Promise.resolve(); });
}

/** The way into the row's menu, while the row carries one. */
const more = () => container.querySelector<HTMLButtonElement>(".slot__more");
/** The box the name is typed in, while it is up. */
const field = () => container.querySelector<HTMLInputElement>(".slot__rename");
/** The line the box stands in for. */
const plate = () => container.querySelector<HTMLElement>(".slot__plate");

/** Open the menu and press the item that names the pane — the last one. */
const chooseRename = async () => {
  await act(async () => { more()?.click(); });
  const items = [...document.querySelectorAll<HTMLButtonElement>(".menu__item")];
  await act(async () => { items[items.length - 1]?.click(); });
  await act(async () => { await Promise.resolve(); });
};

/** Type into the box and press a key on it. */
const press = async (key: string, text?: string) => {
  const box = field();
  if (box && text !== undefined) box.value = text;
  await act(async () => {
    box?.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
  });
};

describe("naming a pane", () => {
  it("keeps the way in off a place with nothing running in it", async () => {
    await act(async () => {
      root.render(createElement(TerminalPane, {
        frame: "1",
        project: 3,
        names: new Map(),
        start: { cwd: "/work/here" },
        autoStart: false,
        focused: true,
        onOpened: () => {}, onSaid: () => {}, onPath: () => {}, onClosed: () => {},
        onDrop: () => {}, onName: () => {}, onFocus: () => {}, onWaiting: () => {},
      }));
    });

    expect(more(), "an empty frame offered a name it has nowhere to keep").toBeNull();
  });

  it("puts the box in the line's place, with the name as it stands ready to type over", async () => {
    await pane(new Map([["1", "the migration"]]));
    await opened();

    await chooseRename();

    expect(field()?.value, "the reader would have typed the old name back in first")
      .toBe("the migration");
    expect(plate()?.className, "the line was taken down rather than put behind the box")
      .toContain("slot__plate--behind");
    expect(plate(), "the row that draws the label was taken off the page").not.toBeNull();
  });

  it("takes the word on Enter, as the person's", async () => {
    await pane();
    await opened();
    await chooseRename();

    await press("Enter", "  the migration  ");

    expect(hoisted.named).toEqual([{ frame: "1", name: "the migration", by: "person" }]);
    expect(field(), "the box stayed up after the name was taken").toBeNull();
  });

  it("leaves the name alone on Escape", async () => {
    await pane(new Map([["1", "the migration"]]));
    await opened();
    await chooseRename();

    await press("Escape", "something else");

    expect(hoisted.named, "a name nobody asked for was taken").toEqual([]);
    expect(field(), "the box stayed up after the reader backed out").toBeNull();
  });

  it("takes no name from an empty box, so an agent's name is not rubbed out by one", async () => {
    await pane(new Map([["1", "the migration"]]));
    await opened();
    await chooseRename();

    await press("Enter", "   ");

    expect(hoisted.named).toEqual([]);
    expect(field(), "the box stayed up after the reader sent nothing").toBeNull();
  });

  it("closes the box where the reader goes somewhere else", async () => {
    await pane();
    await opened();
    await chooseRename();

    // `focusout` and not `blur`: React listens for the one that bubbles, and a `blur` dispatched
    // here reaches no handler at all.
    await act(async () => {
      field()?.dispatchEvent(new FocusEvent("focusout", { bubbles: true }));
    });

    expect(hoisted.named, "leaving the box named the pane").toEqual([]);
    expect(field(), "the box outlived the reader leaving it").toBeNull();
  });
});
