// @vitest-environment jsdom
// The empty frame: what a terminal opened from it is opened with, and that the frame says nothing
// else. The one thing it must not do is ask before there is a project to ask about.
//
// The host's read is stubbed and everything else runs — the point of the component is entirely in
// which of the shapes it draws and what pressing them does.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { WakeDto } from "../bindings/bindings";
import { EmptySlot } from "./EmptySlot";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const hoisted = vi.hoisted(() => ({
  /** What this machine can start, and what the project has settled on. */
  wake: { candidates: [], offered: [] } as unknown,
  /** Which commands the frame put to the host, in the order it asked. */
  asked: [] as string[],
  /** Set to refuse the read, the way a host that could not answer does. */
  wakeFails: false,
}));

vi.mock("../core/ipc", () => ({
  invoke: vi.fn(async (cmd: string) => {
    hoisted.asked.push(cmd);
    if (cmd === "wake_choices") {
      if (hoisted.wakeFails) throw new Error("the host could not say");
      return hoisted.wake;
    }
    throw new Error(`the frame asked the host for ${cmd}`);
  }),
}));

/** What this machine can start: every id named here is installed and offered, in the order given. */
function startable(ids: string[], settled?: string): WakeDto {
  return {
    candidates: ids.map((id) => ({ id, label: id, command: id, traced: false, installed: true })),
    offered: ids,
    ...(settled === undefined ? {} : { settled }),
  };
}

let container: HTMLDivElement;
let root: Root;
/** What the frame was pressed to open a terminal with, in the order it was pressed — null where the
 *  frame had nothing to say and left the answer to the pane's own side. */
const started: (string | null)[] = [];

beforeEach(() => {
  hoisted.asked = [];
  // A machine with no agent on it, which leaves the shell as the only thing to open with — the one
  // shape where the row of them is not drawn at all.
  hoisted.wake = startable([]);
  hoisted.wakeFails = false;
  started.length = 0;
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

/** Draw it. Whether there is an empty frame on this page at all is the face's decision, not this
 *  component's (`./TerminalFace`). */
async function draw(folder: string | null): Promise<void> {
  await act(async () => {
    root.render(
      createElement(EmptySlot, {
        folders: folder === null ? [] : [folder],
        project: folder === null ? null : 1,
        onOpen: (agent: string | null) => { started.push(agent); },
      }),
    );
  });
}

/** Press the button whose words contain this, and say so where there is none. */
async function press(words: string): Promise<void> {
  const one = buttons().find((b) => b.textContent?.includes(words));
  expect(one, `"${words}" was not pressable`).toBeTruthy();
  await act(async () => { one?.click(); });
}

/** The one that is on, out of the row of things to open with. */
function on(): string | null {
  return container.querySelector(".slot__start--on")?.textContent ?? null;
}

function buttons(): HTMLButtonElement[] {
  return [...container.querySelectorAll("button")];
}

describe("what the empty frame says", () => {
  it("is that there is room here, and nothing else", async () => {
    await draw("/work/here");

    expect(container.querySelector(".slot--empty"), "the plain slot was not drawn").toBeTruthy();
    // One press and no reading of the project: what was left in the middle belongs on the ledger,
    // not on the frame that offers a terminal.
    expect(buttons()).toHaveLength(1);
    expect(hoisted.asked).toEqual(["wake_choices"]);
  });

  it("keeps the way in on a page that has no project yet", async () => {
    await draw(null);

    expect(container.querySelector(".slot--empty")).toBeTruthy();
    expect(buttons().some((b) => b.textContent === "Open a terminal here")).toBe(true);
  });
});

describe("what a terminal opened here is opened with", () => {
  it("is a row of what this machine can start, with the plain shell at the end of it", async () => {
    hoisted.wake = startable(["claude-code", "codex-cli"]);
    await draw("/work/here");

    expect([...container.querySelectorAll(".slot__start")].map((b) => b.textContent))
      .toEqual(["claude-code", "codex-cli", "Plain shell"]);
  });

  it("draws no row where the shell is the only thing there is, and opens on it", async () => {
    await draw("/work/here");

    expect(container.querySelector(".slot__starts"), "a row of one was drawn").toBeFalsy();
    expect(buttons()).toHaveLength(1);

    // Not the first run's "nothing is on": there is nothing to choose between, so the one thing
    // there is, is what the press opens.
    await press("Open a terminal here");
    expect(started).toEqual(["shell"]);
  });

  it("says in words that the row is to be chosen from, and names the row with the same words", async () => {
    hoisted.wake = startable(["claude-code", "codex-cli"]);
    await draw("/work/here");

    const ask = container.querySelector(".slot__ask");
    expect(ask?.textContent, "nothing said what the pills are for").toBe("What does this pane open with?");
    // The row is pointed at the question rather than given a second wording: what is heard and what
    // is read have to be the one sentence.
    expect(container.querySelector(".slot__starts")?.getAttribute("aria-labelledby"))
      .toBe(ask?.getAttribute("id"));
  });

  it("says nothing where there is no row to say it about", async () => {
    await draw("/work/here");

    expect(container.querySelector(".slot__ask"), "a question was put over nothing").toBeFalsy();
  });

  it("comes up on what the host arrived at", async () => {
    hoisted.wake = startable(["claude-code", "codex-cli"], "codex-cli");
    await draw("/work/here");

    expect(on()).toBe("codex-cli");
  });

  // The first run: nobody has ever chosen, and more than one thing can be started. Nothing is on and
  // the button says what to do, rather than being pressable and doing nothing (`AMB-T-3686`).
  it("comes up on nothing at all where nobody has chosen yet", async () => {
    hoisted.wake = startable(["claude-code", "codex-cli"]);
    await draw("/work/here");

    expect(on(), "something was on before anybody chose").toBeNull();
    expect(container.querySelector(".slot__open")?.textContent).toBe("Choose one");
  });

  it("does not open a terminal until one of them is chosen", async () => {
    hoisted.wake = startable(["claude-code", "codex-cli"]);
    await draw("/work/here");

    await press("Choose one");
    expect(started, "a press with nothing chosen opened one anyway").toEqual([]);

    await press("codex-cli");
    expect(container.querySelector(".slot__open")?.textContent).toBe("Open a terminal here");
    await press("Open a terminal here");
    expect(started).toEqual(["codex-cli"]);
  });

  // A frame that never heard back is not the first run: it has nothing to draw a row from and
  // nothing to say about what is on, so it presses with no answer and the pane settles one.
  it("opens with no answer at all where the read did not come back", async () => {
    hoisted.wakeFails = true;
    await draw("/work/here");

    expect(container.querySelector(".slot__starts"), "a row was drawn off a read that failed").toBeFalsy();
    await press("Open a terminal here");
    expect(started).toEqual([null]);
  });

  it("opens with the one that is on, and one press does it", async () => {
    hoisted.wake = startable(["claude-code", "codex-cli"], "codex-cli");
    await draw("/work/here");

    await press("Open a terminal here");

    expect(started).toEqual(["codex-cli"]);
  });

  it("opens with another one without asking anything on the way", async () => {
    hoisted.wake = startable(["claude-code", "codex-cli"], "codex-cli");
    await draw("/work/here");

    await press("claude-code");
    expect(on(), "the press did not move what is on").toBe("claude-code");
    await press("Open a terminal here");

    expect(started).toEqual(["claude-code"]);
  });

  it("opens on the plain shell, which is a choice like the others here", async () => {
    hoisted.wake = startable(["claude-code"]);
    await draw("/work/here");

    await press("Plain shell");
    await press("Open a terminal here");

    expect(started).toEqual(["shell"]);
  });
});
