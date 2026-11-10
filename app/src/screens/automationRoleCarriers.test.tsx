// @vitest-environment jsdom
// The table under the build screen's picture: each step of the picked role's round with the agent and
// model that carry it out there.
//
// What these guard: **the rows go in the picture's order**, not the order the placements are listed
// in; **a step no agent carries out is not a row**, and **neither is a step that asks a person**; **a
// step that chose for itself is marked on both its agent and its model**, and one that fell back to the
// role or to Amenbo is not; **a step with no model says the agent's own default**; **the order things
// are decided in is said under the table**; and **a round with no such step draws no table**.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type {
  AutomationDetailDto,
  AutomationPlacementDto,
  AutomationPlacementStepDto,
} from "../bindings/bindings";

vi.mock("../core/boundFolders", () => ({
  useBoundFolders: () => ({ all: [], live: [], answered: true }),
}));
// `inTauri` is false here, so the machine is not asked which agents it has: an agent is named by its id.
vi.mock("../core/ipc", () => ({ invoke: () => Promise.resolve(null) }));

import { t, tf } from "../core/i18n";
import { automationGraph } from "./automationLayout";
import { AutomationRoleCarriers, roleCarrierRows } from "./AutomationRoleCarriers";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

function placement(id: number, roleId: number | undefined, steps: AutomationPlacementStepDto[]): AutomationPlacementDto {
  return {
    id,
    name: `Action ${id}`,
    actionId: 100 + id,
    global: false,
    roleId,
    prompt: "",
    interactive: false,
    reportToTask: false,
    showHistory: true,
    showNotes: true,
    showDecisions: true,
    showComments: true,
    exits: [{ id: id * 100, name: "完了", outputs: [] }],
    inputs: [],
    settings: [],
    steps,
  };
}

// The round of Alice starts at placement 2 and goes on to placement 1, though 1 is listed first.
// Placement 3 is Bob's.
function detail(): AutomationDetailDto {
  return {
    id: 7,
    projectId: 1,
    name: "Morning round",
    notes: "",
    archived: false,
    triggers: [],
    roles: [
      { id: 1, name: "Alice", headcount: 1, startsBy: "task", agent: "claude-code", model: "opus", entryPlacementId: 2, passes: [] },
      { id: 2, name: "Bob", headcount: 1, startsBy: "task", agent: "codex", passes: [] },
    ],
    placements: [
      placement(1, 1, [
        {
          stepId: 11,
          name: "Read the diff",
          interactive: false,
          script: false,
          builtin: false,
          agent: "claude-code",
          model: "haiku",
          // The role's agent written down again, and a model of its own: only the model is the step's.
          carrier: {
            agent: "claude-code",
            model: "haiku",
            chosenAt: { kind: "step" },
            agentByStep: false,
            modelByStep: true,
          },
        },
      ]),
      placement(2, 1, [
        {
          stepId: 21,
          name: "Write it",
          interactive: false,
          script: false,
          builtin: false,
          carrier: {
            agent: "claude-code",
            model: "opus",
            chosenAt: { kind: "role", roleId: 1 },
            agentByStep: false,
            modelByStep: false,
          },
        },
        // A script step: no agent carries it out, so it has no carrier.
        { stepId: 22, name: "Run the tests", interactive: false, script: true, builtin: false },
        {
          stepId: 23,
          name: "Ask Amenbo",
          interactive: false,
          script: false,
          builtin: false,
          carrier: { agent: "gemini", chosenAt: { kind: "amenbo" }, agentByStep: false, modelByStep: false },
        },
        // A step that asks a person: an agent opens it, but it is not a row.
        {
          stepId: 24,
          name: "Pick a plan",
          interactive: true,
          script: false,
          builtin: false,
          carrier: {
            agent: "claude-code",
            model: "opus",
            chosenAt: { kind: "role", roleId: 1 },
            agentByStep: false,
            modelByStep: false,
          },
        },
      ]),
      placement(3, 2, [
        {
          stepId: 31,
          name: "Review",
          interactive: false,
          script: false,
          builtin: false,
          carrier: { agent: "codex", chosenAt: { kind: "role", roleId: 2 }, agentByStep: false, modelByStep: false },
        },
      ]),
    ],
    edges: [{ id: 5, fromId: 2, exitName: "完了", toId: 1, ends: "go" }],
    wires: [],
    maxHeadcount: 10,
    maxRunLimit: 100,
    failureValues: [],
    heldBy: [],
    unsaved: false,
    saveBlocks: [],
    defaultAgent: "codex",
  };
}

async function draw(automation: AutomationDetailDto, roleId: number) {
  await act(async () =>
    root.render(createElement(AutomationRoleCarriers, { automation, graph: automationGraph(automation, roleId) })),
  );
}

const cells = () =>
  [...container.querySelectorAll("tbody tr")].map((row) =>
    [...row.querySelectorAll("td")].map((cell) => cell.textContent),
  );

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the steps of a role's round", () => {
  it("go in the picture's order and leave out the script steps and the steps that ask a person", () => {
    const automation = detail();
    expect(roleCarrierRows(automation, automationGraph(automation, 1)).map((row) => row.name)).toEqual([
      "Write it",
      "Ask Amenbo",
      "Read the diff",
    ]);
    expect(roleCarrierRows(automation, automationGraph(automation, 2)).map((row) => row.name)).toEqual(["Review"]);
    expect(roleCarrierRows(automation, null)).toEqual([]);
  });
});

describe("the table", () => {
  it("marks only the cell a step set apart for itself, and says how they are decided", async () => {
    await draw(detail(), 1);
    const default_ = t("auto.step.modelDefault");
    expect(cells()).toEqual([
      ["Write it", "claude-code", "opus"],
      ["Ask Amenbo", "gemini", default_],
      ["Read the diff", "claude-code", "haiku ●"],
    ]);
    const kinds = [...container.querySelectorAll("tbody tr")].map((row) =>
      [...row.querySelectorAll("td")].slice(1).map((cell) => cell.className),
    );
    const fallen = "autocarriers__cell--fallen";
    expect(kinds).toEqual([
      [fallen, fallen],
      [fallen, fallen],
      [fallen, "autocarriers__cell--step"],
    ]);
    expect([...container.querySelectorAll("th")].map((one) => one.textContent)).toEqual([
      t("auto.build.carrierStep"),
      t("auto.build.carrierAgent"),
      t("auto.build.carrierModel"),
    ]);
    expect(container.querySelector(".autocarriers__mark")?.getAttribute("title")).toBe(t("auto.build.carrierByStep"));
    expect(container.querySelector(".autocarriers__hint")?.textContent).toBe(
      tf("auto.build.carrierHint", { agent: "codex", model: default_ }),
    );
  });

  it("names Amenbo's own default agent as unset while nobody has chosen one", async () => {
    const automation = detail();
    delete automation.defaultAgent;
    await draw(automation, 1);
    expect(container.querySelector(".autocarriers__hint")?.textContent).toBe(
      tf("auto.build.carrierHint", { agent: t("auto.pic.stepUnset"), model: t("auto.step.modelDefault") }),
    );
  });

  it("is not drawn for a round with no step an agent carries out", async () => {
    const automation = detail();
    automation.placements = automation.placements.map((one) => ({
      ...one,
      steps: one.steps.map(({ stepId, name, interactive }) => ({ stepId, name, interactive, script: true, builtin: false })),
    }));
    await draw(automation, 1);
    expect(container.innerHTML).toBe("");
  });
});
