// @vitest-environment jsdom
// The standing row on a project's own screen — the face the reader acts on for the session-start hook
// (`AMB-D-459`, `AMB-D-460`). Only the boundary is stubbed (core's per-project walk, the answer, and the
// clipboard); the row's own branching and wording run for real.
//
// What these guard: **one text with its folders listed under it**, so four folders are four lines and not
// four screens of identical request; the text on screen before it is copied, with the copy carrying the
// tool the reader picked; and the two ways off the screen kept apart — **"no" records a refusal** and is
// the only ending the report has, while **"close" records nothing** and is spent the moment the project is
// opened again.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentHookToolDto, AgentHookWiringDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  /** What core's per-project walk reports, grouped by harness. */
  waiting: [] as AgentHookWiringDto[],
  /** Which projects it was asked about, in order — evidence it is read per project, and once. */
  asked: [] as number[],
  /** Every answer recorded, as `[project, yes]` — the row must write on the no and on nothing else. */
  answered: [] as Array<[number, boolean]>,
  /** When set, recording the answer fails with this message. */
  answerFails: null as string | null,
}));

vi.mock("../core/snapshot", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/snapshot")>();
  return { ...orig, inTauri: () => true, subscribe: () => () => {} };
});
vi.mock("../core/mutations", () => ({
  fetchAgentHookProjectWiring: (projectId: number) => {
    hoisted.asked.push(projectId);
    return Promise.resolve(hoisted.waiting);
  },
  answerAgentHookOffer: (projectId: number, yes: boolean) => {
    if (hoisted.answerFails) return Promise.reject(new Error(hoisted.answerFails));
    hoisted.answered.push([projectId, yes]);
    return Promise.resolve();
  },
}));

import { t } from "../core/i18n";
import { AgentHookWiringRow, useAgentHookWiring } from "./AgentHookWiringRow";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
let clipboard: string[];

function tool(over: Partial<AgentHookToolDto> = {}): AgentHookToolDto {
  return {
    tool: "claude-code",
    label: "Claude Code",
    pasteInto: ".claude/settings.json",
    request: 'Merge this into .claude/settings.json: { "hooks": { "SessionStart": [] } }',
    ...over,
  };
}

// core carries no prose — the tool, the file and the text to hand over arrive, and the row words it.
function waiting(over: Partial<AgentHookWiringDto> = {}): AgentHookWiringDto {
  return { tool: tool(), dirs: ["/w/案件X"], ...over };
}

// The row takes what the project has left to wire rather than reading it — the board needs the same answer
// to order its one standing notice (`AMB-D-535`), so the read is the Hook's and this is what joins them.
function Standing({ projectId }: { projectId: number }) {
  return createElement(AgentHookWiringRow, { projectId, wiring: useAgentHookWiring(projectId) });
}

async function render(projectId = 7) {
  await act(async () => {
    root.render(createElement(Standing, { projectId }));
  });
}

/** The folders listed under the one text. */
const dirs = () => [...container.querySelectorAll("li")].map((li) => li.textContent);
/** The text on screen, which is what the copy button carries. */
const shownRequest = () => container.querySelector(".agenthookrow__request")?.textContent;
/** The row's buttons, by their visible label. */
function button(label: string): HTMLButtonElement {
  const found = [...container.querySelectorAll("button")].find((b) => b.textContent?.includes(label));
  if (!found) throw new Error(`no button labelled ${label}`);
  return found as HTMLButtonElement;
}
const copyButton = () => button(t("agentHookWiring.copy"));

