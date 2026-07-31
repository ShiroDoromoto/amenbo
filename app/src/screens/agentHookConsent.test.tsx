// @vitest-environment jsdom
// The modal that asks whether this folder's AI may be started on amenbo (`AMB-D-440`). Only the boundary is
// stubbed (core's `agent_hook_offer` / `agent_hook_answer`); the modal's own branching runs for real.
//
// What these guard is what makes this question different from the lint's: it is **per project**, so the
// answer has to carry the project it was about — and it is asked about a **state** found on the startup
// sweep, so it must be reachable with nothing traced in the folder (the folder that shows no provider is
// still a folder whose AI can be wired, and the reader is the one who knows which tool they are). The
// three-valued answer is the lint modal's shape and guarded the same way: dismissing records nothing.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentHookOfferDto, AgentHookToolDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  /** The one question core says is due (`harness::reconcile` already judged it), or null for none. */
  offer: null as AgentHookOfferDto | null,
  /** Every answer that reached the store. A dismissal leaves this empty; an answer always names its project. */
  answers: [] as { projectId: number; yes: boolean }[],
  /** What the fetch was asked — the one-question-at-a-time rule reaching the backend. */
  fetchedWith: [] as boolean[],
  /** When set, the answer fails and nothing lands. */
  failWith: null as string | null,
  /** How many times the modal reported it has nothing left to ask. */
  done: 0,
}));

vi.mock("../core/mutations", () => ({
  fetchAgentHookOffer: (canAsk: boolean) => {
    hoisted.fetchedWith.push(canAsk);
    return Promise.resolve(hoisted.offer);
  },
  answerAgentHookOffer: (projectId: number, yes: boolean) => {
    if (hoisted.failWith) return Promise.reject(new Error(hoisted.failWith));
    hoisted.answers.push({ projectId, yes });
    return Promise.resolve();
  },
}));

import { t, tf } from "../core/i18n";
import { AgentHookConsentModal } from "./AgentHookConsentModal";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

function tool(over: Partial<AgentHookToolDto> = {}): AgentHookToolDto {
  return {
    tool: "claude-code",
    label: "Claude Code",
    pasteInto: ".claude/settings.json",
    request: 'Merge this into .claude/settings.json: { "hooks": {} }',
    ...over,
  };
}

function offer(over: Partial<AgentHookOfferDto> = {}): AgentHookOfferDto {
  return {
    projectId: 7,
    projectName: "案件X",
    dir: "/w/案件X",
    cmd: "amenbo",
    again: false,
    named: [],
    ...over,
  };
}

async function render(over: { turn?: boolean; canAsk?: boolean } = {}) {
  await act(async () => {
    root.render(createElement(AgentHookConsentModal, {
      turn: over.turn ?? true,
      canAsk: over.canAsk ?? true,
      onDone: () => (hoisted.done += 1),
    }));
  });
}

function button(label: string): HTMLButtonElement {
  const found = [...container.querySelectorAll("button")].find((b) => b.textContent?.includes(label));
  if (!found) throw new Error(`no button labelled ${label}: ${container.textContent}`);
  return found as HTMLButtonElement;
}

async function click(label: string) {
  await act(async () => {
    button(label).click();
  });
}

/** Esc, which is "not now" — the third answer the two buttons have no room for. */
async function escape() {
  await act(async () => {
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
  });
}

beforeEach(() => {
  hoisted.offer = null;
  hoisted.answers = [];
  hoisted.fetchedWith = [];
  hoisted.failWith = null;
  hoisted.done = 0;
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
  vi.restoreAllMocks();
});

