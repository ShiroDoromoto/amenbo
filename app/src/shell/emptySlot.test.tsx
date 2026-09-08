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
  /** What the frame handed the host along with each command it asked, in the same order. */
  args: [] as unknown[],
  /** What the host answers the next read with, where registering one is meant to change the row. */
  wakeAfter: null as unknown,
  /** Set to refuse a registration, the way a host with an empty half does. */
  keepFails: false,
  /** Set to leave the read out for good, the way a host still starting a login shell does. */
  wakeHangs: false,
  /** What `wake_rescan` answers: whether the machine could be reached this time. */
  reached: true,
  /** What the frame asked to be told when this person's answer changes — called to say it has. */
  chosen: [] as (() => void)[],
  /** What each agent's own command answers when it is asked which models it can be started on
      (`AMB-D-865`). An agent not named here answers with nothing, the way one with no list door
      does. */
  models: {} as Record<string, { id: string; label: string }[]>,
  /** What this device already remembers for each agent: the model it comes up on, what was chosen
      for it before, and the flag its command takes a model behind. */
  kept: {} as Record<string, { chosen: { id: string; label: string } | null; history: { id: string; label: string }[]; flag: string | null }>,
}));

// Asking this machine again is a Tauri call and is gated on being inside Tauri, which a jsdom run is
// not — so the pair around the read is stubbed rather than the transport under it. What the frame
// owes is the order: ask the machine again, then put the question again (`AMB-D-792`).
vi.mock("./wake", () => ({
  onAgentsInstalled: async () => () => {},
  // Kept rather than dropped: what this one says is that somebody answered on another frame, and a
  // test of the frame beside a pane has to be able to say it.
  onAgentChosen: async (take: () => void) => {
    hoisted.chosen.push(take);
    return () => { hoisted.chosen = hoisted.chosen.filter((one) => one !== take); };
  },
  wakeRescan: async () => {
    hoisted.asked.push("wake_rescan");
    if (hoisted.wakeAfter !== null) hoisted.wake = hoisted.wakeAfter;
    return hoisted.reached;
  },
}));

vi.mock("../core/ipc", () => ({
  invoke: vi.fn(async (cmd: string, args?: unknown) => {
    hoisted.asked.push(cmd);
    hoisted.args.push(args);
    if (cmd === "wake_choices") {
      if (hoisted.wakeHangs) return await new Promise(() => {});
      if (hoisted.wakeFails) throw new Error("the host could not say");
      return hoisted.wake;
    }
    if (cmd === "wake_register" || cmd === "wake_amend" || cmd === "wake_unregister") {
      if (hoisted.keepFails) throw new Error("that command could not be registered");
      if (hoisted.wakeAfter !== null) hoisted.wake = hoisted.wakeAfter;
      return cmd === "wake_register" ? "custom:1" : undefined;
    }
    if (cmd === "agent_models") {
      return hoisted.models[(args as { agent: string }).agent] ?? [];
    }
    if (cmd === "wake_model") {
      return hoisted.kept[(args as { agent: string }).agent]
        ?? { chosen: null, history: [], flag: "--model" };
    }
    if (cmd === "wake_chose_model" || cmd === "wake_forget_model") return undefined;
    throw new Error(`the frame asked the host for ${cmd}`);
  }),
}));

/** What this machine can start: every id named here is installed and offered, in the order given. */
function startable(ids: string[], settled?: string): WakeDto {
  return {
    candidates: ids.map((id) => ({ id, label: id, command: id, traced: false, installed: true })),
    offered: ids,
    reach: "answered",
    ...(settled === undefined ? {} : { settled }),
  };
}

/** A machine the catalogue is wider than: `has` is installed, `lacks` is offered and not installed —
 *  the row every real machine draws (`AMB-D-792`). Catalog order is the order given, both together. */
function partly(has: string[], lacks: string[], settled?: string): WakeDto {
  const row = [
    ...has.map((id) => ({ id, label: id, command: id, traced: false, installed: true })),
    ...lacks.map((id) => ({ id, label: id, command: id, traced: false, installed: false })),
  ];
  return {
    candidates: row,
    offered: row.map((one) => one.id),
    reach: "answered",
    ...(settled === undefined ? {} : { settled }),
  };
}

/** A machine with a command the reader registered on it (`AMB-D-794`): `own` is `[label, line]`, and
 *  `installed` says whether this machine can start its first word. Catalog rows come first, as the
 *  host answers them. */
