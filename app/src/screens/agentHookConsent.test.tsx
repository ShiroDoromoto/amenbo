// @vitest-environment jsdom
// The modal that asks whether this project's AI may be started on amenbo (`AMB-D-440`, `AMB-D-459`). Only
// the boundary is stubbed (core's `agent_hook_offer` / `agent_hook_answer`); the modal's own branching runs
// for real.
//
// What these guard is what makes this question different from the lint's: it is **per project**, so it is
// put where it is about — the project on screen raises it, and the answer carries that project. It is asked
// about a **state**, so it must be reachable with nothing traced in the folders (the folder that shows no
// provider is still one whose AI can be wired, and the reader is the one who knows which tool they are).
// The three-valued answer is the lint modal's shape and guarded the same way: dismissing records nothing.
// And a yes has to hand over the text, since amenbo writes no settings file and nothing else will
// (`AMB-D-459`).
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentHookOfferDto, AgentHookToolDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  /** The one question core says is due (`harness::reconcile` already judged it), or null for none. */
  offer: null as AgentHookOfferDto | null,
  /** Every answer that reached the store. A dismissal leaves this empty; an answer always names its project. */
  answers: [] as { projectId: number; yes: boolean }[],
  /** What each fetch was asked — the project it is about, and the one-question-at-a-time rule. */
  fetchedWith: [] as { projectId: number; canAsk: boolean }[],
  /** When set, the answer fails and nothing lands. */
  failWith: null as string | null,
  /** How many times the modal reported it has nothing left on screen. */
  done: 0,
  /** What the copy button put on the clipboard. */
  clipboard: [] as string[],
}));

