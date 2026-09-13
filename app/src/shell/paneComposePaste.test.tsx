// @vitest-environment jsdom
// A file or a picture pasted into the box under a pane (`AMB-D-854`, `AMB-D-864`).
//
// What is pinned here is that the path goes **into the box, quoted** — not into the terminal. The
// box's line is sent to the program as the person's own (`AMB-D-864`), so there is a shell behind it
// and an unquoted name with a space in it would be two words. And that it goes in **at the caret**:
// what is written in the box belongs to the window (`../talk/layout`), so the sentence comes back
// down as a new value and the caret has to be put back where the paste left it.
import { act, createElement, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PaneEvents } from "../talk/terminal";
import { TerminalPane } from "./TerminalPane";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const hoisted = vi.hoisted(() => ({
  /** What the frame was handed, so the test can play the host and open a session in the pane. */
  events: null as PaneEvents | null,
  /** What the host says the clipboard's files are at, which a page cannot read for itself. */
  paths: [] as string[],
  /** Where the host wrote a pasted image down, or none where it would not write one. */
  wrote: "/tmp/amenbo-pasted-7a/pasted-0a0b0c0d.png" as string | null,
  /** What crossed to the host, in the order it crossed. */
  asked: [] as Array<{ cmd: string; args: Record<string, unknown> }>,
  /** What the window is holding for this pane, as it last drew. */
  held: "",
  /** The paths the pane said Amenbo had put into the box, on the last write (`../talk/layout`). */
  put: [] as readonly string[],
}));

vi.mock("../talk/agent", () => ({
  mountAgentFrame: (host: HTMLElement, _lang: string, on: PaneEvents) => {
    hoisted.events = on;
    host.append(document.createElement("textarea"));
    return Promise.resolve(() => {});
  },
}));
// The terminal's own road is left real: how a path is written is the whole of what this is about.
vi.mock("../talk/terminal", async (actual) => ({
  ...(await actual<typeof import("../talk/terminal")>()),
  endTerminal: vi.fn(async () => {}),
}));
vi.mock("../core/hostDrop", () => ({ watchHostDrop: vi.fn(async () => () => {}) }));
vi.mock("../core/dialog", () => ({
  confirmDialog: vi.fn(async () => true),
  pickFiles: vi.fn(async () => []),
  pickFolders: vi.fn(async () => []),
}));
vi.mock("../core/notice", () => ({ pushNotice: vi.fn() }));
vi.mock("../core/ipc", () => ({
  invoke: vi.fn(async (cmd: string, args: Record<string, unknown>) => {
    hoisted.asked.push({ cmd, args });
    if (cmd !== "pty_paste_image") return hoisted.paths;
    if (hoisted.wrote === null) throw new Error("the image could not be written down");
    return hoisted.wrote;
  }),
}));
vi.mock("../talk/plate", () => ({
  mountPlate: () => ({
    opened: () => {}, closed: () => {}, named: () => {},
    focused: () => {}, stop: () => {},
  }),
}));

let container: HTMLDivElement;
let root: Root;
/** What `navigator.clipboard.read()` answers, which is the Linux door (`../core/clipFiles`). */
let read: ReturnType<typeof vi.fn>;

