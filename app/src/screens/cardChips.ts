// What a task card draws of its classification (`AMB-D-650`).
//
// The board's columns are the status and nothing else (`AMB-D-909`), so the card is where the reader sees
// how the work is classified. The answer is not "draw every axis" — axes are unbounded, and `AMB-D-40` kept
// the card surface for the assignee, the due date and the progress. Each axis carries the answer for itself
// instead (`showOnCard`, `AMB-D-651`), and this is the whole of what the board does with that flag: turn the
// axes, their values, and one board's worth of assignments into the chips per task.
//
// It lives apart from BoardScreen so the rule can be read and tested without a board around it.
import type { DimensionDto } from "../bindings/bindings";
import type { DimAssignments } from "../core/filters";

/** One classification a card draws: one value assigned on an axis flagged for the card. */
export type CardChip = {
  dimId: number;
  /** The value's own id. An axis may put several chips on one card, so the axis alone does not name one. */
  valueId: number;
  /** The axis's name. Not drawn on the chip — it names the value in the tooltip. */
  axis: string;
  value: string;
};

/**
 * The chips each task carries, by task id. Tasks with nothing to draw are left out of the map entirely,
 * so a card asks for its own id and gets `undefined` rather than an empty array to test.
 *
 * Axes come out in the order the project lists them, and an axis a task has no value on contributes no
 * chip — a card shows what it was given, never a placeholder for what it was not.
 *
 * An axis that admits several values at once (`AMB-D-826`) draws one chip per value it was given, in the
 * axis's own order rather than the order the assignments were read in, so two cards on the same pair of
 * values read the same way.
 */
export function cardChips(
  dims: DimensionDto[],
  assignments: DimAssignments,
): Record<string, CardChip[]> {
  const shown = dims.filter((d) => d.showOnCard);
  const byTask: Record<string, CardChip[]> = {};
  if (shown.length === 0) return byTask;
  for (const [taskId, assigned] of Object.entries(assignments)) {
    const chips: CardChip[] = [];
    for (const d of shown) {
      const ids = assigned[d.id];
      if (!ids) continue;
      for (const v of d.values) {
        if (ids.includes(v.id)) chips.push({ dimId: d.id, valueId: v.id, axis: d.name, value: v.name });
      }
    }
    if (chips.length > 0) byTask[taskId] = chips;
  }
  return byTask;
}
