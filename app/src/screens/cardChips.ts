// What a task card draws of its classification (`AMB-D-650`).
//
// A board says which axis splits its columns and nothing else, so away from that one axis the cards are
// mute about how the work is classified. The answer is not "draw every axis" — axes are unbounded, and
// `AMB-D-40` kept the card surface for the assignee, the due date and the progress. Each axis carries the
// answer for itself instead (`showOnCard`, `AMB-D-651`), and this is the whole of what the board does with
// that flag: turn the axes, their values, and one board's worth of assignments into the chips per task.
//
// It lives apart from BoardScreen so the rule can be read and tested without a board around it.
import type { DimensionDto } from "../bindings/bindings";
import type { DimAssignments } from "../core/filters";

/** One classification a card draws: the value assigned on an axis flagged for the card. */
export type CardChip = {
  dimId: number;
  /** The axis's name. Not drawn on the chip — it names the value in the tooltip. */
  axis: string;
  value: string;
};

/**
 * The chips each task carries, by task id. Tasks with nothing to draw are left out of the map entirely,
 * so a card asks for its own id and gets `undefined` rather than an empty array to test.
 *
 * `groupingDimId` is the axis splitting the columns, and it is excluded: the column heading over the card
 * already says that value, and repeating it on every card under it spends density for nothing. Axes come
 * out in the order the project lists them, and an axis a task has no value on contributes no chip — a card
 * shows what it was given, never a placeholder for what it was not.
 */
export function cardChips(
  dims: DimensionDto[],
  assignments: DimAssignments,
  groupingDimId: number | null,
): Record<string, CardChip[]> {
  const shown = dims.filter((d) => d.showOnCard && d.id !== groupingDimId);
  const byTask: Record<string, CardChip[]> = {};
  if (shown.length === 0) return byTask;
  for (const [taskId, assigned] of Object.entries(assignments)) {
    const chips: CardChip[] = [];
    for (const d of shown) {
      const name = d.values.find((v) => v.id === assigned[d.id])?.name;
      if (name) chips.push({ dimId: d.id, axis: d.name, value: name });
    }
    if (chips.length > 0) byTask[taskId] = chips;
  }
  return byTask;
}
