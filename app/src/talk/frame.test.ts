// @vitest-environment jsdom
// The three shapes the frame draws, and the one rule about when the agent may be changed.
//
// Only the two boundaries are stubbed — what the host answers, and the terminal itself — so the
// branching, the remembering, and the row on a closed pane all run for real.
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { WakeDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  /** What `wake_probe` answers with, in order — the last one is repeated. */
  answers: [] as WakeDto[],
  /** Every command that crossed, as `[name, args]`. */
  sent: [] as [string, Record<string, unknown> | undefined][],
  /** The options the pane was mounted with, most recent last. */
  panes: [] as { cwd?: string | null; agent?: string | null }[],
  /** Ends the pane most recently mounted, the way the host's `pty://closed` does. */
  end: null as (() => void) | null,
}));

vi.mock("../core/ipc", () => ({
  invoke: vi.fn(async (name: string, args?: Record<string, unknown>) => {
    hoisted.sent.push([name, args]);
    if (name === "wake_probe") {
      return hoisted.answers.length > 1 ? hoisted.answers.shift() : hoisted.answers[0];
    }
    return undefined;
  }),
}));
vi.mock("./terminal", () => ({
  mountTerminal: vi.fn(async (host: HTMLElement, opts: { onClosed?: () => void }) => {
    hoisted.panes.push(opts as { cwd?: string | null; agent?: string | null });
    hoisted.end = () => opts.onClosed?.();
    host.textContent = "(a terminal)";
    return () => {};
  }),
}));

import { mountFrame } from "./frame";

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

/** Draw the frame into a fresh page and hand back its root. */
async function draw(answer: WakeDto): Promise<HTMLElement> {
  hoisted.answers = [answer];
  const root = document.createElement("div");
  document.body.replaceChildren(root);
  await mountFrame(root, "en");
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
});

describe("the frame draws what the host settled", () => {
  it("opens the pane without asking when one agent answers", async () => {
    const root = await draw(wake({ offered: ["claude-code"], settled: "claude-code" }));

    expect(hoisted.panes).toEqual([
      expect.objectContaining({ cwd: "/work/here", agent: "claude-code" }),
    ]);
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
    expect(hoisted.panes).toEqual([expect.objectContaining({ agent: "codex-cli" })]);
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

    expect(hoisted.panes).toEqual([expect.objectContaining({ agent: "claude-code" })]);
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
    const choose = root.querySelector("select");
    expect(choose, "the closed frame had no way to change the agent").toBeTruthy();
    expect(choose?.value).toBe("claude-code");
    expect(root.textContent).toContain("(a terminal)");

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
    expect(hoisted.panes[hoisted.panes.length - 1]).toEqual(expect.objectContaining({ agent: "codex-cli" }));
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
