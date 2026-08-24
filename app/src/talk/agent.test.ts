// @vitest-environment jsdom
// The folder a frame opens in, the three shapes it draws once it has one, and the one rule about when
// the agent may be changed.
//
// Only the three boundaries are stubbed — what the host answers, the folder the person chooses, and
// the terminal itself — so the branching, the remembering, and the row on a closed pane all run for
// real.
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { BoundFolderDto, PtySessionDto, WakeDto } from "../bindings/bindings";
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
  /** What the folder dialog comes back with: a path, null for a cancel, or a refusal to throw. */
  chosen: null as string | null,
  /** Set to refuse the binding, the way a folder something else already owns is refused. */
  refuse: null as Error | null,
  /** How many times the person was taken to the folder dialog. */
  chose: 0,
  /** The folders the project this window is on is bound to, as the host answers. */
  bound: [] as BoundFolderDto[],
  /** Which projects were asked for their folders, in order. */
  asked: [] as number[],
  /** Which project the dialog was told to bind the folder it came back with to. */
  boundTo: [] as number[],
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
vi.mock("../core/mutations", () => ({
  chooseWorkFolder: vi.fn(async () => {
    hoisted.chose += 1;
    if (hoisted.refuse) throw hoisted.refuse;
    return hoisted.chosen;
  }),
  chooseFolderFor: vi.fn(async (project: number) => {
    hoisted.chose += 1;
    hoisted.boundTo.push(project);
    if (hoisted.refuse) throw hoisted.refuse;
    return hoisted.chosen;
  }),
  fetchBoundFolders: vi.fn(async (project: number) => {
    hoisted.asked.push(project);
    return hoisted.bound;
  }),
}));
vi.mock("./terminal", () => ({
  // The one choice that is not a catalogued id, kept where the field it lands in is declared.
  SHELL: "shell",
  mountTerminal: vi.fn(
    async (
      host: HTMLElement,
      on: PaneEvents,
      // What the frame hands a terminal it is *starting*: where, with what, and never taking one up —
      // adopting is settled before the question is put, and a started pane must not take a running
      // terminal off another slot (`./layout`).
      start: { cwd?: string | null; agent?: string | null; adopt?: boolean; session?: string | null },
    ) => {
      hoisted.panes.push(start);
      // What the real one says: the session running here, and **where it runs** — which for a terminal
      // the pane took up is where that one was started, not the folder this pane was handed.
      const took = start.adopt !== false ? hoisted.running[0] : undefined;
      const session = took?.session ?? "session-1";
      on.opened(session, took?.startedAt ?? "2026-01-01T00:00:00Z", took?.folder ?? start.cwd ?? null);
      hoisted.end = () => on.closed(session);
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
const heard = { opened: 0, output: 0, chose: [] as string[], said: 0, closed: 0, named: 0 };
const events: PaneEvents = {
  opened: () => {
    heard.opened += 1;
  },
  output: () => {
    heard.output += 1;
  },
  chose: (folder) => {
    heard.chose.push(folder);
  },
  said: () => {
    heard.said += 1;
  },
  path: () => {},
  closed: () => {
    heard.closed += 1;
  },
  name: () => {
    heard.named += 1;
  },
};

/** Put the frame up in a fresh page, with nothing chosen yet, and hand back its root. `project` is the
 *  one the window is on — null is a machine that has no project for a pane to belong to yet. */
async function put(answer: WakeDto, project: number | null = null): Promise<HTMLElement> {
  hoisted.answers = [answer];
  const root = document.createElement("div");
  document.body.replaceChildren(root);
  await mountAgentFrame(root, "en", events, {}, project);
  return root;
}

/** A folder this project is bound to, as the host lists it. */
function folder(path: string, exists = true): BoundFolderDto {
  return { path, exists, mismatch: null, legacy: false, pointerMissing: false, foreign: null };
}

/** Press the invitation's button and let what it starts run out. */
async function chooseFolder(root: HTMLElement): Promise<void> {
  const choose = buttons(root).find((b) => b.textContent === "Choose a folder");
  expect(choose, "there was no way to choose a folder").toBeTruthy();
  choose?.click();
  await new Promise((r) => setTimeout(r, 0));
}

/** The frame with a folder already chosen — where every question after the first one is asked from. */
async function draw(answer: WakeDto, project: number | null = null): Promise<HTMLElement> {
  hoisted.chosen = "/work/here";
  const root = await put(answer, project);
  await chooseFolder(root);
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
  hoisted.chosen = null;
  hoisted.refuse = null;
  hoisted.chose = 0;
  hoisted.bound = [];
  hoisted.asked = [];
  hoisted.boundTo = [];
  heard.opened = heard.said = heard.closed = heard.named = 0;
  heard.chose = [];
});

describe("a frame with no folder asks for one, and asks for nothing else", () => {
  it("puts the invitation and nothing else — the host is not asked what runs where", async () => {
    const root = await put(wake({ offered: ["claude-code"], settled: "claude-code" }));

    expect(root.textContent).toContain("Choose the folder you show the AI");
    expect(buttons(root).map((b) => b.textContent)).toEqual(["Choose a folder"]);
    expect(hoisted.sent.map(([name]) => name)).not.toContain("wake_probe");
    expect(hoisted.panes).toEqual([]);
  });

  it("opens the pane in the folder chosen, in one press and with nothing to submit", async () => {
    hoisted.chosen = "/work/here";
    const root = await put(wake({ offered: ["claude-code"], settled: "claude-code" }));
    await chooseFolder(root);

    expect(hoisted.chose, "the person was taken to the dialog more than once").toBe(1);
    // Said as soon as it is answered, not when a terminal comes up: the slots beside this one open in
    // the same folder, and on a machine with nothing startable no terminal ever follows.
    expect(heard.chose, "the window was not told where this frame settled").toEqual(["/work/here"]);
    expect(hoisted.sent).toContainEqual(["wake_probe", { folder: "/work/here", project: null }]);
    expect(hoisted.panes).toEqual([{ adopt: false, cwd: "/work/here", agent: "claude-code" }]);
  });

  it("leaves the invitation standing when the dialog is cancelled", async () => {
    hoisted.chosen = null;
    const root = await put(wake({ settled: "claude-code" }));
    await chooseFolder(root);

    const again = buttons(root).find((b) => b.textContent === "Choose a folder");
    expect(again, "cancelling took the invitation away").toBeTruthy();
    expect(again?.disabled, "cancelling left the button unpressable").toBe(false);
    expect(hoisted.sent.map(([name]) => name)).not.toContain("wake_probe");
  });

  it("keeps the invitation, with the reason under it, when the folder cannot be taken", async () => {
    hoisted.chosen = "/work/here";
    hoisted.refuse = new Error("that folder belongs to something else");
    const root = await put(wake({ settled: "claude-code" }));
    await chooseFolder(root);

    expect(root.textContent).toContain("that folder belongs to something else");
    expect(buttons(root).find((b) => b.textContent === "Choose a folder")).toBeTruthy();
    expect(hoisted.panes).toEqual([]);
  });

  it("asks nothing of a frame that adopts a terminal, and opens where that one runs when it ends", async () => {
    hoisted.running = [{ session: "session-1", startedAt: "2026-01-01T00:00:00Z", folder: "/work/adopted" }];
    const root = await put(wake({ offered: ["claude-code"], settled: "claude-code" }));

    expect(hoisted.chose, "a running terminal was asked about").toBe(0);
    // Taken up rather than started, so this pane is handed no folder to start in — it is the session
    // that says where it runs, and saying so is what the frame learns its folder from.
    expect(hoisted.panes).toEqual([{ cwd: null, agent: null }]);

    // The program ends and the frame is asked to open again: it knows where, because the session it
    // took up said so.
    hoisted.running = [];
    hoisted.end?.();
    buttons(root).find((b) => b.textContent === "Open")?.click();
    await new Promise((r) => setTimeout(r, 0));

    expect(hoisted.chose, "the folder was asked for a second time").toBe(0);
    expect(hoisted.sent).toContainEqual(["wake_probe", { folder: "/work/adopted", project: null }]);
    expect(hoisted.panes[hoisted.panes.length - 1]).toEqual({ adopt: false, cwd: "/work/here", agent: "claude-code" });
  });
});

// A frame is told which project it is on by the face it sits on (`../shell/TerminalFace`), and from
// there it keeps the rule that face keeps: a pane works in one of its project's folders and no other.
describe("a window told which project it is on asks among that project's folders and no others", () => {
  it("offers the project's folders, and never the machine's", async () => {
    hoisted.bound = [folder("/work/site"), folder("/work/api")];
    const root = await put(wake({ settled: "claude-code" }), 7);

    expect(hoisted.asked, "this project's folders were not the ones read").toEqual([7]);
    expect(root.textContent).toContain("Which folder does this pane work in?");
    expect(buttons(root).map((b) => b.textContent)).toEqual(["/work/site", "/work/api"]);
    expect(hoisted.chose, "the OS picker was opened on a project that has folders").toBe(0);
  });

  it("opens the pane in the folder pressed, with nothing to submit after it", async () => {
    hoisted.bound = [folder("/work/site"), folder("/work/api")];
    const root = await put(wake({ offered: ["claude-code"], settled: "claude-code" }), 7);
    buttons(root).find((b) => b.textContent === "/work/api")?.click();
    await new Promise((r) => setTimeout(r, 0));

    expect(heard.chose, "the window was not told where this frame settled").toEqual(["/work/api"]);
    expect(hoisted.sent).toContainEqual(["wake_probe", { folder: "/work/api", project: 7 }]);
    expect(hoisted.panes).toEqual([{ adopt: false, cwd: "/work/here", agent: "claude-code" }]);
  });

  it("leaves a folder that is not there off the list — nothing can be started in one", async () => {
    hoisted.bound = [folder("/work/site"), folder("/work/moved", false)];
    const root = await put(wake({ settled: "claude-code" }), 7);

    expect(buttons(root).map((b) => b.textContent)).not.toContain("/work/moved");
  });

  it("asks nothing where the project is bound to one folder — the pane opens there", async () => {
    hoisted.bound = [folder("/work/site")];
    const root = await put(wake({ offered: ["claude-code"], settled: "claude-code" }), 7);

    expect(root.textContent).not.toContain("Which folder does this pane work in?");
    expect(heard.chose, "the one folder was not taken as the answer").toEqual(["/work/site"]);
    expect(hoisted.panes).toEqual([{ adopt: false, cwd: "/work/here", agent: "claude-code" }]);
  });

  it("binds a project's first folder to that project, not to one the folder's name would raise", async () => {
    hoisted.chosen = "/work/first";
    const root = await put(wake({ settled: "claude-code" }), 7);

    expect(root.textContent).toContain("Choose the folder you show the AI");
    await chooseFolder(root);

    expect(hoisted.boundTo, "the folder was bound somewhere other than this project").toEqual([7]);
    expect(heard.chose).toEqual(["/work/first"]);
  });
});

describe("the frame draws what the host settled", () => {
  it("opens the pane without asking when one agent answers", async () => {
    const root = await draw(wake({ offered: ["claude-code"], settled: "claude-code" }));

    expect(hoisted.panes).toEqual([{ cwd: "/work/here", agent: "claude-code", adopt: false }]);
    expect(buttons(root)).toEqual([]);
    expect(hoisted.sent.map(([name]) => name)).not.toContain("wake_remember");
  });

  it("offers the choice when several answer, and keeps the one that is picked", async () => {
    const root = await draw(wake(), 7);
    expect(hoisted.panes).toEqual([]);

    const choice = buttons(root).find((b) => b.textContent === "Codex CLI");
    expect(choice, "the offer did not name the agents").toBeTruthy();
    choice?.click();
    await Promise.resolve();

    // Kept against the project rather than the folder: one project can bind several folders, and an
    // answer written per folder is one project answering the same question several ways.
    expect(hoisted.sent).toContainEqual([
      "wake_remember",
      { project: 7, agent: "codex-cli" },
    ]);
    expect(hoisted.panes).toEqual([{ cwd: "/work/here", agent: "codex-cli", adopt: false }]);
  });

  it("keeps nothing where the frame belongs to no project — there is nothing to keep it against", async () => {
    const root = await draw(wake());

    buttons(root).find((b) => b.textContent === "Codex CLI")?.click();
    await Promise.resolve();

    expect(hoisted.sent.map(([name]) => name)).not.toContain("wake_remember");
    expect(hoisted.panes).toEqual([{ cwd: "/work/here", agent: "codex-cli", adopt: false }]);
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

    expect(hoisted.panes).toEqual([{ cwd: "/work/here", agent: "claude-code", adopt: false }]);
  });
});

describe("the agent can be changed only on a frame that has closed", () => {
  it("puts nothing to press while the pane is running", async () => {
    const root = await draw(wake({ offered: ["claude-code"], settled: "claude-code" }));
    expect(buttons(root)).toEqual([]);
    expect(root.querySelector("select")).toBeNull();
  });

  it("offers the agent and open once the program has ended", async () => {
    const root = await draw(wake({ settled: "claude-code" }), 7);
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

    // The pane opens on what was picked, and **the project's answer stays where it was**: reaching
    // for another tool on one frame is that frame, and what the project opens with is changed on the
    // project's own settings (`AMB-T-3667`).
    expect(hoisted.sent.map(([name]) => name)).not.toContain("wake_remember");
    expect(hoisted.panes[hoisted.panes.length - 1]).toEqual({
      cwd: "/work/here",
      agent: "codex-cli",
      adopt: false,
    });
  });

  it("settles the project's answer from the row where it had settled on nothing", async () => {
    const root = await draw(wake(), 7);
    buttons(root).find((b) => b.textContent === "Claude Code")?.click();
    await Promise.resolve();
    hoisted.sent.length = 0;
    hoisted.answers = [wake()];

    hoisted.end?.();
    const choose = root.querySelector("select");
    if (choose) {
      choose.value = "codex-cli";
      choose.dispatchEvent(new Event("change"));
    }
    buttons(root).find((b) => b.textContent === "Open")?.click();
    await Promise.resolve();

    expect(hoisted.sent).toContainEqual(["wake_remember", { project: 7, agent: "codex-cli" }]);
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

// The folder's own prompt, with nothing started at it. It stands wherever the frame puts a choice —
// the offer, the notice, the closed frame's row — and nowhere else: a folder that settled on one
// agent still opens on it unasked, because the shell is what a reader reaches for once the pane is
// there rather than a question they answer on the way in.
describe("the shell stands beside the agents, and answers nothing about the folder", () => {
  it("puts the shell under the offer, and opens it without writing anything down", async () => {
    const root = await draw(wake());

    const shell = buttons(root).find((b) => b.textContent === "Plain shell");
    expect(shell, "the offer left no way to open a shell").toBeTruthy();
    shell?.click();
    await Promise.resolve();

    // Not remembered: "which agent do you work with in this folder" is not a question a shell
    // answers, and an answer written down here would be one the offer never asked for.
    expect(hoisted.sent.map(([name]) => name)).not.toContain("wake_remember");
    expect(hoisted.panes).toEqual([{ cwd: "/work/here", agent: null, adopt: false }]);
  });

  it("offers the shell where nothing is startable, which is the only terminal such a machine has", async () => {
    const root = await draw(wake({ offered: [] }));

    const shell = buttons(root).find((b) => b.textContent === "Plain shell");
    expect(shell, "a machine with no agent on it was left with no terminal at all").toBeTruthy();
    shell?.click();
    await Promise.resolve();

    expect(hoisted.panes).toEqual([{ cwd: "/work/here", agent: null, adopt: false }]);
  });

  it("draws the row for a folder that settled on one agent, since the shell is the other choice", async () => {
    const root = await draw(wake({ offered: ["claude-code"], settled: "claude-code" }));
    hoisted.end?.();

    const choose = root.querySelector("select");
    expect(choose, "the row with one agent on it had nowhere to choose the shell").toBeTruthy();
    expect([...(choose?.options ?? [])].map((one) => one.textContent)).toEqual([
      "Claude Code",
      "Plain shell",
    ]);
  });

  it("starts nothing, and writes nothing down, when the row opens the shell", async () => {
    const root = await draw(wake({ settled: "claude-code" }));
    hoisted.end?.();
    const choose = root.querySelector("select");
    if (choose) {
      choose.value = "shell";
      choose.dispatchEvent(new Event("change"));
    }
    hoisted.sent = [];
    buttons(root).find((b) => b.textContent === "Open")?.click();
    await Promise.resolve();

    expect(hoisted.sent.map(([name]) => name)).not.toContain("wake_remember");
    expect(hoisted.panes[hoisted.panes.length - 1]).toEqual({
      cwd: "/work/here",
      agent: null,
      adopt: false,
    });
  });

  it("keeps the shell as what the next pane opens with, once one has been opened", async () => {
    const root = await draw(wake({ offered: ["claude-code"], settled: "claude-code" }));
    hoisted.end?.();
    const choose = root.querySelector("select");
    if (choose) {
      choose.value = "shell";
      choose.dispatchEvent(new Event("change"));
    }
    buttons(root).find((b) => b.textContent === "Open")?.click();
    await new Promise((r) => setTimeout(r, 0));

    hoisted.end?.();
    expect(root.querySelector("select")?.value, "the row fell back to the agent").toBe("shell");
  });
});
