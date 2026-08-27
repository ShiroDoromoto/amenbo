// Which of the two entities a classification axis classifies (`AMB-D-789`). An axis is one mechanism
// serving tasks and decisions alike, and until it could say so it served both whether or not that made
// sense — a project's "occupancy" axis is an exclusive lane on a real device, and a decision record
// occupies no device, yet opening one offered the choice.
//
// This module is the gate every screen reads it through: the board, the cards and the task detail
// offer the task side, the decision pane offers the decision side, and the manager — which is where
// the answer is set — offers every axis. Narrowing takes the axis off a side; it takes no assignment
// away, so a value already answered there stays in the store, meaning nothing.

import type { DimensionDto } from "../bindings/bindings";

type Sided = Pick<DimensionDto, "appliesTo">;

/** Does this axis mean anything on a task? */
export function classifiesTasks(dim: Sided): boolean {
  return dim.appliesTo !== "decision";
}

/** Does this axis mean anything on a decision? */
export function classifiesDecisions(dim: Sided): boolean {
  return dim.appliesTo !== "task";
}

/** The axes one side is offered, in the order they were given. */
export function axesFor<T extends Sided>(side: "task" | "decision", dims: readonly T[]): T[] {
  return dims.filter(side === "task" ? classifiesTasks : classifiesDecisions);
}