function withOwn(
  has: string[],
  own: [string, string][],
  installed = true,
  settled?: string,
): WakeDto {
  const row = [
    ...has.map((id) => ({ id, label: id, command: id, traced: false, installed: true })),
    ...own.map(([label, line], i) => ({
      id: `custom:${i + 1}`,
      label,
      command: line.split(" ")[0] ?? "",
      line,
      traced: false,
      installed,
    })),
  ];
  return {
    candidates: row,
    offered: row.map((one) => one.id),
    reach: "answered",
    ...(settled === undefined ? {} : { settled }),
  };
}

/** A machine that could not be asked at all (`AMB-D-792`). Every row says it is not installed —
 *  which is what a machine with nothing on it says too, and the reason `reach` has to travel. */
function unreached(ids: string[]): WakeDto {
  const row = ids.map((id) => ({ id, label: id, command: id, traced: false, installed: false }));
  return { candidates: row, offered: ids, reach: "unreachable" };
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
  hoisted.args = [];
  hoisted.wakeAfter = null;
  hoisted.keepFails = false;
  hoisted.wakeHangs = false;
  hoisted.reached = true;
  hoisted.chosen = [];
  hoisted.models = {};
  hoisted.kept = {};
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

/** Type into the field whose label reads this. React listens for the native input event, so the
 *  value goes in through the setter React did not replace. */
async function type(label: string, text: string): Promise<void> {
  const field = [...container.querySelectorAll(".slot__field")].find(
    (one) => one.querySelector("span")?.textContent === label,
  );
  const input = field?.querySelector("input");
  expect(input, `there is no "${label}" to type into`).toBeTruthy();
  await act(async () => {
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set;
    setter?.call(input, text);
    input?.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

/** The registered commands as the list under the row draws them: name and the line it runs. */
function listed(): [string, string][] {
  return [...container.querySelectorAll(".slot__ownrow")].map((row) => [
    row.querySelector(".slot__ownname")?.textContent ?? "",
    row.querySelector(".slot__ownline")?.textContent ?? "",
  ]);
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

  // And the first run ends for every frame at once. This one is the frame standing beside the pane
  // somebody just opened: it read the ranks while nobody had answered, and the press that opened
  // that pane kept an answer (`wake_chose`). Being told is what ends the asking here — without it
  // the reader is asked a second time for what they answered a press ago (`AMB-T-4357`).
  it("stops asking once the answer was given on the frame beside it", async () => {
    hoisted.wake = startable(["claude-code", "codex-cli"]);
    await draw("/work/here");
    expect(on()).toBeNull();

    hoisted.wake = startable(["claude-code", "codex-cli"], "codex-cli");
    await act(async () => { for (const one of hoisted.chosen) one(); });

    expect(on(), "the frame asked again for what was answered beside it").toBe("codex-cli");
    expect(container.querySelector(".slot__open")?.textContent).toBe("Open a terminal here");
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

  it("draws what this machine has not got as well, folded away and out of the group", async () => {
    hoisted.wake = partly(["claude-code"], ["codex-cli", "cursor"]);
    await draw("/work/here");

    // Folded: the row is what works here, and the shell after it.
    expect([...container.querySelectorAll(".slot__start")].map((b) => b.textContent))
      .toEqual(["claude-code", "Plain shell"]);
    // The press that unfolds them says how many there are, and is not one of the choices — a button
    // inside the group would be read as a fourth thing to open with.
    const more = buttons().find((b) => b.textContent === "Not installed (2)");
    expect(more, "nothing said the other two were there").toBeTruthy();
    expect(more?.closest(".slot__starts"), "the press was put inside the group").toBeNull();
    expect(more?.getAttribute("aria-expanded")).toBe("false");

    await press("Not installed (2)");
    expect([...container.querySelectorAll(".slot__start")].map((b) => b.textContent))
      .toEqual(["claude-code", "Plain shell", "codex-cli", "cursor"]);
    expect(
      buttons().find((b) => b.textContent === "Not installed (2)")?.getAttribute("aria-expanded"),
    ).toBe("true");
  });

  it("leaves the ones it has not got in the group, and does not open a terminal on one", async () => {
    hoisted.wake = partly(["claude-code"], ["codex-cli"], "claude-code");
    await draw("/work/here");
    await press("Not installed (1)");

    const missing = [...container.querySelectorAll(".slot__start--missing")];
    expect(missing.map((b) => b.textContent), "the row it has not got was drawn elsewhere")
      .toEqual(["codex-cli"]);
    // In the group and said to be unreachable: greying is not something a screen reader reports.
    expect(missing[0]?.closest(".slot__starts"), "it was taken out of the group").toBeTruthy();
    expect(missing[0]?.getAttribute("role")).toBe("radio");
    expect(missing[0]?.getAttribute("aria-disabled")).toBe("true");

    // A press on it changes nothing, and the terminal still opens on what was on.
    await press("codex-cli");
    expect(on(), "a press moved what is on to something this machine cannot start").toBe("claude-code");
    await press("Open a terminal here");
    expect(started).toEqual(["claude-code"]);
  });

  it("draws the row on a machine with nothing installed, and opens on the shell", async () => {
    hoisted.wake = partly([], ["claude-code", "codex-cli"]);
    await draw("/work/here");

    // The shell is the whole of what can be started, so it is on — and the row is still drawn, which
    // is what tells a reader with no agent installed that there is something to install.
    expect(on()).toBe("Plain shell");
    expect(buttons().some((b) => b.textContent === "Not installed (2)")).toBe(true);
    await press("Open a terminal here");
    expect(started).toEqual(["shell"]);
  });

  it("opens on the plain shell, which is a choice like the others here", async () => {
    hoisted.wake = startable(["claude-code"]);
    await draw("/work/here");

    await press("Plain shell");
    await press("Open a terminal here");

    expect(started).toEqual(["shell"]);
  });
});

describe("what is known about the row", () => {
  it("says it is still asking rather than drawing a row nobody has answered for", async () => {
    hoisted.wakeHangs = true;
    await draw("/work/here");

    expect(container.querySelector(".slot__note")?.textContent)
      .toBe("Checking what this machine can start…");
    // Nothing to press while it is unknown which of them can be started: a pill drawn now would
    // have to be drawn usable or missing, and both are guesses.
    expect(container.querySelectorAll("button.slot__start")).toHaveLength(0);
  });

  it("says the machine could not be asked, and never that nothing is installed", async () => {
    hoisted.wake = unreached(["claude-code", "codex-cli"]);
    await draw("/work/here");

    expect(container.querySelector(".slot__note")?.textContent)
      .toBe("Amenbo could not check what this machine can start.");
    // The whole catalogue is "not installed" in this state, so the fold that says how many is the
    // one thing that must not be drawn: it would report a number nobody measured.
    expect(buttons().some((b) => b.textContent?.includes("Not installed"))).toBe(false);
    expect(container.querySelectorAll(".slot__start--missing")).toHaveLength(0);
  });

  it("asks the machine again when the reader presses, and draws what it then says", async () => {
    hoisted.wake = unreached(["claude-code", "codex-cli"]);
    hoisted.wakeAfter = startable(["claude-code", "codex-cli"], "claude-code");
    await draw("/work/here");

    await press("Check again");

    expect(hoisted.asked).toContain("wake_rescan");
    expect(container.querySelector(".slot__note")).toBeNull();
    expect(on()).toBe("claude-code");
  });
});

describe("a command the reader registered", () => {
  it("stands among the choices, after the catalogue and before the shell", async () => {
    hoisted.wake = withOwn(["claude-code"], [["Mine", "mine --model big"]]);
    await draw("/work/here");

    expect([...container.querySelectorAll(".slot__start")].map((b) => b.textContent))
      .toEqual(["claude-code", "Mine", "Plain shell"]);
    // And the line it runs is readable without pressing anything: it goes to a terminal as written,
    // so a reader who cannot read it cannot judge it.
    expect(listed()).toEqual([["Mine", "mine --model big"]]);
  });

  it("is drawn last among the ones this machine has not got, right above the form", async () => {
    hoisted.wake = {
      candidates: [
        { id: "claude-code", label: "claude-code", command: "claude", traced: false, installed: true },
        { id: "codex-cli", label: "codex-cli", command: "codex", traced: false, installed: false },
        { id: "custom:1", label: "Mine", command: "mine", line: "mine --model big", traced: false, installed: false },
      ],
      offered: ["claude-code", "codex-cli", "custom:1"],
    };
    await draw("/work/here");
    await press("Not installed (2)");

    expect([...container.querySelectorAll(".slot__start--missing")].map((b) => b.textContent))
      .toEqual(["codex-cli", "Mine"]);
    // Unpressable like any other row this machine has not got: its first word is what was looked
    // for, and it was not found.
    await press("claude-code");
    await press("Mine");
    expect(on(), "a press moved what is on to a line this machine cannot start").toBe("claude-code");
  });

  it("is registered from the frame, and the row is read again once it is", async () => {
    hoisted.wake = startable(["claude-code"]);
    hoisted.wakeAfter = withOwn(["claude-code"], [["Mine", "mine --model big"]]);
    await draw("/work/here");

    await press("Register a command");
    await type("Name", "Mine");
    await type("Command line", "mine --model big");
    // Said before it is saved: this is what will run, and Amenbo composes none of it.
    expect(container.querySelector(".slot__runs")?.textContent)
      .toContain("mine --model big");

    await press("Save");

    expect(hoisted.asked).toEqual(["wake_choices", "wake_register", "wake_choices"]);
    expect(hoisted.args[1]).toEqual({ label: "Mine", line: "mine --model big" });
    expect([...container.querySelectorAll(".slot__start")].map((b) => b.textContent))
      .toEqual(["claude-code", "Mine", "Plain shell"]);
  });

  it("cannot be saved with a half missing", async () => {
    hoisted.wake = startable(["claude-code"]);
    await draw("/work/here");

    await press("Register a command");
    const save = () => buttons().find((b) => b.textContent === "Save");
    expect(save()?.disabled, "an empty form could be saved").toBe(true);
    await type("Name", "Mine");
    expect(save()?.disabled, "a name with no line could be saved").toBe(true);
    await type("Command line", "mine");
    expect(save()?.disabled).toBe(false);
  });

  it("keeps its id when it is corrected, so a pinned answer survives a typo", async () => {
    hoisted.wake = withOwn(["claude-code"], [["Mine", "mien"]]);
    await draw("/work/here");

    await press("Edit");
    await type("Command line", "mine");
    await press("Save");

    expect(hoisted.asked).toEqual(["wake_choices", "wake_amend", "wake_choices"]);
    expect(hoisted.args[1]).toEqual({ id: "custom:1", label: "Mine", line: "mine" });
  });

  it("is dropped from the frame, and what was on moves off it", async () => {
    hoisted.wake = withOwn(["claude-code"], [["Mine", "mine"]]);
    hoisted.wakeAfter = startable(["claude-code"], "claude-code");
    await draw("/work/here");

    await press("Mine");
    expect(on()).toBe("Mine");
    await press("Remove");

    // The two model reads at the end are the row moving onto a catalogued agent: what it can be
    // started on is asked of that agent, and what was chosen for it before is read off this device.
    expect(hoisted.asked).toEqual([
      "wake_choices", "wake_unregister", "wake_choices", "wake_model", "agent_models",
    ]);
    expect(hoisted.args[1]).toEqual({ id: "custom:1" });
    expect(listed(), "the row it was drawn in stayed behind").toEqual([]);
    // The frame is not holding an id it can no longer start: the host's own answer takes over.
    await press("Open a terminal here");
    expect(started).toEqual(["claude-code"]);
  });

  it("says why a registration did not land, without closing the form", async () => {
    hoisted.wake = startable(["claude-code"]);
    hoisted.keepFails = true;
    await draw("/work/here");

    await press("Register a command");
    await type("Name", "Mine");
    await type("Command line", "mine");
    await press("Save");

    expect(container.querySelector(".slot__failed")).toBeTruthy();
    // Still open, with what was typed still in it: the reader has one thing to fix, not two.
    expect(buttons().some((b) => b.textContent === "Save"), "the form was closed on a failure")
      .toBe(true);
  });
});

describe("which model the agent on the row starts on", () => {
  /** The models drawn under the row, in the order they stand — the first is always the default. */
  function offered(): string[] {
    const rows = [...container.querySelectorAll(".slot__pick")];
    const models = rows[rows.length - 1];
    return [...(models?.querySelectorAll(".slot__start") ?? [])].map((one) => one.textContent ?? "");
  }

  /** Which model is on, out of that row — read off the model block and not the row of agents above
   *  it, both being drawn with the same pill. */
  function onModel(): string | null {
    const rows = [...container.querySelectorAll(".slot__pick")];
    return rows[rows.length - 1]?.querySelector(".slot__start--on")?.textContent ?? null;
  }

  /** The line the frame says will run. */
  function runs(): string | null {
    return container.querySelector(".slot__runs code")?.textContent ?? null;
  }

  it("is the agent's own default until somebody says otherwise, and nothing is written down",
    async () => {
      hoisted.wake = startable(["claude-code"], "claude-code");
      hoisted.models["claude-code"] = [{ id: "opus", label: "Opus" }, { id: "sonnet", label: "Sonnet" }];
      await draw("/work/here");

      // The default stands first, and it is what is on: a person who has never been asked is
      // already in this state, and the row says so rather than pre-selecting a model for them.
      expect(offered()).toEqual(["Its own default", "Opus", "Sonnet"]);
      expect(runs(), "a default carries no flag at all").toBe("claude-code");

      await press("Open a terminal here");
      expect(hoisted.asked).not.toContain("wake_chose_model");
      expect(hoisted.asked).not.toContain("wake_forget_model");
      expect(started).toEqual(["claude-code"]);
    });

  it("is kept against the agent when the pane opens, and the line says so first", async () => {
    hoisted.wake = startable(["claude-code"], "claude-code");
    hoisted.models["claude-code"] = [{ id: "opus", label: "Opus" }];
    await draw("/work/here");

    await press("Opus");
    // Said before it is pressed, the way the registration form says what it will run before it
    // saves: what a model choice does is put a flag on a command line.
    expect(runs()).toBe("claude-code --model opus");
    // Not written on the press — the pane opening is what settles it.
    expect(hoisted.asked).not.toContain("wake_chose_model");

    await press("Open a terminal here");
    expect(hoisted.asked[hoisted.asked.length - 1]).toBe("wake_chose_model");
    expect(hoisted.args[hoisted.args.length - 1]).toEqual({ agent: "claude-code", model: "opus", label: "Opus" });
    expect(started).toEqual(["claude-code"]);
  });

  it("goes back to the default by a press, which is a choice and not the absence of one", async () => {
    hoisted.wake = startable(["claude-code"], "claude-code");
    hoisted.models["claude-code"] = [{ id: "opus", label: "Opus" }];
    hoisted.kept["claude-code"] = { chosen: { id: "opus", label: "Opus" }, history: [{ id: "opus", label: "Opus" }], flag: "--model" };
    await draw("/work/here");

    // What was remembered is what is on, without anybody pressing anything.
    expect(onModel()).toBe("Opus");
    expect(runs()).toBe("claude-code --model opus");

    await press("Its own default");
    await press("Open a terminal here");
    expect(hoisted.asked[hoisted.asked.length - 1]).toBe("wake_forget_model");
    expect(hoisted.args[hoisted.args.length - 1]).toEqual({ agent: "claude-code" });
  });

  it("is a box to type into where the agent has no list to give, with what was typed before under it",
    async () => {
      hoisted.wake = startable(["github-copilot"], "github-copilot");
      hoisted.models["github-copilot"] = [];
      hoisted.kept["github-copilot"] = {
        chosen: null,
        history: [{ id: "claude-sonnet-4.5", label: "claude-sonnet-4.5" }],
        flag: "--model",
      };
      await draw("/work/here");

      // The history is the whole of what anybody can offer for an agent that cannot be asked.
      expect(offered()).toEqual(["Its own default", "claude-sonnet-4.5"]);
      await type("Model name", "gpt-5.5");
      expect(runs()).toBe("github-copilot --model gpt-5.5");

      await press("Open a terminal here");
      expect(hoisted.args[hoisted.args.length - 1])
        .toEqual({ agent: "github-copilot", model: "gpt-5.5", label: "gpt-5.5" });
    });

  it("grows a box to narrow itself where the agent answered with more than a row", async () => {
    hoisted.wake = startable(["cursor"], "cursor");
    hoisted.models["cursor"] = [...Array(40).keys()].map((n) => ({ id: `m-${n}`, label: `Model ${n}` }));
    await draw("/work/here");

    // Drawn as far as a row goes and no further, and the rest are said to be there rather than
    // left to be guessed at.
    expect(offered().length, "the whole answer was drawn as a row").toBe(13);
    expect(container.querySelector(".slot__note")?.textContent).toContain("28");

    await type("Narrow the list", "Model 37");
    expect(offered()).toEqual(["Its own default", "Model 37"]);
  });

  // The plain shell is the absence of an agent and a registered command is a line of the reader's
  // own (`AMB-D-794`) — neither is something Amenbo can ask anything of, so neither is asked.
  it("is not asked about for the plain shell or for a command the reader wrote", async () => {
    hoisted.wake = withOwn(["claude-code"], [["Mine", "mine --model big"]], true, "custom:1");
    await draw("/work/here");

    expect(hoisted.asked).not.toContain("agent_models");
    expect(runs(), "a line the reader wrote is not one Amenbo adds a flag to").toBe(null);

    await press("Plain shell");
    expect(hoisted.asked).not.toContain("agent_models");
  });
});
