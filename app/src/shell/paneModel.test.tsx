// @vitest-environment jsdom
// The row under a running pane that moves it to another model (`AMB-D-865`).
//
// What is being held here is what leaves the app: the bytes that go into somebody's terminal. Two of
// the six providers read a model name on that command's line as a *prompt* and bill for the answer
// (`AMB-T-4581`), so a test that only checked "something was sent" would pass on the day this starts
// costing readers money. Every press is checked against what was written to the PTY.
//
// The host is stubbed and everything else runs: which shape is drawn, what a press writes, and what
// the row says afterwards are the whole of the component.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentSwitchDto } from "../bindings/bindings";
import { PaneModel } from "./PaneModel";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const hoisted = vi.hoisted(() => ({
  /** How each agent is moved, as the host answers `wake_switch` — an agent not named here has no
      road, which is what a registered command and an unknown id both come back as. */
  switches: {} as Record<string, { command: string; carries: "named" | "picker" | "filter"; keeps: string | null }>,
  /** What each agent's own command answers when it is asked which models it can be started on. */
  models: {} as Record<string, { id: string; label: string }[]>,
  /** What this device already remembers for each agent. */
  kept: {} as Record<string, { chosen: { id: string; label: string } | null; history: { id: string; label: string }[]; flag: string | null }>,
  /** Which commands the row put to the host, in the order it asked. */
  asked: [] as string[],
  /** What was written into the terminal, in order: `send` carries the return behind it and `paste`
      does not, which is the difference the two providers with a picker turn on. */
  wrote: [] as { how: "send" | "paste"; text: string }[],
  /** Set to refuse the write, the way a terminal that ended between the press and the write does. */
  writeFails: false,
}));

// The two roads into a terminal, stubbed apart. Which of them a press takes is the thing under test:
// a line that settles the model goes as a person's own line, and a name for a picker's search box is
// pasted with nothing submitted behind it (`AMB-D-793`).
vi.mock("../talk/terminal", () => ({
  sendIntoTerminal: async (_session: string, text: string) => {
    if (hoisted.writeFails) throw new Error("that terminal is no longer open");
    hoisted.wrote.push({ how: "send", text });
  },
  pasteIntoTerminal: async (_session: string, text: string) => {
    hoisted.wrote.push({ how: "paste", text });
  },
}));

vi.mock("../core/ipc", () => ({
  invoke: vi.fn(async (cmd: string, args?: unknown) => {
    hoisted.asked.push(cmd);
    if (cmd === "wake_switch") {
      const { agent, model } = args as { agent: string; model: string | null };
      const how = hoisted.switches[agent];
      if (how === undefined) return null;
      const name = model?.trim() ?? "";
      const settled: AgentSwitchDto = {
        ...how,
        line: how.carries === "named" && name !== "" ? `${how.command} ${name}` : how.command,
        then: how.carries === "filter" && name !== "" ? name : null,
        settles: how.carries === "named" && name !== "",
      };
      return settled;
    }
    if (cmd === "agent_models") {
      return { models: hoisted.models[(args as { agent: string }).agent] ?? [], current: null };
    }
    if (cmd === "wake_model") {
      return hoisted.kept[(args as { agent: string }).agent]
        ?? { chosen: null, history: [], flag: "--model" };
    }
    if (cmd === "wake_chose_model") return undefined;
    throw new Error(`the row asked the host for ${cmd}`);
  }),
}));

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  hoisted.switches = {};
  hoisted.models = {};
  hoisted.kept = {};
  hoisted.asked = [];
  hoisted.wrote = [];
  hoisted.writeFails = false;
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

/** Draw the row for a pane running `agent`. */
async function draw(agent: string | null): Promise<void> {
  await act(async () => {
    root.render(createElement(PaneModel, { session: "session-7", agent }));
  });
  await act(async () => { await Promise.resolve(); });
}

function buttons(): HTMLButtonElement[] {
  return [...container.querySelectorAll("button")];
}

/** Press the button whose words contain this, and say so where there is none. */
async function press(words: string): Promise<void> {
  const one = buttons().find((b) => b.textContent?.includes(words));
  expect(one, `"${words}" was not pressable`).toBeTruthy();
  await act(async () => { one?.click(); });
  await act(async () => { await Promise.resolve(); });
}

