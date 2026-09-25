// The number the picture gives a box, read from wherever a run is drawn — the "running" tab's row
// (`./RunningTab`) and the row over a run's pane (`../shell/TerminalPane`). Both say which spot of the
// automation a step was opened from, and both say it as the build screen's picture does.
import { useMemo } from "react";
import { useAutomation } from "../core/automations";
import { automationGraph, pictureOrder } from "./automationLayout";

/**
 * **The number the picture gives the box a run's step was opened from** — read off the automation as
 * it stands now, the same walk the picture is numbered by. Absent before a step has opened, and where
 * that spot is no longer on the picture.
 */
export function useBoxNumber(automation: number | null, placement: number | undefined): number | undefined {
  const detail = useAutomation(placement === undefined ? null : automation);
  const order = useMemo(() => {
    const graph = automationGraph(detail);
    return graph === null ? null : pictureOrder(graph);
  }, [detail]);
  return placement === undefined ? undefined : order?.numberOf.get(placement);
}
