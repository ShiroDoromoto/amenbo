// The run the roles panes draw: an automation of twelve boxes in a row, the run in the sixth, and
// its roles with the round under way in one of them. That is about the size of the runs the panes
// are watched on.
import type {
  AutomationDetailDto,
  AutomationEdgeDto,
  AutomationPlacementDto,
  TaskRoleDto,
} from "@app/bindings/bindings";
import type { Say } from "@app/talk/nameplate";

const BOXES = 12;
const AT_BOX = 6;
const ROLES = 8;
const AT_ROLE = 3;

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

/**
 * The automation's roles: eight, the round under way in the third. That is about as many as an
 * automation is split into, and enough for the bar to hold more than one icon.
 */
export const roles: TaskRoleDto[] = Array.from({ length: ROLES }, (_, i) => ({
  id: 200 + i,
  name: `role-${i + 1}`,
  icon: i % 2 === 0 ? "robot" : null,
  automationId: automation.id,
  automationName: automation.name,
}));

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
    task: { ref: "AMB-T-1", title: "Fix the search filter", seq: 1 },
    round: { role: 200 + AT_ROLE - 1, startedAt: Date.now() },
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
