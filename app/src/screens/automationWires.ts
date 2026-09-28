// What may fill one of a box's inputs, and what is filling it now (`AMB-T-5256`).
//
// **A wire is picked, not drawn.** What a reader is choosing is which way out of which earlier box
// hands this input its value, and that is a list of names — dragging a line between two points on a
// picture asks them to aim at something the picture worked out for itself.
//
// **Either picture is read the same way** (`AMB-D-949`): what is handed over is the shape both are
// laid out as (`./automationLayout`), so the same list answers for an action placed on an automation
// and for a step inside an action.
//
// **What does not fit is not offered.** A wire carries one kind into the same kind
// (`amenbo_core::ops::automation::wire_add`), so an output of another kind in the list would be a
// choice that is refused the moment it is made.
import type { AutomationPortDto, AutomationWireDto } from "../bindings/bindings";
import { ACTION_BOUNDARY, pictureOrder, type PicGraph } from "./automationLayout";

/** One thing that could fill an input: the way out of the box that hands it on. */
export type WireChoice = {
  /** What tells two choices apart, and what a control hands back when one is picked. */
  key: string;
  boxId: number;
  boxName: string;
  /** The built-in the box is, by its key — its words are drawn in the screen's language (`builtinWord`). */
  builtin?: string;
  /** The way out it leaves by. Absent on what the action itself was handed, which leaves by none. */
  exitName?: string;
  portName: string;
};

/** What one choice is called, which is also what tells two of them apart. */
export function choiceKey(boxId: number, exitName: string | undefined, portName: string): string {
  return `${boxId}\u0000${exitName ?? ""}\u0000${portName}`;
}

/**
 * Every output that could fill this input, in the order the boxes were added in.
 *
 * A box's own ways out are left off: what it hands on is read after it has run, and by then it is
 * past the point of taking anything in. **A wire already drawn from one of them into this input stays
 * on**, as that one choice alone: the picture draws it, and a list without it would call the input
 * unwired while the wire is there.
 *
 * **Inside an action, the action's own inputs come first** — what the placement standing on it was
 * handed, passed in from the action itself (`ACTION_BOUNDARY`). `selfName` is what that end is called
 * on screen; the list carries it so each choice reads whole.
 */
export function wireChoices(
  graph: PicGraph,
  boxId: number,
  input: AutomationPortDto,
  selfName = "",
): WireChoice[] {
  const out: WireChoice[] = [];
  for (const port of graph.boundary?.inputs ?? []) {
    if (port.kind !== input.kind) continue;
    out.push({
      key: choiceKey(ACTION_BOUNDARY, undefined, port.name),
      boxId: ACTION_BOUNDARY,
      boxName: selfName,
      portName: port.name,
    });
  }
  const fromSelf = graph.wires.filter(
    (wire) => wire.toId === boxId && wire.toPortName === input.name && wire.fromId === boxId,
  );
  for (const box of graph.boxes) {
    for (const exit of box.exits) {
      for (const port of exit.outputs) {
        if (port.kind !== input.kind) continue;
        if (
          box.id === boxId &&
          !fromSelf.some((wire) => wire.fromExitName === exit.name && wire.fromPortName === port.name)
        ) {
          continue;
        }
        out.push({
          key: choiceKey(box.id, exit.name, port.name),
          boxId: box.id,
          boxName: box.name,
          builtin: box.builtin,
          exitName: exit.name,
          portName: port.name,
        });
      }
    }
  }
  return out;
}

/**
 * **The output a new input is joined to as it is declared** (`AMB-T-5799`): one of the same name and
 * kind, on a way out of a step a run comes to before this one — the nearest of them, the one with the
 * highest number on the picture. Nothing where no step before it hands one on, and the input then
 * starts with nothing reaching it, as it always has.
 *
 * "Before" is a line leading down from that step to this one, not a lower number: a step on a branch
 * beside this one is numbered first, and hands nothing to a run that never passes it.
 */
