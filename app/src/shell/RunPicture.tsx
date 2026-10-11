// **A run's pane, drawn as its picture** (`AMB-T-5775`).
//
// What a reader watching a run wants to know is where it is and where it has been, and a terminal's
// output says neither. So the pane's body can be the automation's picture instead, drawn the way the
// build screen draws it (`../screens/AutomationPicture`), with the lap the run is walking on the task
// it is working lit, and the step under way blinking in the box it is in — or, while the run waits for
// a task it can take, dashed with a spinner over it.
//
// **The trail is core's to read** (`amenbo_core::ops::automation_run::trail`): which spots it passed, by
// which lines, and where it stands. It is read again whenever the run moves (`useRunTrail`), and a
// run that takes its next task starts a fresh one — what was lit for the task before goes out.
//
// **The box under way is grown, with its action's own picture inside** (`AMB-T-6505`). A box that only
// blinks says which action the run is in and not which of its steps, so the action's picture is drawn
// in it as its build screen draws it, lit by the same rule as the boxes outside, and the step under way
// blinks there instead. The steps are core's to read too: the ones it passed inside the spot it is in,
// and the step it stands at. Once it leaves that spot, the box is drawn as it was.
//
// **The step under way is the one picked** — or, where the action has no picture to draw, the box.
// Picking it is what brings it to the middle as the run reaches it, and a reader who scrolls away
// meanwhile is left where they went: that is the picture's own rule (`../screens/AutomationPicture`).
// Nothing opens beside it — picking is only the border. The picture inside is the action as it is now,
// which may not be the version the spot stands on: a step it has no longer is left unlit, not dropped.
import { AutomationPicture, type PicTrail } from "../screens/AutomationPicture";
import { actionGraph, automationGraph } from "../screens/automationLayout";
import { useAutomation, useAutomationAction, useRunTrail } from "../core/automations";
import type { AutomationRunTrailDto } from "../bindings/bindings";
import type { Say } from "../talk/nameplate";

/**
 * The trail as the picture reads it: the boxes and lines passed on the lap under way, and the box
 * under way.
 *
 * **Only the lap under way is lit** (`AMB-T-5825`). A run that goes back to a spot it passed starts a
 * lap there, and what it walked before that is left dark — lit, it would read as where this lap has
 * been. The line it came back by stays lit, as the way into the lap.
 */
export function picTrail(trail: AutomationRunTrailDto | null): PicTrail {
  return { ...lapLit((trail?.passed ?? []).map((pass) => ({ box: pass.placement, edge: pass.edge }))), at: trail?.at };
}

/**
 * The same, on the picture of the action the run is in: the steps and lines it passed there on the
 * lap under way, by the same rule, and the step under way.
 */
export function stepTrail(trail: AutomationRunTrailDto | null): PicTrail {
  return { ...lapLit((trail?.steps ?? []).map((pass) => ({ box: pass.step, edge: pass.edge }))), at: trail?.step };
}

/** One box passed, and the line it was left by. */
type Pass = { box?: number; edge?: number };

/** The boxes and lines lit of the passes, in the order they were passed. */
function lapLit(passed: readonly Pass[]): Pick<PicTrail, "boxes" | "edges"> {
  const boxes = new Set<number>();
  const edges = new Set<number>();
  const back = lapStart(passed);
  const cameBy = passed[back - 1]?.edge;
  if (cameBy !== undefined) edges.add(cameBy);
  for (const pass of passed.slice(back)) {
    if (pass.box !== undefined) boxes.add(pass.box);
    if (pass.edge !== undefined) edges.add(pass.edge);
  }
  return { boxes, edges };
}

/**
 * Where the lap under way starts. Walking the passes in order, a pass onto a box the lap has already
 * passed starts the next lap.
 */
function lapStart(passed: readonly Pass[]): number {
  let lap = new Set<number>();
  let start = 0;
  passed.forEach((pass, i) => {
    if (pass.box === undefined) return;
    if (lap.has(pass.box)) {
      start = i;
      lap = new Set();
    }
    lap.add(pass.box);
  });
  return start;
}

export function RunPicture({ run }: { run: Say }) {
  const automation = useAutomation(run.automationId);
  const walked = useRunTrail(run.run);
  const waiting = run.state?.waiting === true;
  const trail = { ...picTrail(walked), waiting };
  const actionId = automation?.placements.find((placement) => placement.id === trail.at)?.actionId ?? null;
  const action = actionGraph(useAutomationAction(actionId));
  const inside =
    trail.at === undefined || action === null
      ? undefined
      : { boxId: trail.at, graph: action, trail: { ...stepTrail(walked), waiting } };
  return (
    <div className="runpic">
      <AutomationPicture graph={automationGraph(automation)} selectedBoxId={trail.at} trail={trail} inside={inside} />
    </div>
  );
}