vi.mock("../core/mutations", () => ({
  fetchAgentHookOffer: (projectId: number, canAsk: boolean) => {
    hoisted.fetchedWith.push({ projectId, canAsk });
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
    dirs: ["/w/案件X"],
    cmd: "amenbo",
    again: false,
    offered: [tool()],
    ...over,
  };
}

async function render(over: { projectId?: number | null; turn?: boolean; canAsk?: boolean } = {}) {
  await act(async () => {
    root.render(createElement(AgentHookConsentModal, {
      projectId: over.projectId === undefined ? 7 : over.projectId,
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
  hoisted.clipboard = [];
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: { writeText: (text: string) => (hoisted.clipboard.push(text), Promise.resolve()) },
  });
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

  // The question belongs to a place, so it is put in that place and nowhere else: on a view that is not a
  // project there is no project the answer could be recorded against.
  it("asks nothing, and does not probe, where no project is open", async () => {
    hoisted.offer = offer();
    await render({ projectId: null });

    expect(hoisted.fetchedWith).toEqual([]);
    expect(container.textContent).toBe("");
    expect(hoisted.done, "there is nothing to ask here, which the banner is waiting to hear").toBeGreaterThan(0);
  });

  // The trigger is the opening of a project (`AMB-D-459`), so walking into another one asks about that one.
  it("probes the project on screen, and probes again when the reader opens another", async () => {
    await render({ projectId: 7 });
    await render({ projectId: 8 });

    expect(hoisted.fetchedWith).toEqual([
      { projectId: 7, canAsk: true },
      { projectId: 8, canAsk: true },
    ]);
  });

  // The lint spoke this run, so this question stands down — but the probe still runs, which is how a project
  // somebody wired by hand is adopted without anyone being asked. That is why `canAsk` is passed rather than
  // the fetch being skipped.
  it("still probes, asking nothing, on a run where the lint put its question", async () => {
    await render({ canAsk: false });

    expect(hoisted.fetchedWith).toEqual([{ projectId: 7, canAsk: false }]);
    expect(hoisted.done, "it has nothing to ask, so the banner may take its turn").toBeGreaterThan(0);
  });

  it("asks nothing when core has nothing to ask", async () => {
    await render();
    expect(container.textContent).toBe("");
    expect(hoisted.fetchedWith).toEqual([{ projectId: 7, canAsk: true }]);
  });

  // The whole of what sets this apart from the lint's device-wide consent: the answer belongs to a project.
  it("records the answer against the project the question named", async () => {
    hoisted.offer = offer({ projectId: 42 });
    await render({ projectId: 42 });
    await click(t("agentHook.yes"));

    expect(hoisted.answers).toEqual([{ projectId: 42, yes: true }]);
  });

  it("records a no, and leaves nothing on screen: a refusal is silence from there on", async () => {
    hoisted.offer = offer({ projectId: 42 });
    await render({ projectId: 42 });
    await click(t("agentHook.no"));

    expect(hoisted.answers).toEqual([{ projectId: 42, yes: false }]);
    expect(container.textContent).toBe("");
  });

  it("records nothing when dismissed with Esc, and does not put it again while the app is open", async () => {
    hoisted.offer = offer({ projectId: 99 });
    await render({ projectId: 99 });
    await escape();

    expect(hoisted.answers).toEqual([]); // Unanswered, so a later launch asks again.
    expect(container.textContent).toBe(""); // …and it is gone for this run.

    // Walking away and back is a navigation, not an answer — and must not re-put what was waved past.
    await render({ projectId: null });
    await render({ projectId: 99 });
    expect(container.textContent).toBe("");
    expect(hoisted.fetchedWith.filter((f) => f.projectId === 99)).toHaveLength(1);
  });

  it("names the project and every folder the text is to be pasted in", async () => {
    hoisted.offer = offer({ dirs: ["/w/案件X", "/w/案件X-2"] });
    await render();

    expect(container.textContent).toContain("案件X");
    expect(container.textContent).toContain("/w/案件X");
    expect(container.textContent).toContain("/w/案件X-2");
  });

  // What a yes buys, said before the click and not after it: amenbo writes no provider settings file, so a
  // reader must not come away thinking the answer wired anything — the text, and an AI of theirs, do that.
  it("says what a yes hands over and who makes the edit, before the answer is given", async () => {
    hoisted.offer = offer();
    await render();
    expect(container.textContent).toContain(tf("agentHook.why", { tool: "Claude Code" }));
  });

  it("names the tool when the folders point at exactly one", async () => {
    hoisted.offer = offer();
    await render();
    expect(container.textContent).toContain("Claude Code");
  });

  // With several on offer, which one the reader is using is theirs to say — the picker after the yes is
  // where they say it. The sentence stays whole either way: "your tool" stands where the name would be, so
  // what the text does is said to every reader, named tool or not.
  it("names none when there are several to choose between, and still says what the text does", async () => {
    hoisted.offer = offer({ offered: [tool(), tool({ tool: "cursor", label: "Cursor" })] });
    await render();

    expect(container.textContent).not.toContain("Claude Code");
    expect(container.textContent).toContain(tf("agentHook.why", { tool: t("agentHook.someTool") }));
  });

  // The question is asked about a state, and a folder that traces nothing is in that state too. Withholding
  // it there would leave the feature undiscoverable for exactly the readers who have not set anything up.
  it("is still put where the folders point at no tool, and hands over the command that prints the text", async () => {
    hoisted.offer = offer({ offered: [tool(), tool({ tool: "cursor", label: "Cursor" })] });
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
    hoisted.offer = offer({ again: true });
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

  // A yes buys the text and nothing else — amenbo writes no settings file — so the modal that took the yes
  // is what hands it over (`AMB-D-459`). Nothing else will: the reader is not sent looking for it.
  describe("handing over the text a yes bought", () => {
    it("shows the text, with the file it goes in and the folders it goes in", async () => {
      hoisted.offer = offer({ dirs: ["/w/案件X", "/w/案件X-2"] });
      await render();
      await click(t("agentHook.yes"));

      const text = container.textContent ?? "";
      expect(text, "the text is on screen, not behind the button").toContain(tool().request);
      expect(text).toContain(tf("agentHookSetup.unwired", { tool: "Claude Code", file: ".claude/settings.json" }));
      expect(text, "one text, and the folders it is to be pasted in").toContain("/w/案件X-2");
    });

    it("copies the text of the tool on show", async () => {
      hoisted.offer = offer();
      await render();
      await click(t("agentHook.yes"));
      await click(t("agentHookSetup.copy"));

      expect(hoisted.clipboard).toEqual([tool().request]);
      expect(container.textContent).toContain(t("agentHookSetup.copied"));
    });

    // Handing over the wrong tool's text wires nothing, so where there is a choice the reader makes it —
    // and the text under the button changes with it.
    it("lets the reader pick which tool the text is for, and hands over that one", async () => {
      const cursor = tool({ tool: "cursor", label: "Cursor", pasteInto: ".cursor/x.json", request: "Merge into .cursor/x.json" });
      hoisted.offer = offer({ offered: [tool(), cursor] });
      await render();
      await click(t("agentHook.yes"));

      const pick = container.querySelector("select") as HTMLSelectElement;
      expect(pick, "with more than one on offer, which is the reader's to say").not.toBeNull();
      await act(async () => {
        pick.value = "cursor";
        pick.dispatchEvent(new Event("change", { bubbles: true }));
      });
      await click(t("agentHookSetup.copy"));

      expect(container.textContent).toContain(cursor.request);
      expect(hoisted.clipboard).toEqual([cursor.request]);
    });

    it("does not hand anything over after a no", async () => {
      hoisted.offer = offer();
      await render();
      await click(t("agentHook.no"));
      expect(container.textContent).toBe("");
    });

    it("closes on the reader's word, and does not come back when the project is opened again", async () => {
      hoisted.offer = offer({ projectId: 55 });
      await render({ projectId: 55 });
      await click(t("agentHook.yes"));
      await click(t("pane.close"));
      expect(container.textContent).toBe("");

      // The consent is recorded now, so core has nothing left to say — which is what ends it, not a latch.
      hoisted.offer = null;
      await render({ projectId: 7 });
      await render({ projectId: 55 });
      expect(container.textContent).toBe("");
    });
  });

  describe("saying it is done", () => {
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

    // The hand-over is still this surface's turn: what follows it only tells, and telling over an open
    // dialog says the same thing twice.
    it("waits for the hand-over to be read before saying so", async () => {
      hoisted.offer = offer();
      await render();
      await click(t("agentHook.yes"));
      expect(hoisted.done).toBe(0);

      await click(t("pane.close"));
      expect(hoisted.done).toBeGreaterThan(0);
    });

    it("says so once a no ends it", async () => {
      hoisted.offer = offer();
      await render();
      await click(t("agentHook.no"));
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