export function sameNameBefore(
  graph: PicGraph,
  boxId: number,
  input: Pick<AutomationPortDto, "name" | "kind">,
): WireChoice | undefined {
  const order = pictureOrder(graph);
  let nearest: WireChoice | undefined;
  for (const one of wireChoices(graph, boxId, { ...input, required: false })) {
    if (one.boxId === ACTION_BOUNDARY || one.boxId === boxId || one.portName !== input.name) continue;
    if (!order.goesBack(boxId, one.boxId)) continue;
    const no = order.numberOf.get(one.boxId) ?? 0;
    if (nearest === undefined || no > (order.numberOf.get(nearest.boxId) ?? 0)) nearest = one;
  }
  return nearest;
}

/**
 * The wire filling this input now, or nothing where none is.
 *
 * Several wires may land on one input — which of them a run reads is its own to decide — so what the
 * control shows is the last one drawn, which is the one a reader just picked.
 */
export function wireInto(
  graph: PicGraph,
  boxId: number,
  portName: string,
): AutomationWireDto | undefined {
  const all = graph.wires.filter((one) => one.toId === boxId && one.toPortName === portName);
  return all[all.length - 1];
}

/**
 * **What could fill one output a way out of the action hands on** — an output of the same kind, on a
 * way out of a step that leaves the action by that same way out. Core asks for the leaving line first
 * (`amenbo_core::ops::automation::wire_add`), so a step that does not leave by it is not offered.
 */
export function boundaryChoices(
  graph: PicGraph,
  exitName: string,
  port: AutomationPortDto,
): WireChoice[] {
  const out: WireChoice[] = [];
  for (const edge of graph.edges) {
    if (edge.ends !== "exit" || edge.exitTo !== exitName) continue;
    const box = graph.boxes.find((one) => one.id === edge.fromId);
    const exit = box?.exits.find((one) => one.name === edge.exitName);
    if (box === undefined || exit === undefined) continue;
    for (const one of exit.outputs) {
      if (one.kind !== port.kind) continue;
      out.push({
        key: choiceKey(box.id, exit.name, one.name),
        boxId: box.id,
        boxName: box.name,
        builtin: box.builtin,
        exitName: exit.name,
        portName: one.name,
      });
    }
  }
  return out;
}

/** The wire filling one output of a way out of the action now, or nothing where none is. */
export function wireOutOf(
  graph: PicGraph,
  exitName: string,
  portName: string,
): AutomationWireDto | undefined {
  const all = graph.wires.filter(
    (wire) =>
      wire.toId === ACTION_BOUNDARY &&
      wire.toPortName === portName &&
      graph.edges.some(
        (edge) =>
          edge.ends === "exit" &&
          edge.exitTo === exitName &&
          edge.fromId === wire.fromId &&
          edge.exitName === wire.fromExitName,
      ),
  );
  return all[all.length - 1];
}

/** One input of a box as its card says it: the name, and which box hands it its value. */
export type CardInput = {
  name: string;
  /** The box the wire into it comes from — `ACTION_BOUNDARY` for what the action itself was handed.
   *  Absent where no wire reaches it. */
  from?: number;
};

/**
 * **What a step's card on an action's picture says it takes in and hands on** (`AMB-T-5798`) — its
 * inputs, each with the box its value comes from, and the outputs of its ways out, each name once.
 * The card writes them as words, not lines (`AMB-D-1001`): a reader sees what flows through a step
 * without opening its panel, and the picture keeps no line running across the boxes.
 */
export function cardIo(graph: PicGraph, boxId: number): { inputs: CardInput[]; outputs: string[] } {
  const box = graph.boxes.find((one) => one.id === boxId);
  if (box === undefined) return { inputs: [], outputs: [] };
  const inputs = box.inputs.map((port) => ({ name: port.name, from: wireInto(graph, boxId, port.name)?.fromId }));
  const outputs = [...new Set(box.exits.flatMap((exit) => exit.outputs.map((port) => port.name)))];
  return { inputs, outputs };
}
