// @vitest-environment jsdom
// The first loop's one press, from the far side: the ledger hands the face a folder, and a pane opens
// in it with nothing asked (`../components/FirstLoop`, `./AppShell`).
//
// It has a frame of its own rather than riding in `terminalLayout.test.tsx` because the question is
// about the frame that has **not** started: there the stand-in opens a session the moment it is put
// up, which is a face where every slot is busy and the handover has nothing to replace. The frame
// here answers the way the real one does — with no folder it starts nothing and puts the invitation
// up (`../talk/agent`) — which is the state a reader is in when they press the button.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PaneStart } from "../talk/terminal";
import { TerminalFace } from "./TerminalFace";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const hoisted = vi.hoisted(() => ({ mounts: [] as { cwd?: string; opened: boolean }[] }));

vi.mock("../talk/agent", () => ({
  mountAgentFrame: (
    _host: HTMLElement,
    _lang: string,
    on: { opened: (s: string, at: string, where?: string) => void },
    _paneClass: string,
    start: PaneStart = {},
  ) => {
    // No folder, no terminal: that frame is the invitation, and the invitation is what the press
    // arrives to replace.
    const opens = start.cwd !== undefined && start.cwd !== null;
    hoisted.mounts.push({ cwd: start.cwd ?? undefined, opened: opens });
    if (opens) on.opened(`s${hoisted.mounts.length}`, "2026-08-24T00:00:00Z", start.cwd!);
    return Promise.resolve(() => {});
  },
}));

let container: HTMLDivElement;
let root: Root;

const draw = (openIn: { dir: string; nth: number } | null) =>
  act(async () => {
    root.render(createElement(TerminalFace, {
      onSplitOut: () => {}, note: null, onWaiting: () => {}, openIn,
    }));
  });

beforeEach(() => {
  hoisted.mounts = [];
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("a folder handed in from the ledger", () => {
  it("opens a pane in it, over the invitation that was standing there", async () => {
    await draw(null);
    expect(hoisted.mounts.map((one) => one.opened), "a terminal was started with no folder to start it in")
      .toEqual([false]);

    await draw({ dir: "/work/handed", nth: 1 });

    expect(hoisted.mounts.map((one) => one.cwd)).toEqual([undefined, "/work/handed"]);
    expect(hoisted.mounts[hoisted.mounts.length - 1]!.opened, "the pane offered to open instead of opening").toBe(true);
  });

  it("goes to it rather than starting a second terminal in a folder that has one", async () => {
    await draw({ dir: "/work/handed", nth: 1 });
    const after = hoisted.mounts.length;

    await draw({ dir: "/work/handed", nth: 2 });

    expect(hoisted.mounts).toHaveLength(after);
    expect(container.querySelector(".termface__page--on")!.textContent).toBe("1");
  });

  it("puts a second folder on a page of its own, so one screen stays one project", async () => {
    await draw({ dir: "/work/one", nth: 1 });
    await draw({ dir: "/work/two", nth: 2 });

    expect(hoisted.mounts.map((one) => one.cwd)).toEqual([undefined, "/work/one", "/work/two"]);
    expect(container.querySelector(".termface__page--on")!.textContent).toBe("2");
  });
});
