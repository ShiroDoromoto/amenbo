// @vitest-environment jsdom
// Carrying a row of the panel to a pane, from the face's side (`AMB-D-820`).
//
// The gesture itself is pinned where it is written (`../files/handDrag`); what only the face can
// answer is **which** pane — the one the row came down on, which is not the one being worked in, and
// the difference between the two is the whole reason this road exists beside the row's menu
// (`./panelHandOver`).
//
// And what follows from the landing: a person who carried a path into a pane has said which pane
// they mean, so it becomes the one being worked in and the keyboard goes there with the path — the
// same thing a drop from the desktop does (`./TerminalPane`).
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PointerEvent as RowPress } from "react";
import type { PaneStart } from "../talk/terminal";

const hoisted = vi.hoisted(() => ({
  saved: null as unknown,
  running: [] as { session: string; startedAt: string; folder: string | null }[],
  /** The gesture the face handed the panel, which every row of it would put on its press. */
  carry: undefined as undefined | ((whole: string, event: RowPress<HTMLElement>) => void),
  /** Every paste the face asked for: the session it named, and the text. */
  pasted: [] as { session: string; text: string }[],
}));

vi.mock("../talk/agent", () => ({
  mountAgentFrame: (
    host: HTMLElement,
    _lang: string,
    on: { opened: (s: string, at: string) => void },
    start: PaneStart = {},
  ) => {
    on.opened(start.session ?? "unasked", "2026-08-25T00:00:00Z");
    // The box the emulator collects typing in, which is what the keyboard lands on. It is the one
    // thing of the real terminal this stub keeps (`../talk/terminal`).
    host.append(document.createElement("textarea"));
    return Promise.resolve(() => {});
  },
}));

vi.mock("../talk/terminal", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../talk/terminal")>()),
  pasteIntoTerminal: async (session: string, text: string) => {
    hoisted.pasted.push({ session, text });
  },
}));

vi.mock("../talk/frames", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../talk/frames")>()),
  frameNames: async () => new Map(),
  nameFrame: async () => new Map(),
  savedLayout: async () => hoisted.saved,
  keepLayout: async () => {},
}));

vi.mock("../core/snapshot", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../core/snapshot")>()),
  inTauri: () => true,
}));

// The panel draws nothing here: what is under test is what the face hands it, and what the face does
// with what comes back.
vi.mock("../files/FilesPanel", () => ({
  FilesPanel: (props: {
    onCarry?: (whole: string, event: RowPress<HTMLElement>) => void;
  }) => {
    hoisted.carry = props.onCarry;
    return null;
  },
}));

vi.mock("../mock/adapter", () => ({
  dataAdapter: { listProjects: () => [{ id: 1, name: "amenbo" }] },
}));
vi.mock("../core/boundFolders", () => ({
  useBoundFolders: () => ({
    all: [{ path: "/work/a", exists: true }],
    live: [{ path: "/work/a", exists: true }],
    answered: true,
  }),
}));

vi.mock("../core/ipc", async (importOriginal) => {
  const real = await importOriginal<typeof import("../core/ipc")>();
  return {
    ...real,
    invoke: async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "pty_sessions") return hoisted.running;
      if (cmd === "panes_drawn") return undefined;
      return real.invoke(cmd, args);
    },
  };
});

import { TerminalFace } from "./TerminalFace";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
/** The row a reader takes hold of. It stands outside the face, the way the panel's rows stand beside
 *  it, and it is what the gesture clones and captures the pointer on. */
let row: HTMLLIElement;

async function mount() {
  await act(async () => {
    root.render(createElement(TerminalFace, {
      onWindow: () => {}, note: null, onWaiting: () => {}, goPane: null,
    }));
    await new Promise((r) => setTimeout(r, 0));
  });
}

/** jsdom lays nothing out, so what is under the pointer is stated rather than measured. */
function under(el: Element | null): void {
  (document as unknown as { elementFromPoint: () => Element | null }).elementFromPoint = () => el;
}

/** The pane of a frame, as a drop is matched against it. */
const pane = (frame: string) => container.querySelector<HTMLElement>(`[data-hand="${frame}"]`);
/** The panes drawing the surface that says they would take what is being carried. */
const offering = () => [...container.querySelectorAll(".slot")]
  .filter((one) => one.querySelector(".slot__handing"))
  .map((one) => one.getAttribute("data-hand"));

