// @vitest-environment jsdom
// The automations screen and the build screen's start press (`AMB-T-5254`). Only the reads are
// stubbed; the tabs, the list, the wording of each reason and what the button does all run for real.
//
// What these guard: **the screen is four tabs, from making to running, and opens on the
// definitions**, so another tab cannot quietly become the one a reader lands on; **a row's start is
// shut while its automation could not be started**, from the same check the build screen reads; **a
// row opens the build screen** in place of the list rather than beside it; **the panel beside the
// picture is opened by what was pressed** — the library by the press on an empty picture, the
// definition's own fields by "Edit" — and by nothing else; **what is in the way is listed under the
// build screen's head, in words**, every reason the check can give taking a line a person can act on
// — including the unnamed way out, which has no name to put in a sentence; and **the start press is
// shut while anything is in the way**, which is the whole of what that list is for.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type {
  AutomationCardDto,
  AutomationDetailDto,
  AutomationLaunchBlockDto,
  AutomationLaunchCheckDto,
  AutomationRunCardDto,
  EveryAutomationCardDto,
} from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  automations: [] as AutomationCardDto[],
  everywhere: [] as EveryAutomationCardDto[],
  detail: null as AutomationDetailDto | null,
  check: null as AutomationLaunchCheckDto | null,
  launch: vi.fn(async (..._args: unknown[]) => ({ run: 1 })),
  stop: vi.fn(async (..._args: unknown[]) => true),
}));

