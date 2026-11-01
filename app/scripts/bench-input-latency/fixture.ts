// The run the picture panes draw: an automation of twelve boxes in a row, the run in the sixth, and
// inside it an action of six steps with the run at the fourth. That is about the size of the runs
// the panes are watched on, and enough for the box under way to be grown with a picture of its own.
import type {
  AutomationActionDetailDto,
  AutomationDetailDto,
  AutomationEdgeDto,
  AutomationPlacementDto,
  AutomationRunTrailDto,
  AutomationStepDto,
} from "@app/bindings/bindings";
import type { Say } from "@app/talk/nameplate";

const BOXES = 12;
const STEPS = 6;
const AT_BOX = 6;
const AT_STEP = 4;

const exits = (id: number) => [
  { id: id * 10, name: "完了", outputs: [] },
  { id: id * 10 + 1, name: "*", outputs: [] },
];

const shown = { reportToTask: false, showHistory: true, showNotes: true, showDecisions: true, showComments: true };

/** One line on to the next box, the error way out ending the run, and the last box done. */
function chain(ids: number[], base: number): AutomationEdgeDto[] {
  return ids.flatMap((id, i) => [
    i + 1 < ids.length
      ? { id: base + id * 2, fromId: id, exitName: "完了", toId: ids[i + 1], ends: "go" as const }
      : { id: base + id * 2, fromId: id, exitName: "完了", ends: "done" as const },
    { id: base + id * 2 + 1, fromId: id, exitName: "*", ends: "halt" as const },
  ]);
}

const boxIds = Array.from({ length: BOXES }, (_, i) => i + 1);
const stepIds = Array.from({ length: STEPS }, (_, i) => 101 + i);
const ACTION_ID = 900 + AT_BOX;

const placements: AutomationPlacementDto[] = boxIds.map((id) => ({
  id,
  name: `step-${id}`,
  actionId: 900 + id,
  global: false,
  prompt: "",
  interactive: false,
  ...shown,
  exits: exits(id),
  inputs: [],
  settings: [],
  steps: [],
}));

export const automation: AutomationDetailDto = {
  id: 3,
  projectId: 1,
  name: "Nightly",
  notes: "",
  entryPlacementId: 1,
  archived: false,
  placements,
  edges: chain(boxIds, 0),
  wires: [],
  heldBy: [],
  unsaved: false,
  saveBlocks: [],
};

const steps: AutomationStepDto[] = stepIds.map((id) => ({
  id,
  name: `part-${id - 100}`,
  prompt: "",
  interactive: false,
  ...shown,
  exits: exits(id),
  inputs: [],
}));

export const action: AutomationActionDetailDto = {
  id: ACTION_ID,
  name: `step-${AT_BOX}`,
  note: "",
  global: false,
  oneAtATime: false,
  usedBy: 1,
  entryStepId: stepIds[0],
  steps,
  edges: chain(stepIds, 1000),
  wires: [],
  exits: exits(ACTION_ID),
  inputs: [],
  settings: [],
  heldBy: [],
  placedOn: [],
  unsaved: false,
  saveBlocks: [],
};

export const trail: AutomationRunTrailDto = {
  passed: boxIds.slice(0, AT_BOX).map((id) => ({
    placement: id,
    status: id === AT_BOX ? ("running" as const) : ("done" as const),
    edge: id === AT_BOX ? undefined : id * 2,
  })),
  at: AT_BOX,
  steps: stepIds.slice(0, AT_STEP).map((id, i) => ({
    step: id,
    status: i + 1 === AT_STEP ? ("running" as const) : ("done" as const),
    edge: i + 1 === AT_STEP ? undefined : 1000 + id * 2,
  })),
  step: stepIds[AT_STEP - 1],
};

/** What a run's pane is handed for the run, made afresh on every draw as the workspace makes it. */
export function say(run: number): Say {
  return {
    automation: automation.name,
    run,
    step: `step-${AT_BOX}`,
    automationId: automation.id,
    placement: AT_BOX,
    box: AT_BOX,
    builtin: false,
    interactive: false,
    by: { kind: "agent", agent: "claude", model: null },
    command: null,
    startedAt: null,
    action: `step-${AT_BOX}`,
    task: null,
    state: {
      status: "running",
      word: "Running",
      pauseRequested: false,
      pauseBeforeNextTask: false,
      pausableBeforeNextTask: false,
      why: null,
      exit: null,
      errorExit: false,
      acknowledged: false,
    },
  };
}
