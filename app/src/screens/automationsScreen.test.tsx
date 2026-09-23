// @vitest-environment jsdom
// The automations screen and the build screen's launch place (`AMB-T-5254`). Only the reads are
// stubbed; the tabs, the list, the wording of each reason and what the button does all run for real.
//
// What these guard: **the screen is three tabs and opens on the definitions**, so the two that are
// not built yet cannot quietly become the one a reader lands on; **a row says whether its automation
// could be started**, from the same check the launch place reads; **a row opens the build screen** in
// place of the list rather than beside it; **the panel beside the picture is opened by what was
// pressed** — the library by the press on an empty picture, the definition's own fields by "Edit" —
// and by nothing else; **the launch place says what is in the way, in words**,
// every reason the check can give taking a line a person can act on — including the unnamed way out,
// which has no name to put in a sentence; and **the button is shut while anything is in the way**,
// which is the whole of what the launch place is for.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type {
  AutomationCardDto,
  AutomationDetailDto,
  AutomationLaunchBlockDto,
  AutomationLaunchCheckDto,
} from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  automations: [] as AutomationCardDto[],
  detail: null as AutomationDetailDto | null,
  check: null as AutomationLaunchCheckDto | null,
  launch: vi.fn(async (..._args: unknown[]) => ({ run: 1 })),
}));

vi.mock("../core/automations", () => ({
  useAutomations: () => hoisted.automations,
  useAutomation: () => hoisted.detail,
  useLaunchCheck: () => hoisted.check,
  useAutomationActions: () => [],
  useAutomationAction: () => null,
  insertAutomationAction: () => Promise.resolve(),
  placeAutomationAction: () => Promise.resolve(),
  launchAutomation: hoisted.launch,
  // The "running" tab reads it. What that tab draws is its own test (`./runningTab.test.tsx`); here
  // it is the tab being reachable that matters.
  useLiveRuns: () => [],
  // The step panel's own write door. Nothing here presses a step, so the panel draws its "press one"
  // line and these are never called (`./automationStepPanel.test.tsx` is where they are).
  editAutomationStep: () => Promise.resolve(),
  answerAutomationCfg: () => Promise.resolve(),
  setAutomationWire: () => Promise.resolve(),
  clearAutomationWire: () => Promise.resolve(),
}));
vi.mock("../core/boundFolders", () => ({
  useBoundFolders: () => ({ all: [], live: [], answered: true }),
}));

import { errSentence, errText, t, tf } from "../core/i18n";
import { AutomationsScreen } from "./AutomationsScreen";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

function card(over: Partial<AutomationCardDto> = {}): AutomationCardDto {
  return { id: 7, name: "Morning round", placements: 3, archived: false, ...over };
}

function detail(over: Partial<AutomationDetailDto> = {}): AutomationDetailDto {
  return {
    id: 7,
    projectId: 1,
    name: "Morning round",
    notes: "",
    archived: false,
    placements: [],
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
  hoisted.launch.mockResolvedValue({ run: 1 });
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

  it("names each automation, how many actions are placed on it and whether it could start", async () => {
    hoisted.automations = [card({ placements: 3 }), card({ id: 8, name: "Nightly", archived: true })];
    hoisted.check = { ready: false, blocks: [] };
    await render();
    const rows = [...container.querySelectorAll(".autolist__row")].map((one) => one.textContent ?? "");
    expect(rows[0]).toContain("Morning round");
    expect(rows[0]).toContain(tf("auto.stepCount", { count: 3 }));
    expect(rows[0]).toContain(t("auto.notReady"));
    expect(rows[1]).toContain(t("auto.archived"));
  });

  it("leads each row with the automation's ID, the number the terminal names it by", async () => {
    hoisted.automations = [card({ id: 12 })];
    await render();
    const id = container.querySelector(".autolist__row .autoid")?.textContent;
    expect(id).toBe(tf("auto.id", { id: 12 }));
  });

  it("says nothing of a row's readiness until the check answers", async () => {
    hoisted.automations = [card()];
    await render();
    const row = container.querySelector(".autolist__row")?.textContent ?? "";
    expect(row).not.toContain(t("auto.ready"));
    expect(row).not.toContain(t("auto.notReady"));
  });

  it("opens the build screen in place of the list, with no panel open", async () => {
    hoisted.automations = [card()];
    hoisted.detail = detail();
    hoisted.check = { ready: true, blocks: [] };
    await render();
    await act(async () => { button("Morning round").click(); });
    expect(container.querySelector(".autolist")).toBeNull();
    expect(container.textContent).toContain(t("auto.build.launch"));
    expect(container.textContent).toContain(t("auto.build.picture"));
    expect(container.querySelector(".actpanel")).toBeNull();
  });
});

