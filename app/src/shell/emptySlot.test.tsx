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
}));

vi.mock("../core/ipc", () => ({
  invoke: vi.fn(async (cmd: string, args?: unknown) => {
    hoisted.asked.push(cmd);
    hoisted.args.push(args);
    if (cmd === "wake_choices") {
      if (hoisted.wakeFails) throw new Error("the host could not say");
      return hoisted.wake;
    }
    if (cmd === "wake_register" || cmd === "wake_amend" || cmd === "wake_unregister") {
      if (hoisted.keepFails) throw new Error("that command could not be registered");
      if (hoisted.wakeAfter !== null) hoisted.wake = hoisted.wakeAfter;
      return cmd === "wake_register" ? "custom:1" : undefined;
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
  hoisted.args = [];
  hoisted.wakeAfter = null;
  hoisted.keepFails = false;
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

    expect(hoisted.asked).toEqual(["wake_choices", "wake_unregister", "wake_choices"]);
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