vi.mock("../core/automations", () => ({
  useAutomations: () => hoisted.automations,
  useEveryAutomation: () => hoisted.everywhere,
  useAutomation: () => hoisted.detail,
  useLaunchCheck: () => hoisted.check,
  useAutomationActions: () => [],
  useAutomationBuiltins: () => [],
  useAutomationAction: () => null,
  insertAutomationAction: () => Promise.resolve(),
  placeAutomationAction: () => Promise.resolve(),
  launchAutomation: hoisted.launch,
  stopRun: hoisted.stop,
  // The "running" tab reads it. What that tab draws is its own test (`./runningTab.test.tsx`); here
  // it is the tab being reachable that matters.
  useLiveRuns: () => [],
  useRunHistory: () => ({ runs: [], total: 0, pageSize: 20 }),
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
  return { id: 7, name: "Morning round", notes: "", placements: 3, archived: false, ...over };
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
    heldBy: [],
    ...over,
  };
}

// Where a press that started a run goes (`AMB-T-5530`).
const wentToRun = vi.fn();

async function render(workspaceOpen = true) {
  await act(async () => {
    root.render(createElement(AutomationsScreen, { projectId: 1, workspaceOpen, onGoToRun: wentToRun }));
  });
}

const buttons = () => [...container.querySelectorAll("button")];
function button(label: string): HTMLButtonElement {
  const found = buttons().find((b) => b.textContent?.includes(label));
  if (!found) throw new Error(`no button labelled ${label}`);
  return found;
}
const blocks = () => [...container.querySelectorAll(".autolaunch__blocks li")].map((li) => li.textContent);

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  hoisted.automations = [];
  hoisted.everywhere = [];
  hoisted.detail = null;
  hoisted.check = null;
  hoisted.launch.mockClear();
  hoisted.launch.mockResolvedValue({ run: 1 });
  wentToRun.mockClear();
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the automations screen", () => {
  it("draws the four tabs from making to running, and opens on the definitions", async () => {
    await render();
    const tabs = [...container.querySelectorAll<HTMLButtonElement>(".autotabs__tab")];
    expect(tabs.map((one) => one.textContent)).toEqual([
      t("auto.tab.automations"),
      t("auto.tab.actions"),
      t("auto.tab.running"),
      t("auto.tab.history"),
    ]);
    expect(tabs[0].getAttribute("aria-selected")).toBe("true");
  });

  it("moves to the running tab", async () => {
    await render();
    const tabs = [...container.querySelectorAll<HTMLButtonElement>(".autotabs__tab")];
    await act(async () => { tabs[2].click(); });
    expect(container.querySelector(".auto__list")).toBeNull();
    expect(container.textContent).toContain(t("auto.running.empty"));
  });

  it("moves to the history tab, which is read a page at a time", async () => {
    await render();
    const tabs = [...container.querySelectorAll<HTMLButtonElement>(".autotabs__tab")];
    await act(async () => { tabs[3].click(); });
    expect(container.textContent).toContain(t("auto.history.empty"));
    expect(container.querySelector(".autohist__filter")).not.toBeNull();
  });

  // With none, the press that makes the first one is the whole tab (`AMB-T-5523`).
  it("offers a project with no automations the press that makes the first one, and nothing else", async () => {
    await render();
    expect(container.querySelector(".autolist__first")?.textContent).toContain(t("auto.newFirst"));
    expect(container.querySelector(".autolist")).toBeNull();
    expect(container.querySelector(".auto__empty")).toBeNull();
  });

  it("names each automation and how many actions are placed on it, and starts it from its row", async () => {
    hoisted.automations = [card({ placements: 3 })];
    hoisted.check = { ready: true, blocks: [] };
    await render();
    const rows = [...container.querySelectorAll(".autolist__row")].map((one) => one.textContent ?? "");
    expect(rows[0]).toContain("Morning round");
    expect(rows[0]).toContain(tf("auto.stepCount", { count: 3 }));
    await act(async () => { button(t("auto.start")).click(); });
    expect(hoisted.launch).toHaveBeenCalledWith(7, 1, [], true);
  });

  // Whether it could start is the press's state, and why not is read off it (`AMB-T-5523`).
  it("holds the row's start shut while something is in the way, and says what on hover", async () => {
    hoisted.automations = [card()];
    hoisted.check = { ready: false, blocks: [{ code: "automation_no_entry", message_en: "no entry", fields: {} }] };
    await render();
    expect(button(t("auto.start")).disabled).toBe(true);
    expect(container.querySelector(".autolist__start")?.getAttribute("title")).toBe(
      errSentence({ code: "automation_no_entry", message_en: "no entry", fields: {} }),
    );
  });

  it("folds the archived ones at the end of the list, under their count", async () => {
    hoisted.automations = [card({ id: 8, name: "Nightly", archived: true }), card()];
    await render();
    const names = () => [...container.querySelectorAll(".autolist__row")].map((one) => one.textContent ?? "");
    expect(names()).toHaveLength(1);
    expect(names()[0]).toContain("Morning round");
    const fold = container.querySelector<HTMLButtonElement>(".autolist__fold");
    expect(fold?.textContent).toContain(tf("auto.archivedFold", { count: 1 }));
    await act(async () => { fold?.click(); });
    expect(names()).toHaveLength(2);
    expect(names()[1]).toContain("Nightly");
  });

  it("puts the first line of an automation's notes under its name, and nothing where there are none", async () => {
    hoisted.automations = [card({ notes: "\n  Take one from the inbox and write it.  \nThen show it." }), card({ id: 8 })];
    await render();
    const rows = [...container.querySelectorAll(".autolist__row")];
    expect(rows[0].querySelector(".auto__note")?.textContent).toBe("Take one from the inbox and write it.");
    expect(rows[1].querySelector(".auto__note")).toBeNull();
  });

  it("leads each row with the automation's ID, the number the terminal names it by", async () => {
    hoisted.automations = [card({ id: 12 })];
    await render();
    const id = container.querySelector(".autolist__row .autoid")?.textContent;
    expect(id).toBe(tf("auto.id", { id: 12 }));
  });

  it("holds a row's start shut until the check answers", async () => {
    hoisted.automations = [card()];
    await render();
    expect(button(t("auto.start")).disabled).toBe(true);
  });

  it("opens the build screen in place of the list, with no panel open", async () => {
    hoisted.automations = [card()];
    hoisted.detail = detail();
    hoisted.check = { ready: true, blocks: [] };
    await render();
    await act(async () => { button("Morning round").click(); });
    expect(container.querySelector(".autolist")).toBeNull();
    expect(container.querySelector(".actbuild__head")?.textContent).toContain(t("auto.start"));
    expect(container.textContent).toContain(t("auto.build.picture"));
    expect(container.querySelector(".actpanel")).toBeNull();
  });

  // The tabs stay over the build screen (`AMB-T-5419`), with the automation's own tab lit.
  it("keeps the tabs over the build screen, and moves to another tab in one press", async () => {
    hoisted.automations = [card()];
    hoisted.detail = detail();
    hoisted.check = { ready: true, blocks: [] };
    await render();
    await act(async () => { button("Morning round").click(); });
    const tabs = [...container.querySelectorAll<HTMLButtonElement>(".autotabs__tab")];
    expect(tabs[0].getAttribute("aria-selected")).toBe("true");
    await act(async () => { tabs[2].click(); });
    expect(container.textContent).not.toContain(t("auto.build.picture"));
    expect(container.textContent).toContain(t("auto.running.empty"));
  });

  it("goes back to the list from the lit tab, as back does", async () => {
    hoisted.automations = [card()];
    hoisted.detail = detail();
    hoisted.check = { ready: true, blocks: [] };
    await render();
    await act(async () => { button("Morning round").click(); });
    const lit = container.querySelector<HTMLButtonElement>(".autotabs__tab[aria-selected='true']");
    await act(async () => { lit?.click(); });
    expect(container.querySelector(".autolist")).not.toBeNull();
    expect(container.textContent).not.toContain(t("auto.build.picture"));
  });
});

