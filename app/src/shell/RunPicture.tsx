// **A run's pane, drawn as its picture** (`AMB-T-5775`).
//
// What a reader watching a run wants to know is where it is and where it has been, and a terminal's
// output says neither. So the pane's body can be the automation's picture instead, drawn the way the
// build screen draws it (`../screens/AutomationPicture`), with the lap the run is walking on the task
// it is working lit, and the box under way blinking.
//
// **The trail is core's to read** (`amenbo_core::ops::automation_run::trail`): which spots it passed, by
// which lines, and where it stands. It is read again whenever the run moves (`useRunTrail`), and a
// run that takes its next task starts a fresh one — what was lit for the task before goes out.
//
// **The box under way is the one picked.** Picking it is what brings it to the middle as the run
// reaches it, and a reader who scrolls away meanwhile is left where they went: that is the picture's
// own rule (`../screens/AutomationPicture`). Nothing opens beside it — picking is only the border.
import { AutomationPicture, type PicTrail } from "../screens/AutomationPicture";
import { automationGraph } from "../screens/automationLayout";
import { useAutomation, useRunTrail } from "../core/automations";
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
  const passed = trail?.passed ?? [];
  const boxes = new Set<number>();
  const edges = new Set<number>();
  const back = lapStart(passed);
  const lap = passed.slice(back);
  const cameBy = passed[back - 1]?.edge;
  if (cameBy !== undefined) edges.add(cameBy);
  for (const pass of lap) {
    if (pass.placement !== undefined) boxes.add(pass.placement);
    if (pass.edge !== undefined) edges.add(pass.edge);
  }
  return { boxes, edges, at: trail?.at };
}

/**
 * Where the lap under way starts. Walking the passes in order, a pass onto a spot the lap has already
 * passed starts the next lap.
 */
function lapStart(passed: AutomationRunTrailDto["passed"]): number {
  let lap = new Set<number>();
  let start = 0;
  passed.forEach((pass, i) => {
    if (pass.placement === undefined) return;
    if (lap.has(pass.placement)) {
      start = i;
      lap = new Set();
    }
    lap.add(pass.placement);
  });
  return start;
}

export function RunPicture({ run }: { run: Say }) {
  const automation = useAutomation(run.automationId);
  const trail = picTrail(useRunTrail(run.run));
  return (
    <div className="runpic">
      <AutomationPicture graph={automationGraph(automation)} selectedBoxId={trail.at} trail={trail} />
    </div>
  );
}
