// What may fill one of a placement's inputs, and what is filling it now (`AMB-T-5256`).
//
// **A wire is picked, not drawn.** What a reader is choosing is which way out of which earlier spot
// hands this input its value, and that is a list of names — dragging a line between two points on a
// picture asks them to aim at something the picture worked out for itself.
//
// **What does not fit is not offered.** A wire carries one kind into the same kind
// (`amenbo_core::ops::automation::wire_add`), so an output of another kind in the list would be a
// choice that is refused the moment it is made.
import type { AutomationDetailDto, AutomationPortDto, AutomationWireDto } from "../bindings/bindings";

/** One thing that could fill an input: the way out of the placement that hands it on. */
export type WireChoice = {
  /** What tells two choices apart, and what a control hands back when one is picked. */
  key: string;
  placementId: number;
  placementName: string;
  /** The way out it leaves by. Absent is the unnamed one. */
  exitName?: string;
  portName: string;
};

/** What one choice is called, which is also what tells two of them apart. */
export function choiceKey(placementId: number, exitName: string | undefined, portName: string): string {
  return `${placementId}\u0000${exitName ?? ""}\u0000${portName}`;
}

/**
 * Every output that could fill this input, in the order the actions were placed in.
 *
 * A spot's own ways out are left off: what it hands on is read after it has run, and by then it is
 * past the point of taking anything in.
 */
export function wireChoices(
  detail: AutomationDetailDto,
  placementId: number,
  input: AutomationPortDto,
): WireChoice[] {
  const out: WireChoice[] = [];
  for (const placement of detail.placements) {
    if (placement.id === placementId) continue;
    for (const exit of placement.exits) {
      for (const port of exit.outputs) {
        if (port.kind !== input.kind) continue;
        out.push({
          key: choiceKey(placement.id, exit.name, port.name),
          placementId: placement.id,
          placementName: placement.name,
          exitName: exit.name,
          portName: port.name,
        });
      }
    }
  }
  return out;
}

/**
 * The wire filling this input now, or nothing where none is.
 *
 * Several wires may land on one input — which of them a run reads is its own to decide — so what the
 * control shows is the last one drawn, which is the one a reader just picked.
 */
export function wireInto(
  detail: AutomationDetailDto,
  placementId: number,
  portName: string,
): AutomationWireDto | undefined {
  const all = detail.wires.filter((one) => one.toPlacementId === placementId && one.toPortName === portName);
  return all[all.length - 1];
}