describe("the session-start hook consent modal", () => {
  it("asks nothing, and does not even probe, while the lint's question still has the turn", async () => {
    hoisted.offer = offer();
    await render({ turn: false });

    expect(hoisted.fetchedWith, "the probe is not paid before the queue reaches us").toEqual([]);
    expect(container.textContent).toBe("");
  });

  // The lint spoke this run, so this question stands down — but the sweep still runs, which is how a folder
  // somebody wired by hand is adopted without anyone being asked. That is why `canAsk` is passed rather than
  // the fetch being skipped.
  it("still sweeps, asking nothing, on a run where the lint put its question", async () => {
    await render({ canAsk: false });

    expect(hoisted.fetchedWith).toEqual([false]);
    expect(hoisted.done, "it has nothing to ask, so the banner may take its turn").toBeGreaterThan(0);
  });

  it("asks nothing when core has nothing to ask", async () => {
    await render();
    expect(container.textContent).toBe("");
    expect(hoisted.fetchedWith).toEqual([true]);
  });

  // The whole of what sets this apart from the lint's device-wide consent: the answer belongs to a project.
  it("records the answer against the project the question named", async () => {
    hoisted.offer = offer({ projectId: 42 });
    await render();
    await click(t("agentHook.yes"));

    expect(hoisted.answers).toEqual([{ projectId: 42, yes: true }]);
    expect(container.textContent).toBe("");
  });

  it("records a no", async () => {
    hoisted.offer = offer({ projectId: 42 });
    await render();
    await click(t("agentHook.no"));
    expect(hoisted.answers).toEqual([{ projectId: 42, yes: false }]);
  });

  it("records nothing when dismissed with Esc: putting it off is not a no", async () => {
    hoisted.offer = offer();
    await render();
    await escape();

    expect(hoisted.answers).toEqual([]); // Unanswered, so a later startup asks again.
    expect(container.textContent).toBe(""); // …and it is gone for this run.
  });

  it("names the project and folder it is asking about", async () => {
    hoisted.offer = offer();
    await render();

    expect(container.textContent).toContain("案件X");
    expect(container.textContent).toContain("/w/案件X");
  });

  // What a yes buys, said before the click and not after it: amenbo writes no provider settings file, so a
  // reader must not come away thinking the answer wired anything — the text, and an AI of theirs, do that.
  it("says what a yes hands over and who makes the edit, before the answer is given", async () => {
    hoisted.offer = offer({ named: [tool()] });
    await render();
    expect(container.textContent).toContain(tf("agentHook.why", { tool: "Claude Code" }));
  });

  it("names the tool when the folder points at exactly one", async () => {
    hoisted.offer = offer({ named: [tool()] });
    await render();
    expect(container.textContent).toContain("Claude Code");
  });

  // With several traced, which one the reader is using is theirs to say — and the banner lists them all
  // afterwards, each with its own text. The sentence stays whole either way: "your tool" stands where the
  // name would be, so what the text does is said to every reader, named tool or not.
  it("names none when the folder points at several, and still says what the text does", async () => {
    hoisted.offer = offer({ named: [tool(), tool({ tool: "cursor", label: "Cursor" })] });
    await render();

    expect(container.textContent).not.toContain("Claude Code");
    expect(container.textContent).toContain(tf("agentHook.why", { tool: t("agentHook.someTool") }));
  });

  // The question is asked about a state, and a folder that traces nothing is in that state too. Withholding
  // it there would leave the feature undiscoverable for exactly the readers who have not set anything up.
  it("is still put when the folder points at no tool at all, and hands over the command that prints the text", async () => {
    hoisted.offer = offer({ named: [] });
    await render();

    expect(container.querySelector(".hookconsent__modal")).not.toBeNull();
    expect(container.textContent).toContain("amenbo agent-hook snippet");
  });

  it("words its hint with the command name this build answers to, never a spelled-in one", async () => {
    hoisted.offer = offer({ cmd: "amenbo-dev" });
    await render();
    expect(container.textContent).toContain("amenbo-dev agent-hook snippet");
  });

  // The re-ask has a different occasion — a standing yes with nothing wired — and says so, plus that it is
  // the last time. It leads with that and keeps what the text does after it: the first asking was months and
  // a screen ago, so a panel that only said "you already agreed" would ask for a yes to something unstated.
  it("words the one re-ask as a wiring that never landed, says it is the last, and still says what the text does", async () => {
    hoisted.offer = offer({ again: true, named: [tool()] });
    await render();

    const text = container.textContent ?? "";
    expect(text).toContain(t("agentHook.again"));
    expect(text).toContain(t("agentHook.scopeAgain"));
    expect(text).toContain(tf("agentHook.why", { tool: "Claude Code" }));
  });

  it("keeps the question up when the answer failed, so the failure is never recorded as consent", async () => {
    hoisted.offer = offer();
    hoisted.failWith = "database is locked";
    await render();
    await click(t("agentHook.yes"));

    expect(hoisted.answers).toEqual([]);
    expect(container.querySelector(".hookconsent__modal")).not.toBeNull();
  });

  describe("saying it is done asking", () => {
    it("does not say so while the question is still up", async () => {
      hoisted.offer = offer();
      await render();
      expect(hoisted.done).toBe(0);

      await escape();
      expect(hoisted.done).toBeGreaterThan(0);
    });

    it("says so when there was never anything to ask", async () => {
      await render();
      expect(hoisted.done).toBeGreaterThan(0);
    });

    it("says so once the question is answered", async () => {
      hoisted.offer = offer();
      await render();
      await click(t("agentHook.yes"));
      expect(hoisted.done).toBeGreaterThan(0);
    });

    it("does not say so when an answer failed and the question is still up", async () => {
      hoisted.offer = offer();
      hoisted.failWith = "database is locked";
      await render();
      await click(t("agentHook.yes"));
      expect(hoisted.done).toBe(0);
    });
  });
});