describe("the panel beside the picture", () => {
  async function open() {
    hoisted.automations = [card()];
    hoisted.detail = detail();
    hoisted.check = { ready: false, blocks: [] };
    await render();
    await act(async () => { button("Morning round").click(); });
  }
  const panelPlace = () => container.querySelector(".actpanel__head .actbuild__sec")?.textContent;

  it("opens the library from the press on an empty picture", async () => {
    await open();
    await act(async () => { button(t("auto.pic.first")).click(); });
    expect(panelPlace()).toBe(t("auto.pic.place"));
    expect(container.textContent).toContain(t("auto.lib.first"));
    expect(container.textContent).toContain(t("auto.lib.make"));
  });

  it("opens the definition's own fields from Edit, and keeps them open on a second press", async () => {
    await open();
    await act(async () => { button(t("auto.build.edit")).click(); });
    expect(panelPlace()).toBe(t("auto.build.edit"));
    expect(container.textContent).toContain(t("auto.about.remove"));
    await act(async () => { button(t("auto.build.edit")).click(); });
    expect(panelPlace()).toBe(t("auto.build.edit"));
  });

  it("puts the ID on the head, and in Edit with the line the terminal takes it in", async () => {
    await open();
    expect(container.querySelector(".actbuild__head .autoid")?.textContent).toBe(tf("auto.id", { id: 7 }));
    await act(async () => { button(t("auto.build.edit")).click(); });
    expect(container.querySelector(".actpanel")?.textContent).toContain("amenbo automation start 7");
  });

  it("closes from its own ×", async () => {
    await open();
    await act(async () => { button(t("auto.build.edit")).click(); });
    await act(async () => {
      container.querySelector<HTMLButtonElement>(".actpanel__close")!.click();
    });
    expect(container.querySelector(".actpanel")).toBeNull();
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

  /** One reason as the check hands it over: its code, its values, and core's English under them. */
  function reason(code: string, fields: Record<string, string> = {}): AutomationLaunchBlockDto {
    return { code, message_en: `English for ${code}`, fields };
  }

  it("puts every reason into words a person can act on", async () => {
    // Every arm core's check can answer with (`amenbo_core::ops::automation_run::Unmet`), so a reason
    // added there without words on this side is caught here rather than on somebody's screen.
    const given = [
      reason("not_ready_automation_no_entry"),
      reason("not_ready_automation_entry_takes_no_task", { step: "Read" }),
      reason("not_ready_automation_open_exit", { step: "Read", exit: "again" }),
      reason("not_ready_automation_open_exit_unnamed", { step: "Read" }),
      reason("not_ready_automation_unwired_input", { step: "Write", port: "folder" }),
      reason("not_ready_automation_unanswered_cfg", { step: "Write", cfg: "filter" }),
      reason("not_ready_automation_agent_missing", { step: "Write", agent: "codex-cli" }),
      reason("not_ready_automation_model_missing", { step: "Write", model: "opus-9" }),
    ];
    await open({ ready: false, blocks: given });
    // The same sentences the press's refusal is written from — this list and that one are one set of
    // words, which is the whole reason the check hands over a code (`AMB-T-5287`).
    expect(blocks()).toEqual(given.map((one) => errSentence(one)));
    // Nothing is left standing as a bare code, and nothing falls through to core's English: a reason
    // nobody wrote a sentence for would ship as `not_ready_automation_no_entry` at the reader.
    for (const one of given) {
      expect(blocks()).not.toContain(one.code);
      expect(blocks()).not.toContain(one.message_en);
    }
    // And every value the sentence is about reaches it, rather than leaving `{step}` on the screen.
    expect(blocks().some((line) => line?.includes("codex-cli"))).toBe(true);
    expect(blocks().every((line) => line && !line.includes("{"))).toBe(true);
  });

  it("holds the button shut while anything is in the way", async () => {
    await open({ ready: false, blocks: [reason("not_ready_automation_no_steps")] });
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

  it("says nothing of its own once the launch lands", async () => {
    // What a launch looks like is the pane arriving, which is the workspace's and not this screen's
    // (`../talk/automationStep`). Nothing here stands in for it.
    await open({ ready: true, blocks: [] });
    await act(async () => { button(t("auto.start")).click(); });
    expect(container.querySelector(".auto__notready")).toBeNull();
  });

  /** Press start, with the launch rejecting with this. */
  async function refusedWith(err: unknown) {
    hoisted.launch.mockRejectedValue(err);
    await open({ ready: true, blocks: [] });
    await act(async () => { button(t("auto.start")).click(); });
  }

  it("puts a refusal in front of the reader in their own language, not core's English", async () => {
    // The one a person pressing start meets most: the window the run would draw its steps in is not
    // open. It names itself, so the line is written from the dictionary (`AMB-D-413`).
    const refusal = {
      code: "invalid_automation_workspace_closed",
      message_en: "the workspace is closed — a run draws its steps in its panes, so open it and launch again",
    };
    await refusedWith(refusal);
    // The pair is what makes this a test: with no template `errText` hands back the English, and the
    // second half would then be asserting that the English is both on the screen and not.
    expect(container.textContent).toContain(errText(refusal));
    expect(container.textContent).not.toContain(refusal.message_en);
  });

  it("writes the whole of a refusal over a list — the line and every reason under it", async () => {
    // A launch check that passed when the screen drew it and failed at the press: the machine changed
    // in between. Each reason names itself as well, so none of them arrives as English inside the
    // reader's sentence.
    await refusedWith({
      code: "not_ready_automation",
      message_en: "cannot launch 'Morning round': it has no steps",
      fields: { automation: "Morning round" },
      parts: [
        { code: "not_ready_automation_agent_missing", message_en: "x", fields: { step: "Write", agent: "codex-cli" } },
        { code: "not_ready_automation_unanswered_cfg", message_en: "y", fields: { step: "Write", cfg: "filter" } },
      ],
    });
    const line = container.textContent ?? "";
    expect(line).toContain("Morning round");
    expect(line).toContain("codex-cli");
    expect(line).toContain("filter");
    // Nothing is left standing as a bare code: a reason nobody wrote a sentence for would ship as
    // `not_ready_automation_agent_missing` at the reader.
    expect(line).not.toContain("not_ready_automation");
  });

  it("falls back to core's English where the refusal names no sentence of its own", async () => {
    // The family code, which covers dozens of sentences and so holds no template. The reader gets what
    // core said rather than a blank — this is the road every uncoded refusal still takes.
    await refusedWith({ code: "invalid", message_en: "something core has no code for" });
    expect(container.textContent).toContain("something core has no code for");
  });
});
