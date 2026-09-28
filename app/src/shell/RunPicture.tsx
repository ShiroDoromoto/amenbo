// **A run's pane, drawn as its picture** (`AMB-T-5775`).
//
// What a reader watching a run wants to know is where it is and where it has been, and a terminal's
// output says neither. So the pane's body can be the automation's picture instead, drawn the way the
// build screen draws it (`../screens/AutomationPicture`), with what the run has walked on the task it
// is working lit, and the box under way blinking.
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

/** The trail as the picture reads it: the boxes and lines passed, and the box under way. */
export function picTrail(trail: AutomationRunTrailDto | null): PicTrail {
  const boxes = new Set<number>();
  const edges = new Set<number>();
  for (const pass of trail?.passed ?? []) {
    if (pass.placement !== undefined) boxes.add(pass.placement);
    if (pass.edge !== undefined) edges.add(pass.edge);
  }
  return { boxes, edges, at: trail?.at };
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