/** Open the candidates, which is the press on the row itself. */
async function open(): Promise<void> {
  await act(async () => { container.querySelector<HTMLButtonElement>(".modelrow__now")?.click(); });
  await act(async () => { await Promise.resolve(); });
}

describe("the row is drawn for a provider that can be moved, and for nothing else", () => {
  it("draws nothing for a pane with a plain prompt in it", async () => {
    await draw(null);
    expect(container.querySelector(".modelrow"), "a shell was offered a model").toBeNull();
  });

  it("draws nothing where the host says there is no road", async () => {
    // A command the reader registered: Amenbo cannot name the program inside it (`AMB-D-794`), so
    // there is no slash command it could type.
    await draw("custom:1");
    expect(container.querySelector(".modelrow"), "a registered command was offered a model").toBeNull();
  });

  it("draws the row for a catalogued provider", async () => {
    hoisted.switches["claude-code"] = { command: "/model", carries: "named", keeps: "~/.claude/settings.json" };
    await draw("claude-code");
    expect(container.querySelector(".modelrow")).toBeTruthy();
    // Nothing is claimed about the model before anything has been pressed: what the pane opened on
    // is not something this row was told, and reading it off the screen is what the pane exists not
    // to do (`AMB-D-747`).
    expect(container.querySelector(".modelrow__now")?.textContent).toContain("Model");
  });
});

describe("what a press puts in the terminal", () => {
  it("names the model on the line where the provider takes one there, and says so afterwards", async () => {
    hoisted.switches["claude-code"] = { command: "/model", carries: "named", keeps: "~/.claude/settings.json" };
    hoisted.models["claude-code"] = [{ id: "sonnet", label: "Sonnet 5" }];
    await draw("claude-code");
    await open();
    await press("Sonnet 5");

    expect(hoisted.wrote).toEqual([{ how: "send", text: "/model sonnet" }]);
    // The line settled it, so the row says which model — and keeps it, the way a press on an empty
    // frame keeps one.
    expect(container.querySelector(".modelrow__now")?.textContent).toContain("Sonnet 5");
    expect(hoisted.asked).toContain("wake_chose_model");
    expect(container.querySelector(".modelrow__note"), "a settled line said it was waiting").toBeNull();
  });

  it("never puts the name on the line of a provider that reads one as a prompt", async () => {
    // Codex and OpenCode were both watched sending `/model <name>` to the model and being billed for
    // the answer (`AMB-T-4581`). This is the test that failure lands on.
    hoisted.switches["codex-cli"] = { command: "/model", carries: "picker", keeps: "~/.codex/config.toml" };
    hoisted.models["codex-cli"] = [{ id: "gpt-5.6-luna", label: "gpt-5.6-luna" }];
    await draw("codex-cli");
    await open();
    await press("gpt-5.6-luna");

    expect(hoisted.wrote).toEqual([{ how: "send", text: "/model" }]);
    for (const one of hoisted.wrote) {
      expect(one.text, "the name reached a line the provider reads as a prompt").not.toContain("gpt-5.6-luna");
    }
  });

  it("says the terminal is waiting where the provider's own picker opened", async () => {
    hoisted.switches["gemini-cli"] = { command: "/model", carries: "picker", keeps: null };
    hoisted.models["gemini-cli"] = [{ id: "gemini-3.5-flash", label: "Flash" }];
    await draw("gemini-cli");
    await open();
    await press("Flash");

    // Nothing is claimed about the model: what was chosen in the provider's picker is between the
    // person and the provider, and the row says what it did instead.
    expect(container.querySelector(".modelrow__note")?.textContent).toContain("waiting");
    expect(container.querySelector(".modelrow__now")?.textContent).toContain("Model");
    expect(hoisted.asked, "a picker that opened was recorded as a choice").not.toContain("wake_chose_model");
  });

  it("pastes the name into the picker's search box and submits nothing behind it", async () => {
    hoisted.switches["opencode"] = {
      command: "/models",
      carries: "filter",
      keeps: "~/.local/share/opencode/opencode.db",
    };
    hoisted.models["opencode"] = [{ id: "opencode/mimo", label: "MiMo" }];
    await draw("opencode");
    await open();
    await press("MiMo");

    expect(hoisted.wrote).toEqual([
      { how: "send", text: "/models" },
      { how: "paste", text: "opencode/mimo" },
    ]);
  });

  it("keeps what was written when the terminal ended under the press", async () => {
    hoisted.switches["claude-code"] = { command: "/model", carries: "named", keeps: null };
    hoisted.models["claude-code"] = [{ id: "sonnet", label: "Sonnet 5" }];
    hoisted.writeFails = true;
    await draw("claude-code");
    await open();
    await press("Sonnet 5");

    expect(container.querySelector(".modelrow__failed")).toBeTruthy();
    expect(container.querySelector(".modelrow__now")?.textContent, "a write that never landed moved the row")
      .toContain("Model");
  });
});

