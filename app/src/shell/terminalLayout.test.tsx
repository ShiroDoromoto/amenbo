// @vitest-environment jsdom
// What the arrangement has to do to the panes, which the layout's own tests cannot see: they know
// where a frame is, not whether turning a page killed the terminal in one.
//
// The two things pinned here are the ones that are invisible in code that looks right either way.
// Turning a page takes panes down, and the terminals in them must be *picked up* when the page comes
// back rather than started again — a second shell where the reader left one is a lost session with
// nothing to say it happened. And a pane opened on a page must open in that page's folder, which is
// the whole of what keeps one screen to one project (`../talk/layout`).
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PaneStart } from "../talk/terminal";
import { TerminalFace } from "./TerminalFace";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

type Mounted = { start: PaneStart; said: (statement: unknown) => void; session: string };

const hoisted = vi.hoisted(() => ({ mounts: [] as unknown[], detached: 0 }));

// The frame a slot puts up, stood in for. It answers the way a real one does — a session id comes
// back, and what the agent in it says arrives through the same callback — so the face has something
// to arrange. What agent runs in it is the frame's own question and not this one's (`../talk/agent`).
vi.mock("../talk/agent", () => ({
  mountAgentFrame: (
    _host: HTMLElement,
    _lang: string,
    on: { opened: (s: string, at: string) => void; said: (statement: unknown) => void },
    _paneClass: string,
    start: PaneStart = {},
  ) => {
    const session = start.session ?? `s${hoisted.mounts.length + 1}`;
    hoisted.mounts.push({ start, said: on.said, session });
    on.opened(session, "2026-08-24T00:00:00Z");
    return Promise.resolve(() => { hoisted.detached++; });
  },
}));

let container: HTMLDivElement;
let root: Root;

const mounts = () => hoisted.mounts as Mounted[];
const q = (sel: string) => [...container.querySelectorAll<HTMLElement>(sel)];
const click = async (el: HTMLElement) => {
  await act(async () => { el.dispatchEvent(new MouseEvent("click", { bubbles: true })); });
};
const press = async (key: string) => {
  await act(async () => {
    document.dispatchEvent(new KeyboardEvent("keydown", { key, metaKey: true, bubbles: true }));
  });
};

beforeEach(async () => {
  hoisted.mounts = [];
  hoisted.detached = 0;
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  await act(async () => {
    root.render(createElement(TerminalFace, { onSplitOut: () => {}, note: null, onWaiting: () => {} }));
  });
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the face comes up arranged", () => {
  it("puts a terminal in the first slot and leaves the rest of the page to be asked for", () => {
    expect(mounts()).toHaveLength(1);
    // Two panes to a page by default: one running, one offering to start.
    expect(q(".slot--empty")).toHaveLength(1);
  });

  it("offers a page beyond the ones the frames fill, so there is somewhere to put the next pane", () => {
    expect(q(".termface__page")).toHaveLength(1);
  });
});

describe("one page, one project", () => {
  it("opens the next pane in the folder the page's first terminal is in", async () => {
    // The agent in the first pane says where it is; that is what the page is.
    await act(async () => {
      mounts()[0]!.said({ session: "s1", verb: "note", at: "2026-08-24T00:01:00Z", cwd: "/repo", text: "on it" });
    });
    await click(q(".slot--empty")[0]!);
    expect(mounts()).toHaveLength(2);
    expect(mounts()[1]!.start.cwd).toBe("/repo");
  });

  it("gives a fresh page its own folder rather than the one before it", async () => {
    await act(async () => {
      mounts()[0]!.said({ session: "s1", verb: "note", at: "2026-08-24T00:01:00Z", cwd: "/repo", text: "on it" });
    });
    await click(q(".slot--empty")[0]!);
    await press("2");
    await click(q(".slot--empty")[0]!);
    expect(mounts()[2]!.start.cwd).toBeNull();
  });
});

describe("turning a page", () => {
  it("takes the panes down and picks the same terminals up again — never starts a second", async () => {
    // A page beyond the first exists once this one is full.
    await click(q(".slot--empty")[0]!);
    const started = mounts().map((one) => one.session);

    await press("2");
    expect(hoisted.detached, "the panes were left drawn on a page nobody is on").toBe(2);
    expect(mounts(), "turning a page started a terminal").toHaveLength(2);

    await press("1");
    expect(mounts()).toHaveLength(4);
    expect(mounts().slice(2).map((one) => one.start.session), "the panes were given different terminals")
      .toEqual(started);
  });

  it("does not answer a digit while the other face is the one showing", async () => {
    // The face is kept mounted behind `hidden` so the emulator survives the switch — which means a
    // page could be turned under a reader who is looking at the ledger.
    const hidden = document.createElement("div");
    hidden.hidden = true;
    document.body.appendChild(hidden);
    const other = createRoot(hidden);
    await act(async () => {
      other.render(createElement(TerminalFace, { onSplitOut: () => {}, note: null, onWaiting: () => {} }));
    });
    const before = hidden.querySelector(".termface__page--on")!.textContent;
    await act(async () => {
      document.dispatchEvent(new KeyboardEvent("keydown", { key: "2", metaKey: true, bubbles: true }));
    });
    expect(hidden.querySelector(".termface__page--on")!.textContent).toBe(before);
    act(() => other.unmount());
    hidden.remove();
  });
});

describe("how many panes", () => {
  it("carries the pane being worked in over to the page it is on at the new count", async () => {
    await click(q(".slot--empty")[0]!);            // a second pane, on page 1 at two a page
    await click(q(".termface__count")[0]!);        // one pane a page

    // At one a page the second pane is page 2, and that is where the screen is: a person who asks for
    // one pane means the one they were looking at.
    expect(q(".termface__page--on")[0]!.textContent).toBe("2");
    expect(q(".slot")).toHaveLength(1);
    expect(mounts(), "the pane carried across was restarted rather than kept").toHaveLength(2);
  });
});