beforeEach(() => {
  hoisted.events = null;
  hoisted.paths = [];
  hoisted.wrote = "/tmp/amenbo-pasted-7a/pasted-0a0b0c0d.png";
  hoisted.asked = [];
  hoisted.held = "";
  read = vi.fn(async () => [
    {
      types: ["image/png"],
      getType: async () => new Blob(["the image"], { type: "image/png" }),
    },
  ]);
  Object.defineProperty(navigator, "clipboard", { value: { read }, configurable: true });
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

/** The window the pane is drawn in, which is what holds the line being written (`../talk/layout`). */
function Window() {
  const [written, setWritten] = useState("");
  hoisted.held = written;
  return createElement(TerminalPane, {
    frame: "1",
    project: 3,
    names: new Map(),
    start: { cwd: "/work/here" },
    autoStart: true,
    focused: true,
    written,
    onWrite: (_frame: string, text: string, put: readonly string[] = []) => {
      hoisted.put = put;
      setWritten(text);
    },
    onOpened: () => {},
    onSaid: () => {},
    onPath: () => {},
    onClosed: () => {},
    onDrop: () => {},
    onName: () => {},
    onFocus: () => {},
  });
}

/** A pane with a terminal running in it, which is what puts the box up. */
async function pane(): Promise<void> {
  await act(async () => { root.render(createElement(Window)); });
  await act(async () => { hoisted.events?.opened("session-7", "/work/here", null); });
  await act(async () => { await Promise.resolve(); });
}

const box = () => container.querySelector<HTMLTextAreaElement>(".compose__box")!;

/** Write `text` in the box, the way a person does. */
async function write(text: string): Promise<void> {
  const field = box();
  await act(async () => { field.focus(); });
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")!.set!.call(field, text);
    field.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

/** A paste carrying files, landing on the box the way a real one does. */
async function pasteFiles(over: { words?: string; carried?: File[] } = {}): Promise<void> {
  await act(async () => {
    const e = new Event("paste", { bubbles: true, cancelable: true });
    Object.defineProperty(e, "clipboardData", {
      value: {
        files: over.carried ?? [new File(["a note"], "a note.md", { type: "text/markdown" })],
        types: ["Files"],
        getData: () => over.words ?? "",
      },
    });
    box().dispatchEvent(e);
    await new Promise((r) => setTimeout(r, 0));
  });
}

/** A paste carrying a screenshot: an image the engine made, with no file on disk behind it. */
async function pasteImage(): Promise<void> {
  await pasteFiles({ carried: [new File(["the image"], "pasted", { type: "image/png" })] });
}

/** The press that asks the clipboard on Linux, where a paste carries nothing (`AMB-D-854`). */
async function press(over: Partial<KeyboardEventInit> = {}): Promise<void> {
  await act(async () => {
    box().dispatchEvent(
      new KeyboardEvent("keydown", { key: "v", ctrlKey: true, bubbles: true, cancelable: true, ...over }),
    );
    await new Promise((r) => setTimeout(r, 0));
  });
}

/** What was written to the terminal, in the order it went. */
const wrote = () => hoisted.asked.filter((one) => one.cmd === "pty_write").map((one) => one.args.data);

describe("pasting a file into the box under a pane", () => {
  it("puts the paths in the box, quoted, and sends nothing", async () => {
    hoisted.paths = ["/work/a shot.png", "/work/notes.md"];

    await pane();
    await pasteFiles();

    expect(hoisted.held).toBe("'/work/a shot.png' '/work/notes.md'");
    expect(wrote(), "a paste went off to the program on its own").toEqual([]);
  });

  it("puts them at the caret, and leaves what is written where it was", async () => {
    hoisted.paths = ["/work/a shot.png"];

    await pane();
    await write("読んで  ください");
    box().setSelectionRange(4, 4);
    await pasteFiles();

    expect(hoisted.held).toBe("読んで '/work/a shot.png' ください");
    // And the caret after what was put in, so the next thing typed follows the path rather than
    // the sentence it was pasted into the middle of.
    expect(box().selectionStart).toBe(4 + "'/work/a shot.png'".length);
  });

  // A clipboard the host will not name a path off still carries the words somebody copied
  // (`../core/clipFiles`), and those go in as they stand — they are not a path to quote.
  it("falls back to the words the paste carried", async () => {
    await pane();
    await pasteFiles({ words: "書き写した文" });

    expect(hoisted.held).toBe("書き写した文");
  });
});

describe("what a paste says was put into the box", () => {
  // The send waits the pane's agent out for a file the body names, and what settles whether it does
  // is whether Amenbo put the path there (`../talk/layout`, `../talk/terminal`, `AMB-D-879`). So a
  // paste that puts paths in says which, beside the body it put them into.
  it("names the paths it put in", async () => {
    hoisted.paths = ["/work/a shot.png", "/work/notes.md"];

    await pane();
    await pasteFiles();

    expect(hoisted.put).toEqual(["/work/a shot.png", "/work/notes.md"]);
  });

  it("names the path a picture was written down at", async () => {
    await pane();
    await pasteImage();

    expect(hoisted.put).toEqual(["/tmp/amenbo-pasted-7a/pasted-0a0b0c0d.png"]);
  });

  // Words are not a path, so there is no file for the agent to stop over and nothing to name.
  it("names nothing where what went in was the words the paste carried", async () => {
    await pane();
    await pasteFiles({ words: "書き写した文" });

    expect(hoisted.put).toEqual([]);
  });
});

describe("pasting a picture into the box under a pane", () => {
  // A screenshot is bytes and no file, so there is nothing to name until the host has written it
  // down — in this pane's own directory, because a pane has a session (`AMB-D-854`).
  it("writes it down under this pane's session and puts the path in, quoted", async () => {
    await pane();
    await pasteImage();

    expect(hoisted.asked.find((one) => one.cmd === "pty_paste_image")?.args).toMatchObject({
      session: "session-7",
      mime: "image/png",
    });
    expect(hoisted.held).toBe("'/tmp/amenbo-pasted-7a/pasted-0a0b0c0d.png'");
  });

  it("puts nothing in where the host could not write it down", async () => {
    hoisted.wrote = null;

    await pane();
    await write("書きかけ");
    await pasteImage();

    expect(hoisted.held).toBe("書きかけ");
  });

  // **On Linux the picture comes in by the press**, and the press is a text box's own: `Ctrl+V`,
  // which is what a person writing here will actually make. A pane holds out for `Ctrl+Shift+V`
  // because `Ctrl+V` is `^V` to the program in it, and there is no program behind this box.
  it("is read off the clipboard on the press a text box takes", async () => {
    await pane();
    await press({ shiftKey: true });
    expect(read, "the pane's press was taken by the box").not.toHaveBeenCalled();

    await press();

    expect(read).toHaveBeenCalled();
    expect(hoisted.held).toBe("'/tmp/amenbo-pasted-7a/pasted-0a0b0c0d.png'");
  });
});