describe("what the reader is told before they press", () => {
  it("names the file the change lands in, where the provider keeps it past this session", async () => {
    hoisted.switches["cursor"] = { command: "/model", carries: "named", keeps: "~/.cursor/cli-config.json" };
    hoisted.models["cursor"] = [{ id: "auto", label: "Auto" }];
    await draw("cursor");
    await open();

    expect(container.textContent).toContain("~/.cursor/cli-config.json");
    expect(container.textContent, "the command going in was not said").toContain("/model");
  });

  it("says nothing about a file for the provider that changes only this session", async () => {
    hoisted.switches["github-copilot"] = { command: "/model", carries: "named", keeps: null };
    await draw("github-copilot");
    await open();

    expect(container.textContent).not.toContain("~/");
  });

  it("offers a box and the history where the provider has no list to give", async () => {
    // GitHub Copilot has no list door at all (`AMB-T-4581`), so what was typed for it before is the
    // whole of what anybody can offer.
    hoisted.switches["github-copilot"] = { command: "/model", carries: "named", keeps: null };
    hoisted.models["github-copilot"] = [];
    hoisted.kept["github-copilot"] = {
      chosen: null,
      history: [{ id: "auto", label: "auto" }],
      flag: "--model",
    };
    await draw("github-copilot");
    await open();

    expect(container.querySelector(".slot__field input"), "there was nowhere to type a name").toBeTruthy();
    await press("auto");
    expect(hoisted.wrote).toEqual([{ how: "send", text: "/model auto" }]);
  });

  it("puts a box over a list too long to be a row", async () => {
    hoisted.switches["cursor"] = { command: "/model", carries: "named", keeps: null };
    hoisted.models["cursor"] = Array.from({ length: 20 }, (_, i) => ({ id: `m${i}`, label: `Model ${i}` }));
    await draw("cursor");
    await open();

    const box = container.querySelector<HTMLInputElement>(".slot__field input");
    expect(box, "a list of 20 was drawn as a row").toBeTruthy();
    // Twelve is where the row stops, and what is past it is said rather than left to be noticed.
    expect(container.querySelectorAll(".slot__starts button")).toHaveLength(12);
    expect(container.querySelector(".slot__note")?.textContent).toContain("8");
  });
});

describe("opening and closing the candidates", () => {
  it("closes on a second press of the row", async () => {
    // The row's own button is inside the box for the outside-press watch, or the press would close
    // the box on the way down and the toggle would open it again on the way up.
    hoisted.switches["claude-code"] = { command: "/model", carries: "named", keeps: null };
    hoisted.models["claude-code"] = [{ id: "sonnet", label: "Sonnet 5" }];
    await draw("claude-code");
    await open();
    expect(container.querySelector(".modelpick")).toBeTruthy();

    const row = container.querySelector<HTMLButtonElement>(".modelrow__now");
    await act(async () => {
      row?.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true }));
      row?.click();
    });
    expect(container.querySelector(".modelpick"), "a second press left it open").toBeNull();
  });

  it("closes on a press anywhere else", async () => {
    hoisted.switches["claude-code"] = { command: "/model", carries: "named", keeps: null };
    await draw("claude-code");
    await open();
    await act(async () => {
      document.body.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true }));
    });
    expect(container.querySelector(".modelpick")).toBeNull();
  });
});

describe("asking the host", () => {
  it("asks for the candidates only once the reader has opened the row", async () => {
    // The first ask starts a login shell and the provider on top of it, so a pane that asked on the
    // way up would pay for one nobody opened (`crate::agent_models`).
    hoisted.switches["claude-code"] = { command: "/model", carries: "named", keeps: null };
    await draw("claude-code");
    expect(hoisted.asked).toEqual(["wake_switch"]);

    await open();
    expect(hoisted.asked).toContain("agent_models");
  });
});
