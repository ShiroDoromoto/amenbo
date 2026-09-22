// @vitest-environment jsdom
// The automations screen and the build screen's launch place (`AMB-T-5254`). Only the reads are
// stubbed; the tabs, the list, the wording of each reason and what the button does all run for real.
//
// What these guard: **the screen is three tabs and opens on the definitions**, so the two that are
// not built yet cannot quietly become the one a reader lands on; **a row opens the build screen** in
// place of the list rather than beside it; **the launch place says what is in the way, in words**,
// every reason the check can give taking a line a person can act on — including the unnamed way out,
// which has no name to put in a sentence; and **the button is shut while anything is in the way**,
// which is the whole of what the launch place is for.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type {
  AutomationCardDto,
  AutomationDetailDto,
  AutomationLaunchCheckDto,
} from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  automations: [] as AutomationCardDto[],
  detail: null as AutomationDetailDto | null,
  check: null as AutomationLaunchCheckDto | null,
  launch: vi.fn(async (..._args: unknown[]) => ({ run: 1, queued: false })),
}));

vi.mock("../core/automations", () => ({
  useAutomations: () => hoisted.automations,
  useAutomation: () => hoisted.detail,
  useLaunchCheck: () => hoisted.check,
  useAutomationActions: () => [],
  launchAutomation: hoisted.launch,
  // The "running" tab reads it. What that tab draws is its own test (`./runningTab.test.tsx`); here
  // it is the tab being reachable that matters.
  useLiveRuns: () => [],
}));
vi.mock("../core/boundFolders", () => ({
  useBoundFolders: () => ({ all: [], live: [], answered: true }),
}));

import { t, tf } from "../core/i18n";
import { AutomationsScreen } from "./AutomationsScreen";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

function card(over: Partial<AutomationCardDto> = {}): AutomationCardDto {
  return { id: 7, name: "Morning round", steps: 3, archived: false, ...over };
}

function detail(over: Partial<AutomationDetailDto> = {}): AutomationDetailDto {
  return {
    id: 7,
    projectId: 1,
    name: "Morning round",
    notes: "",
    preamble: "",
    archived: false,
    steps: [],
    edges: [],
    wires: [],
    ...over,
  };
}

async function render(workspaceOpen = true) {
  await act(async () => {
    root.render(createElement(AutomationsScreen, { projectId: 1, workspaceOpen }));
  });
}

const buttons = () => [...container.querySelectorAll("button")];
function button(label: string): HTMLButtonElement {
  const found = buttons().find((b) => b.textContent?.includes(label));
  if (!found) throw new Error(`no button labelled ${label}`);
  return found;
}
const blocks = () => [...container.querySelectorAll(".auto__blocks li")].map((li) => li.textContent);

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  hoisted.automations = [];
  hoisted.detail = null;
  hoisted.check = null;
  hoisted.launch.mockClear();
  hoisted.launch.mockResolvedValue({ run: 1, queued: false });
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the automations screen", () => {
  it("draws the three tabs and opens on the definitions", async () => {
    await render();
    const tabs = [...container.querySelectorAll<HTMLButtonElement>(".autotabs__tab")];
    expect(tabs.map((one) => one.textContent)).toEqual([
      t("auto.tab.running"),
      t("auto.tab.automations"),
      t("auto.tab.actions"),
    ]);
    expect(tabs[1].getAttribute("aria-selected")).toBe("true");
  });

  it("moves to the running tab, which is about no one project", async () => {
    await render();
    const tabs = [...container.querySelectorAll<HTMLButtonElement>(".autotabs__tab")];
    await act(async () => { tabs[0].click(); });
    expect(container.querySelector(".auto__list")).toBeNull();
    expect(container.textContent).toContain(t("auto.running.empty"));
  });

  it("says a project with no automations has none", async () => {
    await render();
    expect(container.textContent).toContain(t("auto.empty"));
  });

  it("names each automation and how many steps it is built out of", async () => {
    hoisted.automations = [card({ steps: 3 }), card({ id: 8, name: "Nightly", archived: true })];
    await render();
    const rows = [...container.querySelectorAll(".auto__row")].map((one) => one.textContent ?? "");
    expect(rows[0]).toContain("Morning round");
    expect(rows[0]).toContain(tf("auto.stepCount", { count: 3 }));
    expect(rows[1]).toContain(t("auto.archived"));
  });

  it("opens the build screen in place of the list", async () => {
    hoisted.automations = [card()];
    hoisted.detail = detail();
    hoisted.check = { ready: true, blocks: [] };
    await render();
    await act(async () => { button("Morning round").click(); });
    expect(container.querySelector(".auto__list")).toBeNull();
    expect(container.textContent).toContain(t("auto.build.launch"));
    expect(container.textContent).toContain(t("auto.build.picture"));
    expect(container.textContent).toContain(t("auto.build.step"));
  });
});