// The sidebar's entrance (`AMB-D-954`): every project's definitions, each with its project; started
// from the row; a press on the row goes to that project rather than opening anything here; and
// nothing is made here, since making one would first ask which project it is for.
describe("the automations screen opened from the sidebar", () => {
  const goTo = vi.fn();
  function everywhere(over: Partial<EveryAutomationCardDto> = {}): EveryAutomationCardDto {
    return { projectId: 3, projectName: "site", card: card(), ...over };
  }
  async function renderEverywhere() {
    await act(async () => {
      root.render(createElement(AutomationsScreen, {
        projectId: null, workspaceOpen: true, onGoToAutomation: goTo, onGoToRun: wentToRun,
      }));
    });
  }
  beforeEach(() => goTo.mockClear());

  it("lists every project's automations, each naming its project, and offers no new one", async () => {
    hoisted.everywhere = [
      everywhere({ projectId: 1, projectName: "amenbo", card: card({ id: 7, name: "Morning round" }) }),
      everywhere({ projectId: 3, projectName: "site", card: card({ id: 9, name: "Publish" }) }),
    ];
    await renderEverywhere();
    const rows = [...container.querySelectorAll(".autolist__row")].map((one) => one.textContent ?? "");
    expect(rows).toHaveLength(2);
    expect(rows[0]).toContain("Morning round");
    expect(rows[0]).toContain("amenbo");
    expect(rows[1]).toContain("site");
    expect(buttons().some((b) => b.textContent === t("auto.new"))).toBe(false);
  });

  it("says no project has one yet, rather than that this project has none", async () => {
    await renderEverywhere();
    expect(container.textContent).toContain(t("auto.emptyEverywhere"));
    expect(container.querySelector(".autolist__first")).toBeNull();
  });

  it("goes to the row's own project to open it, rather than opening it here", async () => {
    hoisted.everywhere = [everywhere()];
    await renderEverywhere();
    await act(async () => { button("Morning round").click(); });
    expect(goTo).toHaveBeenCalledWith(3, 7);
    expect(container.querySelector(".autolist")).not.toBeNull();
  });

  it("starts the row's automation in the row's project", async () => {
    hoisted.everywhere = [everywhere()];
    hoisted.check = { ready: true, blocks: [] };
    await renderEverywhere();
    await act(async () => { button(t("auto.start")).click(); });
    expect(hoisted.launch).toHaveBeenCalledWith(7, 3, [], true);
    expect(goTo).not.toHaveBeenCalled();
  });

  it("goes to the pane of the run it started, in the row's project", async () => {
    hoisted.everywhere = [everywhere()];
    hoisted.check = { ready: true, blocks: [] };
    hoisted.launch.mockResolvedValue({ run: 31 });
    await renderEverywhere();
    await act(async () => { button(t("auto.start")).click(); });
    expect(wentToRun).toHaveBeenCalledWith(3, 31);
  });

  it("holds the start shut while the check has not said it could start", async () => {
    hoisted.everywhere = [everywhere()];
    hoisted.check = { ready: false, blocks: [] };
    await renderEverywhere();
    expect(button(t("auto.start")).disabled).toBe(true);
  });

  it("puts no heading over the run tabs, from the sidebar or a project", async () => {
    await renderEverywhere();
    const tabs = [...container.querySelectorAll<HTMLButtonElement>(".autotabs__tab")];
    await act(async () => { tabs[2].click(); });
    expect(container.querySelector(".autotabs__head")).toBeNull();
    await act(async () => { tabs[3].click(); });
    expect(container.querySelector(".autotabs__head")).toBeNull();
  });
});

describe("arriving on the sidebar with a global action", () => {
  it("opens on that action's build screen, and goes back to the actions tab", async () => {
    await act(async () => {
      root.render(createElement(AutomationsScreen, { projectId: null, openingAction: 4, workspaceOpen: true }));
    });
    const lit = () => container.querySelector<HTMLButtonElement>(".autotabs__tab[aria-selected='true']");
    expect(lit()?.textContent).toBe(t("auto.tab.actions"));
    await act(async () => { button(t("auto.build.back")).click(); });
    const on = container.querySelector<HTMLButtonElement>(".autotabs__tab[aria-selected='true']");
    expect(on?.textContent).toBe(t("auto.tab.actions"));
  });
});