beforeEach(() => {
  hoisted.waiting = [];
  hoisted.asked = [];
  hoisted.answered = [];
  hoisted.answerFails = null;
  clipboard = [];
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: { writeText: (s: string) => { clipboard.push(s); return Promise.resolve(); } },
  });
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the project's standing wiring row", () => {
  // The gap the row exists for: consent is per project and wiring is per folder, so this asks about the
  // project being looked at — not the device.
  it("asks about the project on screen, and once", async () => {
    hoisted.waiting = [waiting()];
    await render(7);

    expect(hoisted.asked).toEqual([7]);
    expect(container.querySelector(".agenthookrow")).not.toBeNull();
  });


  // Everything wired is nothing left. A project that said no is silent the same way, core having kept the
  // refusal out of the walk.
  it("draws nothing when core reports nothing left", async () => {
    await render();

    expect(container.querySelector(".agenthookrow")).toBeNull();
  });

  // The whole shape of the decision: the request is the same text wherever it is pasted, so it goes up once
  // and the folders are a list. Four folders must not be four copies of the same screenful.
  it("shows the text once, with every folder still waiting listed under it", async () => {
    hoisted.waiting = [waiting({ dirs: ["/w/one", "/w/two", "/w/three"], tool: tool({ request: "REQUEST-A" }) })];
    await render();

    expect(container.querySelectorAll(".agenthookrow__request")).toHaveLength(1);
    expect(shownRequest()).toBe("REQUEST-A");
    expect(dirs()).toEqual(["/w/one", "/w/two", "/w/three"]);
  });

  // Three buttons and no fourth: the text, the refusal, and the way off the screen that answers nothing.
  it("offers the text, the no, and the close — and nothing else", async () => {
    hoisted.waiting = [waiting()];
    await render();

    expect([...container.querySelectorAll("button")].map((b) => b.textContent)).toEqual([
      t("agentHookWiring.copy"),
      t("agentHookWiring.no"),
      t("pane.close"),
    ]);
  });

  // The reason the no is on the row at all: it is the one answer that ends the report, and a reader who
  // changed their mind reaches it where they read it rather than three steps away in the settings.
  it("records a refusal against this project, and goes", async () => {
    hoisted.waiting = [waiting()];
    await render(7);

    await act(async () => { button(t("agentHookWiring.no")).click(); });

    expect(hoisted.answered).toEqual([[7, false]]);
    expect(container.querySelector(".agenthookrow"), "the refusal is what ends it").toBeNull();
  });

  // A row that vanished on a write that never landed would report an answer nobody has.
  it("stays up with the reason when the refusal could not be recorded", async () => {
    hoisted.waiting = [waiting()];
    hoisted.answerFails = "permission denied";
    await render();

    await act(async () => { button(t("agentHookWiring.no")).click(); });

    expect(container.querySelector(".agenthookrow"), "nothing was recorded, so nothing is over").not.toBeNull();
    expect(container.textContent).toContain("permission denied");
  });

  // Closing is not answering: it takes the row off the screen in front of the reader and writes nothing,
  // so the work behind it is still there the next time the project is opened.
  it("closes without recording anything, and comes back when the project is opened again", async () => {
    hoisted.waiting = [waiting()];
    await render(7);

    await act(async () => { button(t("pane.close")).click(); });
    expect(container.querySelector(".agenthookrow")).toBeNull();
    expect(hoisted.answered, "closing answers nothing").toEqual([]);

    // Walking to another project and back is where "for now" ends — the row reads the disk again, and what
    // it finds is the work nobody answered for.
    await render(8);
    await render(7);
    expect(container.querySelector(".agenthookrow")).not.toBeNull();
  });

  it("names the tool and the file the text goes into", async () => {
    hoisted.waiting = [waiting()];
    await render();

    const text = container.textContent ?? "";
    expect(text).toContain("Claude Code");
    expect(text).toContain(".claude/settings.json");
  });

  // The dev channel answers to a different command, and the only place a command name is on screen is inside
  // the text core worded — the row's own sentences name none.
  it("carries the command in core's text, and spells none of its own", async () => {
    hoisted.waiting = [waiting({ tool: tool({ request: "run `amenbo-dev agent --json`" }) })];
    await render();

    const text = container.textContent ?? "";
    expect(text).toContain("amenbo-dev agent");
    expect(text).not.toContain("`amenbo agent`");
  });

  // The point of the surface: amenbo writes no settings file, so the wiring only happens if the text reaches
  // the clipboard.
  it("copies the text, and says it did", async () => {
    hoisted.waiting = [waiting({ tool: tool({ request: "REQUEST-A" }) })];
    await render();

    await act(async () => { copyButton().click(); });

    expect(clipboard).toEqual(["REQUEST-A"]);
    expect(container.textContent).toContain(t("agentHookWiring.copied"));
  });

  // Read before it is handed on: it asks an AI of the reader's to edit a file of theirs.
  it("shows the whole text on screen, before anything is copied", async () => {
    hoisted.waiting = [waiting({ tool: tool({ request: "REQUEST-A\nsecond line" }) })];
    await render();

    expect(shownRequest()).toBe("REQUEST-A\nsecond line");
    expect(clipboard, "nothing was copied to make it visible").toEqual([]);
  });

  // More than one tool waiting is more than one text, and handing over the wrong one wires nothing — so the
  // reader picks, and the folders shown are that tool's own.
  it("lets the reader pick which tool, and follows the pick in text, folders and copy", async () => {
    hoisted.waiting = [
      waiting({ dirs: ["/w/one"], tool: tool({ request: "REQUEST-A" }) }),
      waiting({
        dirs: ["/w/two"],
        tool: tool({ tool: "cursor", label: "Cursor", pasteInto: ".cursor/hooks.json", request: "REQUEST-B" }),
      }),
    ];
    await render();

    // The first waiting until the reader says otherwise, and one text on screen — not both.
    expect(shownRequest()).toBe("REQUEST-A");
    expect(dirs()).toEqual(["/w/one"]);

    const pick = container.querySelector<HTMLSelectElement>(".agenthookrow__pick")!;
    await act(async () => {
      pick.value = "cursor";
      pick.dispatchEvent(new Event("change", { bubbles: true }));
    });

    expect(shownRequest(), "the text on screen follows the pick").toBe("REQUEST-B");
    expect(dirs(), "and so do the folders waiting for it").toEqual(["/w/two"]);
    expect(container.textContent).toContain(".cursor/hooks.json");
    await act(async () => { copyButton().click(); });
    expect(clipboard).toEqual(["REQUEST-B"]);
  });

  // One tool waiting is a question with no other answer.
  it("asks nothing where there is only one tool waiting", async () => {
    hoisted.waiting = [waiting()];
    await render();

    expect(container.querySelector(".agenthookrow__pick")).toBeNull();
  });

  // A machine with no clipboard must not leave the row claiming the text was handed over.
  it("says nothing was copied when the clipboard is unavailable", async () => {
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: () => Promise.reject(new Error("no clipboard")) },
    });
    hoisted.waiting = [waiting()];
    await render();

    await act(async () => { copyButton().click(); });
    expect(container.textContent).not.toContain(t("agentHookWiring.copied"));
  });
});
