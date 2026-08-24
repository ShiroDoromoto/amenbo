// @vitest-environment jsdom
// The one thing the switch between the two faces must not do: kill the terminal (`AMB-D-753`).
//
// The rule lives in how AppShell renders the face — `hidden` on a container that stays mounted, not
// a conditional that takes it away — and it is invisible in the code either way round, because both
// spellings look right and only one of them keeps the emulator. What is pinned here is that
// switching back and forth puts up exactly one terminal and never detaches it, against a control
// that renders conditionally and loses one on every switch.
import { act, createElement, useState } from "react";
import type { ComponentType } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { TerminalFace } from "./TerminalFace";

// React 18's act() requires this environment flag to be set.
(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const hoisted = vi.hoisted(() => ({ mounted: 0, detached: 0 }));

// The pane, stood in for: putting one up is a host round trip, and what is being counted here is how
// many times the component asks for one — not what a terminal does once it has one. What is stubbed
// is the frame around the pane (`../talk/agent`), because that is what the face asks for now: which
// agent to start is settled there, and the terminal is put up on the far side of it.
vi.mock("../talk/agent", () => ({
  mountAgentFrame: () => {
    hoisted.mounted++;
    return Promise.resolve(() => {
      hoisted.detached++;
    });
  },
}));

// The ledger's projects and the one folder this one is bound to, so the pane this is about is one
// press away and nothing is asked on the way (`./FolderChoice`).
vi.mock("../mock/adapter", () => ({
  dataAdapter: { listProjects: () => [{ id: 1, name: "amenbo" }] },
}));
vi.mock("../core/boundFolders", () => ({
  useBoundFolders: () => ({
    all: [{ path: "/repo", exists: true }],
    live: [{ path: "/repo", exists: true }],
    answered: true,
  }),
}));

let container: HTMLDivElement;
let root: Root;

// The production wiring: the face is mounted from the moment it is first asked for, and the other
// face being up is `hidden` on its container.
function HiddenShell({ face }: { face: "tasks" | "terminal" }) {
  return createElement(
    "div",
    { hidden: face !== "terminal" },
    createElement(TerminalFace, { onSplitOut: () => {}, note: null, onWaiting: () => {} }),
  );
}

// The control: the same face, rendered only while it is the one showing.
function ConditionalShell({ face }: { face: "tasks" | "terminal" }) {
  return face === "terminal"
    ? createElement(TerminalFace, { onSplitOut: () => {}, note: null, onWaiting: () => {} })
    : null;
}

function Switcher({ shell }: { shell: ComponentType<{ face: "tasks" | "terminal" }> }) {
  const [face, setFace] = useState<"tasks" | "terminal">("terminal");
  return createElement(
    "div",
    null,
    createElement("button", { onClick: () => setFace(face === "tasks" ? "terminal" : "tasks") }, "switch"),
    createElement(shell, { face }),
  );
}

beforeEach(() => {
  // The face measures the window to work out whether the columns beside the panes are columns at all
  // (`../talk/columns`). jsdom's window is 1024, which is genuinely too narrow for two panes and two
  // columns — so a test about what is drawn beside the panes says it is on a wide screen.
  window.innerWidth = 1600;
  hoisted.mounted = 0;
  hoisted.detached = 0;
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

const flip = async () => {
  await act(async () => {
    container.querySelector("button")!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  });
};

/** Open the one pane the switching is about. A pane is made by opening one (`../talk/layout`), so
 *  there is nothing to lose track of until somebody has. */
const openPane = async () => {
  await act(async () => {
    container.querySelector(".slot--empty .slot__open")!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  });
};

describe("switching faces does not kill the terminal", () => {
  it("hiding the face keeps the one pane it put up, however many times the user switches", async () => {
    await act(async () => root.render(createElement(Switcher, { shell: HiddenShell })));
    await openPane();
    expect(hoisted.mounted).toBe(1);

    await flip(); // to the ledger
    await flip(); // and back
    await flip();
    await flip();

    expect(hoisted.mounted, "a second terminal was started").toBe(1);
    expect(hoisted.detached, "the pane was taken away behind the user's back").toBe(0);
  });

  it("rendering it only while it shows — the spelling that looks the same — loses the pane on the first switch", async () => {
    await act(async () => root.render(createElement(Switcher, { shell: ConditionalShell })));
    await openPane();
    expect(hoisted.mounted).toBe(1);

    await flip();
    await flip();

    // The pane went with the face, and what comes back has nothing open in it: a pane is made by
    // opening one, so the terminal the reader was in is not even redrawn — it is gone with nothing
    // on the screen to say so.
    expect(hoisted.detached).toBe(1);
    expect(hoisted.mounted).toBe(1);
    expect(container.querySelector(".slot--empty"), "the pane came back").not.toBeNull();
  });
});
