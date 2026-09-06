// @vitest-environment jsdom
// Handing a pane a file by dropping it on one.
//
// Nothing is moved and nothing is copied: what goes into the terminal is where the file or the
// folder is now, exactly as the drop handed it over (`AMB-D-820`). What is pinned here is the shape
// of that: the surface is only offered where there is a terminal to hand something to, a drop on the
// pane beside this one is not this pane's, and the path is put in front of the reader rather than
// run for them — and that a drop is a person saying which pane they mean, so the selection and the
// keyboard both follow the path into it (`AMB-T-4182`).
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
  /** What was put in front of the reader, as the pane asked for it. */
  pasted: [] as Array<{ session: string; text: string }>,
  /** Whether the terminal refuses what is pasted into it. */
  pasteFails: false,
  /** The lines the pane put on the screen. */
  noticed: [] as string[],
  /** What the machine's own file picker answers with. */
  picked: [] as string[],
  /** And its folder picker, which is a second window rather than the same one. */
  pickedFolders: [] as string[],
  /** The frames the pane said were being worked in, in the order it said so. */
  focused: [] as string[],
}));

vi.mock("../talk/agent", () => ({
  mountAgentFrame: (host: HTMLElement, _lang: string, on: PaneEvents) => {
    hoisted.events = on;
    // The box the emulator collects typing in, which is what the keyboard lands on. It is the one
    // thing of the real terminal this stub keeps, because where the focus goes is what is being
    // pinned here (`../talk/terminal`).
    host.append(document.createElement("textarea"));
    return Promise.resolve(() => {});
  },
}));
// The terminal is stubbed except for the one thing this is about: how a path is written down
// (`../talk/terminal`). Quoting it is what the pane hands over, so a stub of that would leave the
// road's last step tested nowhere.
vi.mock("../talk/terminal", async (actual) => ({
  ...(await actual<typeof import("../talk/terminal")>()),
  endTerminal: vi.fn(async () => {}),
  pasteIntoTerminal: vi.fn(async (session: string, text: string) => {
    if (hoisted.pasteFails) throw new Error("that terminal is not there any more");
    hoisted.pasted.push({ session, text });
  }),
}));
vi.mock("../core/hostDrop", () => ({
  watchHostDrop: vi.fn(async (watch: HostDropWatch) => {
    hoisted.watch = watch;
    return () => { hoisted.watch = null; };
  }),
}));
vi.mock("../core/dialog", () => ({
  confirmDialog: vi.fn(async () => true),
  pickFiles: vi.fn(async () => hoisted.picked),
  pickFolders: vi.fn(async () => hoisted.pickedFolders),
}));
vi.mock("../core/notice", () => ({
  pushNotice: vi.fn((msg: string) => { hoisted.noticed.push(msg); }),
}));
vi.mock("../core/ipc", () => ({ invoke: vi.fn(async () => undefined) }));
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
  hoisted.pasted = [];
  hoisted.pasteFails = false;
  hoisted.noticed = [];
  hoisted.picked = [];
  hoisted.pickedFolders = [];
  hoisted.focused = [];
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
      onFocus: (id: string) => { hoisted.focused.push(id); },
      onWaiting: () => {},
    }));
  });
}

/** The host opens a terminal in the pane, which is what puts the watch up. */
async function opened(): Promise<void> {
  await act(async () => {
    hoisted.events?.opened("session-7", "2026-08-29T00:00:00Z", "/work/here");
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

/** The box the keyboard lands on, as the terminal in this pane draws one. */
const typing = () => container.querySelector<HTMLTextAreaElement>(".termface__face textarea");

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

  it("pastes where what landed already is, leaving it where it is", async () => {
    await pane();
    await opened();

    await over(slot());
    await drop(["/Users/somebody/Desktop/shot.png"]);

    expect(hoisted.pasted).toEqual([
      { session: "session-7", text: "'/Users/somebody/Desktop/shot.png'" },
    ]);
    expect(surface(), "the surface stayed after the drop").toBeNull();
  });

  it("pastes a folder's path the same way a file's, without copying the tree under it", async () => {
    await pane();
    await opened();

    await drop(["/Users/somebody/Projects/notes"]);

    expect(hoisted.pasted).toEqual([
      { session: "session-7", text: "'/Users/somebody/Projects/notes'" },
    ]);
  });

  it("quotes each path on its own, so the space between two is not the space inside one", async () => {
    await pane();
    await opened();

    await drop(["/Users/somebody/Desktop/a shot.png", "/Users/somebody/Desktop/it's shot.png"]);

    expect(hoisted.pasted[0]?.text).toBe(
      "'/Users/somebody/Desktop/a shot.png' "
      + "'/Users/somebody/Desktop/it'\\''s shot.png'",
    );
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

  it("pastes what was chosen from the machine's picker down the same road", async () => {
    await pane();
    await opened();
    hoisted.picked = ["/Users/somebody/Documents/notes.md"];

    await chooseInMenu(0);

    expect(hoisted.pasted).toEqual([
      { session: "session-7", text: "'/Users/somebody/Documents/notes.md'" },
    ]);
  });

  /** Two items and not one, because the machine's picker takes `directory` as a yes or a no: one
   *  window cannot offer files and folders together (`../core/dialog`). */
  it("opens a second window for a folder, and pastes that path the same way", async () => {
    await pane();
    await opened();
    hoisted.pickedFolders = ["/Users/somebody/Projects/notes"];

    await chooseInMenu(1);

    expect(hoisted.pasted).toEqual([
      { session: "session-7", text: "'/Users/somebody/Projects/notes'" },
    ]);
  });

  it("does nothing where the picker was cancelled", async () => {
    await pane();
    await opened();

    await chooseInMenu(0);

    expect(hoisted.pasted, "an empty pick was pasted anyway").toEqual([]);
  });

  it("takes the pane the drop landed on as the one being worked in", async () => {
    await pane();
    await opened();

    await drop(["/Users/somebody/Desktop/shot.png"]);

    expect(hoisted.focused, "the path went into a pane the face still called unselected")
      .toEqual(["1"]);
  });

  it("moves the keyboard into it too, so the next keystroke goes where the path did", async () => {
    await pane();
    await opened();
    // Somewhere else on the page holds the keyboard, which is what a drop from the desktop leaves
    // untouched: the gesture never presses the page at all.
    const elsewhere = document.createElement("input");
    document.body.append(elsewhere);
    elsewhere.focus();

    await drop(["/Users/somebody/Desktop/shot.png"]);

    expect(document.activeElement, "the reader would have typed into the pane they came from")
      .toBe(typing());
    elsewhere.remove();
  });

  it("says the refusal rather than swallowing it", async () => {
    await pane();
    await opened();
    hoisted.pasteFails = true;

    await drop(["/Users/somebody/Desktop/shot.png"]);

    expect(hoisted.pasted, "a path landed for a paste that never happened").toEqual([]);
    expect(hoisted.noticed).toHaveLength(1);
  });
});
