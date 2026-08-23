// @vitest-environment jsdom
// The three shapes the frame draws, and the one rule about when the agent may be changed.
//
// Only the two boundaries are stubbed — what the host answers, and the terminal itself — so the
// branching, the remembering, and the row on a closed pane all run for real.
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { PtySessionDto, WakeDto } from "../bindings/bindings";
import type { PaneEvents } from "./terminal";

const hoisted = vi.hoisted(() => ({
  /** What `wake_probe` answers with, in order — the last one is repeated. */
  answers: [] as WakeDto[],
  /** Every command that crossed, as `[name, args]`. */
  sent: [] as [string, Record<string, unknown> | undefined][],
  /** Where each pane was started, most recent last. */
  panes: [] as { cwd?: string | null; agent?: string | null }[],
  /** Ends the pane most recently mounted, the way the host's `pty://closed` does. */
  end: null as (() => void) | null,
  /** What `pty_sessions` answers with — a terminal already running is one nothing is asked about. */
  running: [] as PtySessionDto[],
}));

vi.mock("../core/ipc", () => ({
  invoke: vi.fn(async (name: string, args?: Record<string, unknown>) => {
    hoisted.sent.push([name, args]);
    if (name === "pty_sessions") return hoisted.running;
    if (name === "wake_probe") {
      return hoisted.answers.length > 1 ? hoisted.answers.shift() : hoisted.answers[0];
    }
    return undefined;
  }),
}));
vi.mock("./terminal", () => ({
  mountTerminal: vi.fn(
    async (
      host: HTMLElement,
      on: PaneEvents,
      start: { cwd?: string | null; agent?: string | null },
    ) => {
      hoisted.panes.push(start);
      hoisted.end = () => on.closed("session-1");
      host.textContent = "(a terminal)";
      return () => {};
    },
  ),
}));

import { mountAgentFrame } from "./agent";

/** A host answer, with the parts a test does not care about filled in. */
function wake(over: Partial<WakeDto> = {}): WakeDto {
  return {
    folder: "/work/here",
    candidates: [
      { id: "claude-code", label: "Claude Code", command: "claude", traced: true, installed: true },
      { id: "codex-cli", label: "Codex CLI", command: "codex", traced: true, installed: true },
    ],
    offered: ["claude-code", "codex-cli"],
    ...over,
  };
}

/** What the window is told by the panes under it, counted rather than kept. */
const heard = { opened: 0, said: 0, closed: 0, named: 0 };
const events: PaneEvents = {
  opened: () => {
    heard.opened += 1;
  },
  said: () => {
    heard.said += 1;
  },
  closed: () => {
    heard.closed += 1;
  },
  name: () => {
    heard.named += 1;
  },
};

/** Draw the frame into a fresh page and hand back its root. */
async function draw(answer: WakeDto): Promise<HTMLElement> {
  hoisted.answers = [answer];
  const root = document.createElement("div");
  document.body.replaceChildren(root);
  await mountAgentFrame(root, "en", events, "pane");
  return root;
}

/** The buttons on screen, by their text. */
function buttons(root: HTMLElement): HTMLButtonElement[] {
  return [...root.querySelectorAll("button")];
}

beforeEach(() => {
  hoisted.sent = [];
  hoisted.panes = [];
  hoisted.end = null;
  hoisted.running = [];
  heard.opened = heard.said = heard.closed = heard.named = 0;
});

describe("the frame draws what the host settled", () => {
  it("opens the pane without asking when one agent answers", async () => {
    const root = await draw(wake({ offered: ["claude-code"], settled: "claude-code" }));

    expect(hoisted.panes).toEqual([{ cwd: "/work/here", agent: "claude-code" }]);
    expect(buttons(root)).toEqual([]);
    expect(hoisted.sent.map(([name]) => name)).not.toContain("wake_remember");
  });

  it("offers the choice when several answer, and keeps the one that is picked", async () => {
    const root = await draw(wake());
    expect(hoisted.panes).toEqual([]);

    const choice = buttons(root).find((b) => b.textContent === "Codex CLI");
    expect(choice, "the offer did not name the agents").toBeTruthy();
    choice?.click();
    await Promise.resolve();

    expect(hoisted.sent).toContainEqual([
      "wake_remember",
      { folder: "/work/here", agent: "codex-cli" },
    ]);
    expect(hoisted.panes).toEqual([{ cwd: "/work/here", agent: "codex-cli" }]);
  });

  it("says what it looked for, and looks again on request, when nothing is startable", async () => {
    const root = await draw(wake({ offered: [] }));

    expect(hoisted.panes).toEqual([]);
    expect(root.textContent).toContain("claude, codex");

    const again = buttons(root).find((b) => b.textContent === "Search again");
    expect(again, "there was no way to look again").toBeTruthy();
    hoisted.answers = [wake({ offered: ["claude-code"], settled: "claude-code" })];
    again?.click();
    await new Promise((r) => setTimeout(r, 0));

    expect(hoisted.panes).toEqual([{ cwd: "/work/here", agent: "claude-code" }]);
  });
});

describe("the agent can be changed only on a frame that has closed", () => {
  it("puts nothing to press while the pane is running", async () => {
    const root = await draw(wake({ offered: ["claude-code"], settled: "claude-code" }));
    expect(buttons(root)).toEqual([]);
    expect(root.querySelector("select")).toBeNull();
  });

  it("offers the agent and open once the program has ended", async () => {
    const root = await draw(wake({ settled: "claude-code" }));
    expect(root.querySelector("select")).toBeNull();

    hoisted.end?.();
    expect(heard.closed, "the window was not told the pane closed").toBe(1);
    const choose = root.querySelector("select");
    expect(choose, "the closed frame had no way to change the agent").toBeTruthy();
    expect(choose?.value).toBe("claude-code");

    if (choose) {
      choose.value = "codex-cli";
      choose.dispatchEvent(new Event("change"));
    }
    buttons(root).find((b) => b.textContent === "Open")?.click();
    await Promise.resolve();

    expect(hoisted.sent).toContainEqual([
      "wake_remember",
      { folder: "/work/here", agent: "codex-cli" },
    ]);
    expect(hoisted.panes[hoisted.panes.length - 1]).toEqual({
      cwd: "/work/here",
      agent: "codex-cli",
    });
  });

  it("does not write the answer down again when the same agent is opened", async () => {
    const root = await draw(wake({ settled: "claude-code" }));
    hoisted.end?.();
    buttons(root).find((b) => b.textContent === "Open")?.click();
    await Promise.resolve();

    expect(hoisted.sent.map(([name]) => name)).not.toContain("wake_remember");
    expect(hoisted.panes).toHaveLength(2);
  });
});