describe("arriving from the sidebar's list", () => {
  it("opens on the build screen of the automation that was pressed", async () => {
    hoisted.automations = [card()];
    hoisted.detail = detail();
    hoisted.check = { ready: true, blocks: [] };
    await act(async () => {
      root.render(createElement(AutomationsScreen, { projectId: 1, opening: 7, workspaceOpen: true }));
    });
    expect(container.querySelector(".autolist")).toBeNull();
    expect(container.textContent).toContain(t("auto.build.picture"));
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
    expect(container.querySelector(".wheremark")?.textContent).toBe(t("auto.lib.here"));
    expect(container.textContent).toContain(t("auto.lib.makeNew"));
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

  // The panel's head is the name, the archive is a switch, and the command is copied rather than
  // explained (`AMB-T-5523`).
  it("heads Edit with the name to write, and offers the archive as a switch", async () => {
    await open();
    await act(async () => { button(t("auto.build.edit")).click(); });
    const name = container.querySelector<HTMLInputElement>(".actpanel__head .actpanel__titleinput");
    expect(name?.value).toBe("Morning round");
    expect(container.querySelector(".actpanel__body textarea")?.getAttribute("placeholder")).toBe(t("auto.about.notesHint"));
    expect(container.querySelector(".actpanel__body input[role='switch']")).not.toBeNull();
    expect(buttons().some((b) => b.textContent === t("auto.about.copy"))).toBe(true);
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

// Whether it can be started is the start press on the head; what is in the way is listed under the
// head only while there is something (`AMB-T-5523`).
describe("the start press on the build screen's head", () => {
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
    expect(blocks()).toHaveLength(1);
  });

  it("offers the button once nothing is in the way, and lists nothing under the head", async () => {
    await open({ ready: true, blocks: [] });
    expect(button(t("auto.start")).disabled).toBe(false);
    expect(container.querySelector(".autolaunch")).toBeNull();
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
    // (`../talk/automationStep`) — the press goes there (below). Nothing here stands in for it.
    await open({ ready: true, blocks: [] });
    await act(async () => { button(t("auto.start")).click(); });
    expect(container.querySelector(".auto__notready")).toBeNull();
  });

  it("goes to the pane of the run it started", async () => {
    hoisted.launch.mockResolvedValue({ run: 31 });
    await open({ ready: true, blocks: [] });
    await act(async () => { button(t("auto.start")).click(); });
    expect(wentToRun).toHaveBeenCalledWith(1, 31);
  });

  it("goes nowhere when the launch is refused", async () => {
    hoisted.launch.mockRejectedValue({ code: "invalid", message_en: "no" });
    await open({ ready: true, blocks: [] });
    await act(async () => { button(t("auto.start")).click(); });
    expect(wentToRun).not.toHaveBeenCalled();
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

describe("an automation a run is going on (AMB-D-961)", () => {
  const goToRun = vi.fn();
  const run: AutomationRunCardDto = {
    run: 31,
    project: 1,
    projectName: "Here",
    automation: 7,
    automationName: "Morning round",
    status: "paused",
    pauseRequested: false,
    waiting: false,
    stepsDone: 2,
    reportWithheld: [],
    acknowledged: false,
  };
  async function openHeld(heldBy: AutomationRunCardDto[] = [run]) {
    hoisted.automations = [card()];
    hoisted.detail = detail({ heldBy });
    hoisted.check = { ready: true, blocks: [] };
    goToRun.mockClear();
    await act(async () => {
      root.render(createElement(AutomationsScreen, { projectId: 1, opening: 7, workspaceOpen: true, onGoToRun: goToRun }));
    });
  }

  it("names the run using it, and goes to its pane from the band", async () => {
    await openHeld();
    expect(container.querySelector(".autoheld")?.textContent).toContain(tf("auto.held.by", { run: 31 }));
    await act(async () => { button(t("auto.held.openPane")).click(); });
    expect(goToRun).toHaveBeenCalledWith(1, 31);
  });

  it("stops the run from the band", async () => {
    hoisted.stop.mockClear();
    await openHeld();
    const stop = container.querySelector<HTMLButtonElement>(".autoheld .btn--danger");
    expect(stop?.textContent).toBe(t("auto.run.stop"));
    await act(async () => { stop?.click(); });
    expect(hoisted.stop).toHaveBeenCalledWith(31);
  });

  it("offers nothing to place, and opens its own fields with them shut", async () => {
    await openHeld();
    expect(buttons().some((b) => b.textContent === t("auto.pic.first"))).toBe(false);
    await act(async () => { button(t("auto.build.edit")).click(); });
    expect(container.querySelector<HTMLFieldSetElement>(".actpanel__body")?.disabled).toBe(true);
    expect(container.querySelector<HTMLButtonElement>(".actpanel__close")?.disabled).toBe(false);
  });

  it("still offers a start, which is not a rewrite", async () => {
    await openHeld();
    expect(button(t("auto.start")).disabled).toBe(false);
  });

  it("says nothing of runs, and holds nothing shut, while none is going", async () => {
    await openHeld([]);
    expect(container.querySelector(".autoheld")).toBeNull();
    expect(buttons().some((b) => b.textContent === t("auto.pic.first"))).toBe(true);
  });
});
