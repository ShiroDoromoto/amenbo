// @vitest-environment jsdom
// The standing row on a project's own screen (`AMB-D-459`). Only the boundary is stubbed (core's per-project
// walk) and the clipboard; the row's own branching and wording run for real.
//
// What these guard is the shape the decision asked for and the banner could not hold: **one text with its
// folders listed under it**, so four folders are four lines and not four screens of identical request; and
// **no way to dismiss it**, because it is work left rather than a question — the only ending it has is the
// last folder being wired. Beside those, the same two the banner is held to: the text is on screen before it
// is copied, and the copy carries the tool the reader picked.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentHookToolDto, AgentHookWiringDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  /** What core's per-project walk reports, grouped by harness. */
  waiting: [] as AgentHookWiringDto[],
  /** Which projects it was asked about, in order — evidence it is read per project, and once. */
  asked: [] as number[],
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
}));

import { t } from "../core/i18n";
import { AgentHookWiringRow } from "./AgentHookWiringRow";

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

async function render(projectId = 7) {
  await act(async () => {
    root.render(createElement(AgentHookWiringRow, { projectId }));
  });
}

/** The folders listed under the one text. */
const dirs = () => [...container.querySelectorAll("li")].map((li) => li.textContent);
/** The text on screen, which is what the copy button carries. */
const shownRequest = () => container.querySelector(".agenthookrow__request")?.textContent;
const copyButton = () => container.querySelector<HTMLButtonElement>(".agenthookrow .btn")!;

beforeEach(() => {
  hoisted.waiting = [];
  hoisted.asked = [];
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

  // Everything wired is nothing left, which is the only way a row with no ✕ ever goes. A project that said
  // no comes back the same way, core having kept the refusal silent.
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

  // It is not a question, so there is nothing to put off: no ✕, and no other way to send it away.
  it("offers no way to dismiss it", async () => {
    hoisted.waiting = [waiting()];
    await render();

    expect(container.querySelector(".healthbanner__close")).toBeNull();
    expect([...container.querySelectorAll("button")]).toHaveLength(1); // the copy button, and nothing else
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
