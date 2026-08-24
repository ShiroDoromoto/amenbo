// @vitest-environment jsdom
// The empty frame: what it says about work nothing is doing any more, and what a terminal opened from
// it is opened with. Two things it must not do: ask before there is a project to ask about, and act
// on the answer.
//
// The host's reads are stubbed and everything else runs — the point of the component is entirely in
// which of the shapes it draws and what pressing them does.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AdriftDto, AdriftRowDto, WakeDto } from "../bindings/bindings";
import { RefNavProvider } from "../core/refNav";
import { AdriftSlot } from "./AdriftSlot";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const hoisted = vi.hoisted(() => ({
  /** What the host answers with, and which folders it was asked about. */
  adrift: { tasks: [], decisions: [] } as unknown,
  asked: [] as string[],
  /** What this machine can start, and what the project has settled on. */
  wake: { candidates: [], offered: [] } as unknown,
}));

vi.mock("../core/mutations", () => ({
  fetchAdrift: vi.fn(async (folder: string) => {
    hoisted.asked.push(folder);
    return hoisted.adrift;
  }),
}));

vi.mock("../core/ipc", () => ({
  invoke: vi.fn(async (cmd: string) => {
    if (cmd === "wake_choices") return hoisted.wake;
    throw new Error(`the frame asked the host for ${cmd}`);
  }),
}));

/** A row, as the host hands one over. */
function row(ref: string, id: number, title: string): AdriftRowDto {
  return { id, ref, title };
}

/** What the host answers with, with the half a test does not care about empty. */
function left(over: Partial<AdriftDto>): AdriftDto {
  return { tasks: [], decisions: [], ...over };
}

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
const opened: string[] = [];
const ledger: number[] = [];
/** What the frame was pressed to open a terminal with, in the order it was pressed. */
const started: string[] = [];

beforeEach(() => {
  hoisted.adrift = left({});
  hoisted.asked = [];
  // A machine with no agent on it, which leaves the shell as the only thing to open with — the one
  // shape where the row of them is not drawn at all.
  hoisted.wake = startable([]);
  opened.length = 0;
  ledger.length = 0;
  started.length = 0;
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

/** Draw it, with the way to the ledger and the way to a task both counted. Whether there is an empty
 *  frame on this page at all is the face's decision, not this component's (`./TerminalFace`). */
async function draw(folder: string | null): Promise<void> {
  await act(async () => {
    root.render(
      createElement(
        RefNavProvider,
        {
          value: {
            selectTask: (id: number) => { opened.push(`task ${id}`); },
            selectDecision: (id: number | null) => { opened.push(`decision ${id}`); },
          },
          children: createElement(AdriftSlot, {
            folders: folder === null ? [] : [folder],
            project: folder === null ? null : 1,
            onOpen: (agent: string) => { started.push(agent); },
            onOpenLedger: () => ledger.push(1),
          }),
        },
      ),
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

describe("a face with nothing to ask about", () => {
  it("is the plain empty frame", async () => {
    await draw("/work/here");

    expect(buttons()).toHaveLength(1);
    expect(container.querySelector(".slot--empty"), "the plain slot was not drawn").toBeTruthy();
  });

  it("asks the host nothing about a page that has no folder yet", async () => {
    await draw(null);

    expect(hoisted.asked, "a page with no project was asked about anyway").toEqual([]);
    expect(container.querySelector(".slot--empty")).toBeTruthy();
  });
});

describe("a face with something left in the middle", () => {
  it("puts one question over both kinds, and keeps the way to open a terminal", async () => {
    hoisted.adrift = left({
      tasks: [row("AMB-T-11", 11, "the migration")],
      decisions: [row("AMB-D-21", 21, "which of the two roads")],
    });
    await draw("/work/here");

    expect(hoisted.asked).toEqual(["/work/here"]);
    // One sentence, not one per kind: it is one question, and the ref on each row says which kind it
    // is and so what pressing it opens.
    expect(container.querySelectorAll(".adrift__ask")).toHaveLength(1);
    expect(container.textContent).toContain("Carry on with it?");
    expect(container.textContent).toContain("the migration");
    expect(container.textContent).toContain("which of the two roads");
    // The way in is still there: the face is somewhere to start a terminal whether or not there
    // is anything to be asked about.
    expect(buttons().some((b) => b.textContent === "Open a terminal here")).toBe(true);
  });

  it("opens a task on the task face and a decision on the decision face, and moves neither", async () => {
    hoisted.adrift = left({
      tasks: [row("AMB-T-11", 11, "the migration")],
      decisions: [row("AMB-D-21", 21, "which of the two roads")],
    });
    await draw("/work/here");

    await press("the migration");
    await press("which of the two roads");

    // The ledger first each time — a press that selected without switching would land on a face the
    // reader cannot see — and each kind on the face that reads it.
    expect(ledger, "the ledger was not brought up").toEqual([1, 1]);
    expect(opened).toEqual(["task 11", "decision 21"]);
  });

  it("asks about a decision alone, where that is all there is", async () => {
    hoisted.adrift = left({ decisions: [row("AMB-D-21", 21, "which of the two roads")] });
    await draw("/work/here");

    expect(container.querySelector(".slot--adrift"), "the plain slot was drawn").toBeTruthy();
    expect(container.textContent).toContain("which of the two roads");
  });
});

describe("what a terminal opened here is opened with", () => {
  it("is a row of what this machine can start, with the plain shell at the end of it", async () => {
    hoisted.wake = startable(["claude-code", "codex-cli"]);
    await draw("/work/here");

    expect([...container.querySelectorAll(".slot__start")].map((b) => b.textContent))
      .toEqual(["claude-code", "codex-cli", "Plain shell"]);
  });

  it("draws no row where the shell is the only thing there is", async () => {
    await draw("/work/here");

    expect(container.querySelector(".slot__starts"), "a row of one was drawn").toBeFalsy();
    expect(buttons()).toHaveLength(1);
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

  it("comes up on the project's own answer", async () => {
    hoisted.wake = startable(["claude-code", "codex-cli"], "codex-cli");
    await draw("/work/here");

    expect(on()).toBe("codex-cli");
  });

  it("comes up on the first of them where the project has settled on nothing", async () => {
    hoisted.wake = startable(["claude-code", "codex-cli"]);
    await draw("/work/here");

    expect(on()).toBe("claude-code");
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

  it("puts the row on the frame that is asking about work left in the middle too", async () => {
    hoisted.wake = startable(["claude-code", "codex-cli"], "codex-cli");
    hoisted.adrift = left({ tasks: [row("AMB-T-11", 11, "the migration")] });
    await draw("/work/here");

    expect(on()).toBe("codex-cli");
    await press("Open a terminal here");
    expect(started).toEqual(["codex-cli"]);
  });
});
