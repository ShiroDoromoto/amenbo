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
// many times the component asks for one — not what a terminal does once it has one.
vi.mock("../talk/terminal", () => ({
  mountTerminal: () => {
    hoisted.mounted++;
    return Promise.resolve(() => {
      hoisted.detached++;
    });
  },
}));

let container: HTMLDivElement;
let root: Root;

// The production wiring: the face is mounted from the moment it is first asked for, and the other
// face being up is `hidden` on its container.
function HiddenShell({ face }: { face: "tasks" | "terminal" }) {
  return createElement(
    "div",
    { hidden: face !== "terminal" },
    createElement(TerminalFace, { onSplitOut: () => {}, note: null }),
  );
}

// The control: the same face, rendered only while it is the one showing.
function ConditionalShell({ face }: { face: "tasks" | "terminal" }) {
  return face === "terminal"
    ? createElement(TerminalFace, { onSplitOut: () => {}, note: null })
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

describe("switching faces does not kill the terminal", () => {
  it("hiding the face keeps the one pane it put up, however many times the user switches", async () => {
    await act(async () => root.render(createElement(Switcher, { shell: HiddenShell })));
    expect(hoisted.mounted).toBe(1);

    await flip(); // to the ledger
    await flip(); // and back
    await flip();
    await flip();

    expect(hoisted.mounted, "a second terminal was started").toBe(1);
    expect(hoisted.detached, "the pane was taken away behind the user's back").toBe(0);
  });

  it("rendering it only while it shows — the spelling that looks the same — loses the pane on every switch", async () => {
    await act(async () => root.render(createElement(Switcher, { shell: ConditionalShell })));
    expect(hoisted.mounted).toBe(1);

    await flip();
    await flip();

    expect(hoisted.detached).toBe(1);
    expect(hoisted.mounted).toBe(2);
  });
});
