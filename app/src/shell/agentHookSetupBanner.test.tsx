// @vitest-environment jsdom
// The banner that says this folder's AI is not started on amenbo (`AMB-D-440`). Only the boundary is stubbed
// (core's `harness::setup_notice` scan) and the clipboard; the banner's own branching and wording run for real.
//
// What these guard above all is the **copy**: amenbo writes no provider settings file, so the wiring only ever
// happens if the text reaches the reader's clipboard — a copy button that handed over the wrong tool's snippet,
// or nothing at all, would leave a setup that reads as finished and is not. And the ordering: it waits for the
// question to be done before it reads the disk or says a word.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentHookNoticeDto, AgentHookToolDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  /** What core's scan reports (the one probe per bound folder). */
  notices: [] as AgentHookNoticeDto[],
  /** How many times the scan was called — evidence it happens once, and never before the modal is done. */
  calls: 0,
}));

vi.mock("../core/snapshot", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/snapshot")>();
  return { ...orig, inTauri: () => true, subscribe: () => () => {} };
});
vi.mock("../core/mutations", () => ({
  fetchAgentHookNotices: () => {
    hoisted.calls += 1;
    return Promise.resolve(hoisted.notices);
  },
  // The remaining boundaries the AppShell module (where the banner lives) imports; unused by this test.
  fetchHookNotices: () => Promise.resolve([]),
  fetchPointerIssues: () => Promise.resolve([]),
  repairPointers: () => Promise.resolve({ repaired: [], unresolved: [] }),
  fetchStaleManagedBlocks: () => Promise.resolve([]),
  fetchOrphanBindings: () => Promise.resolve([]),
  resyncManagedBlocks: () => Promise.resolve({ scanned: 0, updated: [] }),
  forgetOrphanBindings: () => Promise.resolve(0),
  openLatestInstaller: () => Promise.resolve(),
}));

import { t } from "../core/i18n";
import { AgentHookSetupBanner } from "./AppShell";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
let clipboard: string[];

function tool(over: Partial<AgentHookToolDto> = {}): AgentHookToolDto {
  return {
    tool: "claude-code",
    label: "Claude Code",
    pasteInto: ".claude/settings.json",
    snippet: '{ "hooks": { "SessionStart": [] } }',
    ...over,
  };
}

// core carries no prose — the tools, the files and the command's own name arrive, and the banner words it.
function notice(over: Partial<AgentHookNoticeDto> = {}): AgentHookNoticeDto {
  return {
    projectName: "案件X",
    dir: "/w/案件X",
    cmd: "amenbo",
    unwired: [tool()],
    ...over,
  };
}

async function render(asked: boolean) {
  await act(async () => {
    root.render(createElement(AgentHookSetupBanner, { asked }));
  });
}

/** The copy buttons, in the order they are on screen. */
const copyButtons = () =>
  [...container.querySelectorAll<HTMLButtonElement>(".healthbanner__action")];

beforeEach(() => {
  hoisted.notices = [];
  hoisted.calls = 0;
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

describe("the session-start hook setup banner", () => {
  // The whole reason the banner takes a prop instead of scanning on mount: it must not talk over the question,
  // and the disk it reports on is the disk that question's sweep just wrote to.
  it("reads nothing and shows nothing while the modal is still asking", async () => {
    hoisted.notices = [notice()];
    await render(false);

    expect(hoisted.calls, "the probe is not paid before the answers are in").toBe(0);
    expect(container.querySelector(".healthbanner")).toBeNull();
  });

  it("reads the disk once the modal is done, and once only", async () => {
    hoisted.notices = [notice()];
    await render(true);

    expect(hoisted.calls).toBe(1);
    expect(container.querySelector(".healthbanner")).not.toBeNull();
  });

  // A folder that is wired, and one whose owner said no, both come back from core as nothing to say.
  it("stays silent when core reports nothing", async () => {
    await render(true);
    expect(container.querySelector(".healthbanner")).toBeNull();
  });

  it("names the project, the folder, the tool and the file the text goes into", async () => {
    hoisted.notices = [notice()];
    await render(true);

    const text = container.textContent ?? "";
    expect(text).toContain("案件X");
    expect(text).toContain("/w/案件X");
    expect(text).toContain("Claude Code");
    expect(text).toContain(".claude/settings.json");
  });

  // The dev channel is a different command, and the wording must follow it rather than spell `amenbo` in.
  it("says the command core reported, not a hardcoded name", async () => {
    hoisted.notices = [notice({ cmd: "amenbo-dev" })];
    await render(true);

    const text = container.textContent ?? "";
    expect(text).toContain("amenbo-dev agent");
    expect(text).not.toContain("`amenbo agent`");
  });

  // The point of the surface. The text is what finishes the setup, since amenbo will not write the file.
  it("copies that tool's own snippet, and says it did", async () => {
    hoisted.notices = [notice({ unwired: [tool({ snippet: "SNIPPET-A" })] })];
    await render(true);

    await act(async () => { copyButtons()[0].click(); });

    expect(clipboard).toEqual(["SNIPPET-A"]);
    expect(container.textContent).toContain(t("agentHookSetup.copied"));
  });

  // Two tools traced in one folder is two texts, and pasting the wrong one wires nothing — so it is one
  // button each, and the "copied" mark belongs to the one that was pressed.
  it("gives every unwired tool its own button, carrying its own text", async () => {
    hoisted.notices = [notice({
      unwired: [
        tool({ tool: "claude-code", label: "Claude Code", snippet: "SNIPPET-A" }),
        tool({ tool: "cursor", label: "Cursor", pasteInto: ".cursor/hooks.json", snippet: "SNIPPET-B" }),
      ],
    })];
    await render(true);

    expect(copyButtons()).toHaveLength(2);
    await act(async () => { copyButtons()[1].click(); });
    expect(clipboard).toEqual(["SNIPPET-B"]);
  });

  // The same tool appears under every folder that traces it, so a copy in one folder must not light up the
  // button in the other — the mark says which text is actually on the clipboard.
  it("marks only the button that was pressed, across folders sharing a tool", async () => {
    hoisted.notices = [
      notice({ unwired: [tool({ snippet: "SNIPPET-X" })] }),
      notice({ projectName: "案件Y", dir: "/w/案件Y", unwired: [tool({ snippet: "SNIPPET-Y" })] }),
    ];
    await render(true);

    await act(async () => { copyButtons()[1].click(); });

    expect(clipboard).toEqual(["SNIPPET-Y"]);
    expect(copyButtons()[0].textContent).toContain(t("agentHookSetup.copy"));
    expect(copyButtons()[1].textContent).toContain(t("agentHookSetup.copied"));
  });

  it("lists every unfinished folder, and ✕ dismisses the banner for the session", async () => {
    hoisted.notices = [notice(), notice({ projectName: "案件Y", dir: "/w/案件Y" })];
    await render(true);

    expect(container.querySelectorAll(".healthbanner__line")).toHaveLength(2);

    await act(async () => {
      container.querySelector<HTMLButtonElement>(".healthbanner__close")!.click();
    });
    expect(container.querySelector(".healthbanner")).toBeNull();
  });

  // A terminal with no clipboard must not leave the banner claiming the text was handed over.
  it("says nothing was copied when the clipboard is unavailable", async () => {
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: () => Promise.reject(new Error("no clipboard")) },
    });
    hoisted.notices = [notice()];
    await render(true);

    await act(async () => { copyButtons()[0].click(); });
    expect(container.textContent).not.toContain(t("agentHookSetup.copied"));
  });
});