function pointer(kind: string, x: number, y: number): PointerEvent {
  const e = new MouseEvent(kind, { bubbles: true, clientX: x, clientY: y, button: 0 });
  Object.defineProperty(e, "pointerId", { value: 7 });
  return e as PointerEvent;
}

/** Take the row — which stands for `/work/a/notes.md` — and carry it onto `frame`, without letting
 *  go. */
async function carryOnto(frame: string) {
  await act(async () => {
    row.dispatchEvent(pointer("pointerdown", 10, 10));
  });
  under(pane(frame));
  await act(async () => {
    row.dispatchEvent(pointer("pointermove", 300, 300));
    await new Promise((r) => setTimeout(r, 20));
  });
}

/** And let it go where it is. */
async function letGo() {
  await act(async () => {
    row.dispatchEvent(pointer("pointerup", 300, 300));
    await new Promise((r) => setTimeout(r, 20));
  });
}

/** Press the rail's row for a pane, which is how a reader says which one they are working in. */
async function focusPane(name: string) {
  const at = [...container.querySelectorAll<HTMLElement>(".rail__row")]
    .find((one) => one.querySelector(".rail__name")?.textContent === name);
  await act(async () => {
    at?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await new Promise((r) => setTimeout(r, 0));
  });
}

/** Which pane the face says is being worked in, as the pane itself is drawn. */
const worked = () => container.querySelector(".slot--focused")?.getAttribute("data-hand") ?? null;

beforeEach(() => {
  window.innerWidth = 1600;
  hoisted.carry = undefined;
  hoisted.pasted = [];
  hoisted.saved = {
    count: 2,
    nextId: 3,
    project: 1,
    frames: [
      { id: "1", project: 1, folder: "/work/a" },
      { id: "2", project: 1, folder: "/work/b" },
    ],
  };
  hoisted.running = ["a", "b"].map((one, at) => ({
    session: `s-${one}`,
    startedAt: `2026-08-25T00:00:0${at}Z`,
    folder: `/work/${one}`,
  }));
  Element.prototype.setPointerCapture = () => {};
  Element.prototype.releasePointerCapture = () => {};
  Element.prototype.hasPointerCapture = () => false;
  container = document.createElement("div");
  document.body.appendChild(container);
  row = document.createElement("li");
  row.textContent = "notes.md";
  row.addEventListener("pointerdown", (e) => {
    hoisted.carry?.("/work/a/notes.md", e as unknown as RowPress<HTMLElement>);
  });
  document.body.appendChild(row);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
  row.remove();
  document.body.className = "";
  under(null);
});

describe("carrying a row of the panel onto a pane", () => {
  it("hands the panel the gesture, so every row of it can be taken hold of", async () => {
    await mount();
    expect(hoisted.carry, "the panel was drawn with no way to carry a row out of it").toBeDefined();
  });

  it("offers the pane under the pointer, and only that one", async () => {
    await mount();
    await carryOnto("2");
    expect(offering()).toEqual(["2"]);
  });

  it("puts the path in front of what is running in the pane it came down on", async () => {
    await mount();
    // The reader is working in the first pane, and carries the row onto the second: the path goes
    // where it was let go, which is what tells this road from the row's menu.
    await focusPane("a");
    await carryOnto("2");
    await letGo();
    expect(hoisted.pasted).toEqual([{ session: "s-b", text: "'/work/a/notes.md'" }]);
    expect(offering(), "the surface stayed after the row had landed").toEqual([]);
  });

  it("takes that pane as the one being worked in, and moves the keyboard into it", async () => {
    await mount();
    await focusPane("a");
    expect(worked()).toBe("1");

    await carryOnto("2");
    await letGo();

    expect(worked(), "the path went into a pane the face still called unselected").toBe("2");
    expect(
      document.activeElement,
      "the reader would have typed into the pane they came from",
    ).toBe(pane("2")?.querySelector("textarea"));
  });

  it("hands nothing to a pane with nothing running in it", async () => {
    hoisted.running = [];
    await mount();
    await carryOnto("2");
    expect(offering(), "a pane with nothing running in it offered to take a row").toEqual([]);
    await letGo();
    expect(hoisted.pasted).toEqual([]);
  });
});
