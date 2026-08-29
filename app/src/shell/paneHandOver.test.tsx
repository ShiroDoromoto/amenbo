// @vitest-environment jsdom
// Handing a pane a file by dropping it on one.
//
// The pane is a real terminal and the agent in it reads the folder it was opened in, so a file from
// the desktop has to be *in* that folder before it can be read at all — which is why the drop carries
// it into the project's own inbox and hands back where it landed (`AMB-D-800`). What is pinned here
// is the shape of that: the surface is only offered where there is a terminal to hand something to,
// a drop on the pane beside this one is not this pane's, and what comes back is put in front of the
// reader rather than run for them.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { HostDropWatch } from "../core/hostDrop";
import type { PaneEvents } from "../talk/terminal";
import { t } from "../core/i18n";
import { TerminalPane } from "./TerminalPane";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const hoisted = vi.hoisted(() => ({
  /** What the frame was handed, so the test can play the host and open a session in the pane. */
  events: null as PaneEvents | null,
  /** The drop watch this pane took up, while it holds one. */
  watch: null as HostDropWatch | null,
  /** What the carry into the inbox was asked for, in the order it was asked. */
  carried: [] as Array<{ project: number; root: string; paths: string[] }>,
  /** What that carry answers with. */
  inboxed: { arrived: [] as string[], stopped: null as { code: string; name: string; why: string } | null },
  /** Whether the host refuses it outright. */
  carryFails: false,
  /** What was put in front of the reader, as the pane asked for it. */
  pasted: [] as Array<{ session: string; text: string }>,
  /** The lines the pane put on the screen. */
  noticed: [] as string[],
  /** What the machine's own picker answers with. */
  picked: [] as string[],
}));

vi.mock("../talk/agent", () => ({
  mountAgentFrame: (_host: HTMLElement, _lang: string, on: PaneEvents) => {
    hoisted.events = on;
    return Promise.resolve(() => {});
  },
}));
vi.mock("../talk/terminal", () => ({
  endTerminal: vi.fn(async () => {}),
  pasteIntoTerminal: vi.fn(async (session: string, text: string) => {
    hoisted.pasted.push({ session, text });
  }),
}));
vi.mock("../core/hostDrop", () => ({
  watchHostDrop: vi.fn(async (watch: HostDropWatch) => {
    hoisted.watch = watch;
    return () => { hoisted.watch = null; };
  }),
}));
vi.mock("../files/folder", () => ({
  folderInbox: vi.fn(async (project: number, root: string, paths: string[]) => {
    hoisted.carried.push({ project, root, paths });
    if (hoisted.carryFails) throw new Error("that folder is not there any more");
    return hoisted.inboxed;
  }),
}));
vi.mock("../core/dialog", () => ({
  confirmDialog: vi.fn(async () => true),
  pickFiles: vi.fn(async () => hoisted.picked),
}));
vi.mock("../core/notice", () => ({
  pushNotice: vi.fn((msg: string) => { hoisted.noticed.push(msg); }),
}));
vi.mock("../core/ipc", () => ({ invoke: vi.fn(async () => ({ holding: [], finished: 0 })) }));
// The label above the pane is a live thing of its own; what it draws is not what this is about.
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
  hoisted.watch = null;
  hoisted.carried = [];
  hoisted.inboxed = { arrived: [], stopped: null };
  hoisted.carryFails = false;
  hoisted.pasted = [];
  hoisted.noticed = [];
  hoisted.picked = [];
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

/** A pane on frame 1, working in `/work/here`. */
async function pane(autoStart = true): Promise<void> {
  await act(async () => {
    root.render(createElement(TerminalPane, {
      frame: "1",
      project: 3,
      names: new Map(),
      start: { cwd: "/work/here" },
      autoStart,
      focused: true,
      onOpened: () => {},
      onSaid: () => {},
      onPath: () => {},
      onClosed: () => {},
      onDrop: () => {},
      onName: () => {},
      onFocus: () => {},
      onWaiting: () => {},
    }));
  });
}

/** The host opens a terminal in the pane, which is what puts the watch up. */
async function opened(folder: string | null = "/work/here"): Promise<void> {
  await act(async () => {
    hoisted.events?.opened("session-7", "2026-08-29T00:00:00Z", folder);
  });
  await act(async () => { await Promise.resolve(); });
}

/** The slot, which is what a drop is matched against. */
const slot = () => container.querySelector<HTMLElement>(".slot");
/** The receiving surface, while it is on the screen. */
const surface = () => container.querySelector(".slot__handing");
/** Play the host: the drag is over this pane, or over nothing of this pane's. */
const over = async (on: Element | null) => {
  await act(async () => { hoisted.watch?.over?.({ x: 0, y: 0, el: on }); });
};
/** Play the host: files land on the pane, and everything they set off settles. */
const drop = async (paths: string[]) => {
  await act(async () => {
    hoisted.watch?.drop?.({ x: 0, y: 0, el: slot()! }, paths, "default");
  });
  await act(async () => { await Promise.resolve(); });
};

