// @vitest-environment jsdom
// The banner that says this folder's AI is not started on amenbo (`AMB-D-440`). Only the boundary is stubbed
// (core's `harness::setup_notice` scan) and the clipboard; the banner's own branching and wording run for real.
//
// What these guard above all is the **copy**: amenbo writes no provider settings file, so the wiring only ever
// happens if the text reaches the reader's clipboard — a copy button that handed over the wrong tool's text, or
// nothing at all, would leave a setup that reads as finished and is not. Beside it, that the text is on screen
// and not behind the button: what it asks for is an edit by an AI of the reader's, so a copy taken unread is
// the thing this surface exists to avoid. And the ordering: it waits for the question to be done before it
// reads the disk or says a word.
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
    request: 'Merge this into .claude/settings.json: { "hooks": { "SessionStart": [] } }',
    ...over,
  };
}

// core carries no prose — the tools, the files and the text to hand over arrive, and the banner words it.
function notice(over: Partial<AgentHookNoticeDto> = {}): AgentHookNoticeDto {
  return {
    projectName: "案件X",
    dir: "/w/案件X",
    offered: [tool()],
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

/** The text shown in the one folder's block, which is what the copy button carries. */
const shownRequest = () => container.querySelector(".agenthook__request")?.textContent;

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

  // The dev channel is a different command, and the only place its name is on screen is inside the text core
  // worded — the banner's own sentence names no command at all, which is what keeps it from spelling in a
  // name that is wrong on half the builds.
  it("carries the command in core's text, and spells none of its own", async () => {
    hoisted.notices = [notice({ offered: [tool({ request: "run `amenbo-dev agent --json`" })] })];
    await render(true);

    const text = container.textContent ?? "";
    expect(text).toContain("amenbo-dev agent");
    expect(text).not.toContain("`amenbo agent`");
  });

  // The point of the surface. The text is what finishes the setup, since amenbo will not write the file.
  it("copies that tool's own text, and says it did", async () => {
    hoisted.notices = [notice({ offered: [tool({ request: "REQUEST-A" })] })];
    await render(true);

    await act(async () => { copyButtons()[0].click(); });

    expect(clipboard).toEqual(["REQUEST-A"]);
    expect(container.textContent).toContain(t("agentHookSetup.copied"));
  });

  // Read before it is handed on. The text asks an AI of the reader's to edit a file of theirs, so it is on
  // screen in full — not summarised, and not folded behind the button that copies it.
  it("shows the whole text on screen, before anything is copied", async () => {
    hoisted.notices = [notice({ offered: [tool({ request: "REQUEST-A\nsecond line" })] })];
    await render(true);

    const shown = container.querySelector(".agenthook__request");
    expect(shown?.textContent).toBe("REQUEST-A\nsecond line");
    expect(clipboard, "nothing was copied to make it visible").toEqual([]);
  });

  // More than one on offer is more than one text, and handing over the wrong one wires nothing — so the
  // reader picks, and what is shown and what is copied both follow that pick.
  it("lets the reader pick which tool, and shows and copies that one's text", async () => {
    hoisted.notices = [notice({
      offered: [
        tool({ tool: "claude-code", label: "Claude Code", request: "REQUEST-A" }),
        tool({ tool: "cursor", label: "Cursor", pasteInto: ".cursor/hooks.json", request: "REQUEST-B" }),
      ],
    })];
    await render(true);

    // The first on offer until the reader says otherwise, and one text on screen — not both.
    expect(copyButtons()).toHaveLength(1);
    expect(shownRequest()).toBe("REQUEST-A");

    const pick = container.querySelector<HTMLSelectElement>(".agenthook__pick")!;
    await act(async () => {
      pick.value = "cursor";
      pick.dispatchEvent(new Event("change", { bubbles: true }));
    });

    expect(shownRequest(), "the text on screen follows the pick").toBe("REQUEST-B");
    expect(container.textContent).toContain(".cursor/hooks.json");
    await act(async () => { copyButtons()[0].click(); });
    expect(clipboard).toEqual(["REQUEST-B"]);
  });

  // A folder that points at one tool has already answered which; a picker holding a single value is a
  // question with no other answer.
  it("asks nothing where there is only one on offer", async () => {
    hoisted.notices = [notice({ offered: [tool()] })];
    await render(true);

    expect(container.querySelector(".agenthook__pick")).toBeNull();
  });

  // The same tool appears under every folder that traces it, so a copy in one folder must not light up the
  // button in the other — the mark says which text is actually on the clipboard.
  it("marks only the button that was pressed, across folders sharing a tool", async () => {
    hoisted.notices = [
      notice({ offered: [tool({ request: "REQUEST-X" })] }),
      notice({ projectName: "案件Y", dir: "/w/案件Y", offered: [tool({ request: "REQUEST-Y" })] }),
    ];
    await render(true);

    await act(async () => { copyButtons()[1].click(); });

    expect(clipboard).toEqual(["REQUEST-Y"]);
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