describe("the launch place", () => {
  async function open(check: AutomationLaunchCheckDto) {
    hoisted.automations = [card()];
    hoisted.detail = detail();
    hoisted.check = check;
    await render();
    await act(async () => { button("Morning round").click(); });
  }

  it("puts every reason into words a person can act on", async () => {
    // Every arm core's check can answer with (`amenbo_core::ops::automation_run::Unmet`), so a reason
    // added there without words on this side is caught here rather than on somebody's screen.
    await open({
      ready: false,
      blocks: [
        { reason: "no_entry" },
        { reason: "entry_takes_no_task", stepName: "Read" },
        { reason: "open_exit", stepName: "Read", at: "again" },
        { reason: "open_exit", stepName: "Read" },
        { reason: "unwired_input", stepName: "Write", at: "folder" },
        { reason: "unanswered_cfg", stepName: "Write", at: "filter" },
        { reason: "agent_missing", stepName: "Write", at: "codex-cli" },
      ],
    });
    expect(blocks()).toEqual([
      t("auto.block.noEntry"),
      tf("auto.block.entryTakesNoTask", { step: "Read" }),
      tf("auto.block.openExit", { step: "Read", at: "again" }),
      tf("auto.block.openExitUnnamed", { step: "Read" }),
      tf("auto.block.unwiredInput", { step: "Write", at: "folder" }),
      tf("auto.block.unansweredCfg", { step: "Write", at: "filter" }),
      tf("auto.block.agentMissing", { step: "Write", at: "codex-cli" }),
    ]);
    // Nothing is left standing as a bare reason code: a line nobody wrote words for would ship as
    // `input_unfed` on the screen.
    expect(blocks().every((line) => line && !line.includes("_"))).toBe(true);
  });

  it("holds the button shut while anything is in the way", async () => {
    await open({ ready: false, blocks: [{ reason: "no_steps" }] });
    expect(button(t("auto.start")).disabled).toBe(true);
    expect(container.textContent).toContain(t("auto.notReady"));
  });

  it("offers the button once nothing is in the way", async () => {
    await open({ ready: true, blocks: [] });
    expect(container.textContent).toContain(t("auto.ready"));
    expect(button(t("auto.start")).disabled).toBe(false);
  });
});

describe("the press that starts a run", () => {
  async function open(check: AutomationLaunchCheckDto, workspaceOpen = true) {
    hoisted.automations = [card()];
    hoisted.detail = detail();
    hoisted.check = check;
    await render(workspaceOpen);
    await act(async () => { button("Morning round").click(); });
  }

  it("tells the launch which automation, which project and whether the workspace is standing", async () => {
    // The workspace is the shell's to know, and core refuses a launch without one — so a press that
    // did not carry the answer would be refused on a guess made here (`AMB-D-753`).
    await open({ ready: true, blocks: [] }, false);
    await act(async () => { button(t("auto.start")).click(); });
    expect(hoisted.launch).toHaveBeenCalledWith(7, 1, [], false);
  });

  it("says a run took no lane, rather than leaving the press unanswered", async () => {
    hoisted.launch.mockResolvedValue({ run: 3, queued: true });
    await open({ ready: true, blocks: [] });
    await act(async () => { button(t("auto.start")).click(); });
    expect(container.textContent).toContain(t("auto.queued"));
  });

  it("says nothing about a queue where the run took a lane", async () => {
    // What it looks like is the pane arriving, which is the workspace's and not this screen's
    // (`../talk/automationStep`).
    await open({ ready: true, blocks: [] });
    await act(async () => { button(t("auto.start")).click(); });
    expect(container.textContent).not.toContain(t("auto.queued"));
  });

  it("puts a refusal in front of the reader, in the words core refused with", async () => {
    hoisted.launch.mockRejectedValue({
      code: "invalid",
      message_en: "the workspace is closed — a run draws its steps in its panes",
    });
    await open({ ready: true, blocks: [] });
    await act(async () => { button(t("auto.start")).click(); });
    expect(container.textContent).toContain("the workspace is closed");
  });
});