/** The way into the row's menu, while the row carries one. */
const more = () => container.querySelector<HTMLButtonElement>(".slot__more");
/** Open it, and press the item at `nth`. */
const chooseInMenu = async (nth: number) => {
  await act(async () => { more()?.click(); });
  const items = document.querySelectorAll<HTMLButtonElement>(".menu__item");
  await act(async () => { items[nth]?.click(); });
  await act(async () => { await Promise.resolve(); });
};

describe("handing a pane a file", () => {
  it("is not offered where there is no terminal to hand it to", async () => {
    await pane(false);
    expect(hoisted.watch, "a place with nothing running in it watched for drops").toBeNull();
  });

  it("watches for this pane's own drops once a terminal is running", async () => {
    await pane();
    await opened();

    expect(hoisted.watch?.select, "a drop on the pane beside this one would be taken as this one's")
      .toBe('[data-hand="1"]');
    expect(slot()?.dataset.hand, "the slot is not what a drop is matched against").toBe("1");
  });

  it("draws the surface while the drag hangs on it, and takes it away as it leaves", async () => {
    await pane();
    await opened();

    await over(slot());
    expect(surface()?.textContent).toBe(t("face.handHere"));

    await over(null);
    expect(surface(), "the surface stayed after the drag had gone").toBeNull();
  });

  it("carries what landed into the folder the terminal runs in, and pastes where it went", async () => {
    await pane();
    await opened();
    hoisted.inboxed = { arrived: ["/work/here/.amenbo-inbox/2026-08-29/shot.png"], stopped: null };

    await over(slot());
    await drop(["/Users/somebody/Desktop/shot.png"]);

    expect(hoisted.carried).toEqual([
      { project: 3, root: "/work/here", paths: ["/Users/somebody/Desktop/shot.png"] },
    ]);
    expect(hoisted.pasted).toEqual([
      { session: "session-7", text: "/work/here/.amenbo-inbox/2026-08-29/shot.png" },
    ]);
    expect(surface(), "the surface stayed after the drop").toBeNull();
  });

  it("takes the folder off the session, not off the slot it was handed", async () => {
    await pane();
    // A pane that took up a terminal draws one that was opened somewhere else (`AMB-D-753`).
    await opened("/work/elsewhere");
    hoisted.inboxed = { arrived: ["/work/elsewhere/.amenbo-inbox/2026-08-29/shot.png"], stopped: null };

    await drop(["/Users/somebody/Desktop/shot.png"]);

    expect(hoisted.carried[0]?.root).toBe("/work/elsewhere");
  });

  it("says what stopped a carry, and still pastes what got there", async () => {
    await pane();
    await opened();
    hoisted.inboxed = {
      arrived: ["/work/here/.amenbo-inbox/2026-08-29/one.png"],
      stopped: { code: "nameless", name: "two.png", why: "that has no name" },
    };

    await drop(["/Users/somebody/Desktop/one.png", "/Users/somebody/Desktop/two.png"]);

    expect(hoisted.pasted).toHaveLength(1);
    expect(hoisted.noticed, "a carry that stopped part-way said nothing").toHaveLength(1);
    expect(hoisted.noticed[0]).toContain("two.png");
  });

  it("keeps the row's menu off a place with nothing running in it", async () => {
    await pane(false);
    expect(more(), "a place with nothing running in it offered to be handed a file").toBeNull();
  });

  it("puts the row's menu up once a terminal is running", async () => {
    await pane();
    await opened();
    expect(more(), "a running pane had no way to be handed anything but a drop").not.toBeNull();
  });

  it("carries what was chosen from the machine's picker down the same road", async () => {
    await pane();
    await opened();
    hoisted.picked = ["/Users/somebody/Documents/notes.md"];
    hoisted.inboxed = { arrived: ["/work/here/.amenbo-inbox/2026-08-29/notes.md"], stopped: null };

    await chooseInMenu(0);

    expect(hoisted.carried).toEqual([
      { project: 3, root: "/work/here", paths: ["/Users/somebody/Documents/notes.md"] },
    ]);
    expect(hoisted.pasted).toEqual([
      { session: "session-7", text: "/work/here/.amenbo-inbox/2026-08-29/notes.md" },
    ]);
  });

  it("does nothing where the picker was cancelled", async () => {
    await pane();
    await opened();

    await chooseInMenu(0);

    expect(hoisted.carried, "an empty pick was carried anyway").toEqual([]);
    expect(hoisted.pasted).toEqual([]);
  });

  it("says the refusal rather than swallowing it, and pastes nothing", async () => {
    await pane();
    await opened();
    hoisted.carryFails = true;

    await drop(["/Users/somebody/Desktop/shot.png"]);

    expect(hoisted.pasted, "a path was pasted for a carry that never happened").toEqual([]);
    expect(hoisted.noticed).toHaveLength(1);
  });
});
