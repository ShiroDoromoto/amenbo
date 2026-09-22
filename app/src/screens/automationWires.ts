// What may fill one of a step's inputs, and what is filling it now (`AMB-T-5256`).
//
// **A wire is picked, not drawn.** What a reader is choosing is which way out of which earlier step
// hands this input its value, and that is a list of names — dragging a line between two points on a
// picture asks them to aim at something the picture worked out for itself.
//
// **What does not fit is not offered.** A wire carries one kind into the same kind
// (`amenbo_core::ops::automation::wire_add`), so an output of another kind in the list would be a
// choice that is refused the moment it is made.
import type { AutomationDetailDto, AutomationPortDto, AutomationWireDto } from "../bindings/bindings";

/** One thing that could fill an input: the way out of the step that hands it on. */
export type WireChoice = {
  /** What tells two choices apart, and what a control hands back when one is picked. */
  key: string;
  stepId: number;
  stepName: string;
  /** The way out it leaves by. Absent is the unnamed one. */
  exitName?: string;
  portName: string;
};

/** What one choice is called, which is also what tells two of them apart. */
export function choiceKey(stepId: number, exitName: string | undefined, portName: string): string {
  return `${stepId}\u0000${exitName ?? ""}\u0000${portName}`;
}

/**
 * Every output that could fill this input, in the order the steps were placed in.
 *
 * A step's own ways out are left off: what a step hands on is read after it has run, and by then it
 * is past the point of taking anything in.
 */
export function wireChoices(
  detail: AutomationDetailDto,
  stepId: number,
  input: AutomationPortDto,
): WireChoice[] {
  const out: WireChoice[] = [];
  for (const step of detail.steps) {
    if (step.id === stepId) continue;
    for (const exit of step.exits) {
      for (const port of exit.outputs) {
        if (port.kind !== input.kind) continue;
        out.push({
          key: choiceKey(step.id, exit.name, port.name),
          stepId: step.id,
          stepName: step.name,
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
  stepId: number,
  portName: string,
): AutomationWireDto | undefined {
  const all = detail.wires.filter((one) => one.toStepId === stepId && one.toPortName === portName);
  return all[all.length - 1];
}
