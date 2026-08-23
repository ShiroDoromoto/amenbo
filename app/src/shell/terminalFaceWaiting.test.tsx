// @vitest-environment jsdom
// The face tells the shell one thing, and this is the wiring that carries it: a pane waiting on a
// person, said as it happens (`AMB-D-753`). What is pinned here is that it follows `waiting` and not
// the pane's chatter — an agent at work says a great deal, and the shell must not be woken by it —
// and that the pane going away takes the turn with it.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SessionSaidDto } from "../bindings/bindings";
import type { PaneEvents } from "../talk/terminal";
import { TerminalFace } from "./TerminalFace";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// The pane, stood in for — and kept, so the test can say what the host would have said.
const hoisted = vi.hoisted(() => ({ on: null as PaneEvents | null }));
vi.mock("../talk/terminal", () => ({
  mountTerminal: (_host: HTMLElement, on: PaneEvents) => {
    hoisted.on = on;
    return Promise.resolve(() => {});
  },
}));

const AT = "2026-08-24T09:00:00Z";
const say = (over: Partial<SessionSaidDto> & Pick<SessionSaidDto, "verb">): SessionSaidDto =>
  ({ session: "pane-1", at: AT, ...over });

let container: HTMLDivElement;
let root: Root;
let told: boolean[];

beforeEach(async () => {
  hoisted.on = null;
  told = [];
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  await act(async () =>
    root.render(
      createElement(TerminalFace, {
        onSplitOut: () => {},
        note: null,
        onWaiting: (waiting: boolean) => told.push(waiting),
      }),
    ),
  );
});

afterEach(() => {
  container.remove();
});

describe("what the terminal face tells the shell", () => {
  it("says a turn came, once, and says it is over when the agent goes back to work", async () => {
    await act(async () => {
      hoisted.on!.opened("pane-1", AT);
      hoisted.on!.said(say({ verb: "note", text: "running the tests" }));
    });
    expect(told, "the shell was woken by a pane merely working").toEqual([]);

    await act(async () => hoisted.on!.said(say({ verb: "waiting", text: "which of the two" })));
    expect(told).toEqual([true]);

    // The pane keeps talking while the turn stands. The shell hears the change, not the chatter.
    await act(async () => hoisted.on!.said(say({ verb: "waiting", text: "still which of the two" })));
    expect(told).toEqual([true]);

    await act(async () => hoisted.on!.said(say({ verb: "note", text: "on it" })));
    expect(told).toEqual([true, false]);
  });

  it("takes the turn away with the pane, however the pane goes", async () => {
    await act(async () => {
      hoisted.on!.opened("pane-1", AT);
      hoisted.on!.said(say({ verb: "waiting", text: "which of the two" }));
    });
    expect(told).toEqual([true]);

    await act(async () => hoisted.on!.closed("pane-1"));
    expect(told, "the program exited and the badge was left standing").toEqual([true, false]);
  });

  it("takes the turn away when the face itself comes down", async () => {
    await act(async () => {
      hoisted.on!.opened("pane-1", AT);
      hoisted.on!.said(say({ verb: "waiting", text: "which of the two" }));
    });
    expect(told).toEqual([true]);

    // Split out into a window of its own: the pane detaches and this face is gone.
    await act(async () => root.unmount());
    expect(told, "the face went and the badge was left standing").toEqual([true, false]);
  });
});
