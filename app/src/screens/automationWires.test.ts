// What may fill one of a step's inputs (`AMB-T-5256`).
//
// What these guard: **an output of another kind is never offered**, a wire carrying one kind into the
// same kind; **a step is not offered its own ways out**, which are read after it has run; and **the
// wire drawn last is the one the control shows**, several being allowed to land on one input.
import { describe, expect, it } from "vitest";
import { choiceKey, wireChoices, wireInto } from "./automationWires";
import type {
  AutomationDetailDto,
  AutomationPortDto,
  AutomationPlacementDto,
} from "../bindings/bindings";

function port(name: string, kind: AutomationPortDto["kind"]): AutomationPortDto {
  return { name, kind, required: true };
}

function step(
  id: number,
  name: string,
  outputs: AutomationPortDto[],
  inputs: AutomationPortDto[] = [],
): AutomationPlacementDto {
  return {
    id,
    name,
    actionId: 900 + id,
    prompt: "",
    agent: "claude-code",
    interactive: false,
    reportToTask: false,
    showHistory: true,
    exits: [{ id: id * 10, outputs }],
    inputs,
    settings: [],
  };
}

function detail(steps: AutomationPlacementDto[], wires: AutomationDetailDto["wires"] = []): AutomationDetailDto {
  return {
    id: 7,
    projectId: 1,
    name: "Morning round",
    notes: "",
    preamble: "",
    archived: false,
    placements: steps,
    edges: [],
    wires,
  };
}

describe("what can fill an input", () => {
  const one = detail([
    step(1, "take", [port("note", "value"), port("draft", "file")]),
    step(2, "work", [port("report", "value")], [port("note", "value")]),
  ]);

  it("offers only the outputs that carry the same kind", () => {
    const choices = wireChoices(one, 2, port("note", "value"));
    expect(choices.map((c) => c.portName)).toEqual(["note"]);
  });

  it("does not offer a spot its own ways out", () => {
    const choices = wireChoices(one, 2, port("report", "value"));
    expect(choices.every((c) => c.placementId !== 2)).toBe(true);
    expect(choices.map((c) => c.portName)).toEqual(["note"]);
  });

  it("names a choice by the spot, the way out and the output together", () => {
    const choices = wireChoices(one, 2, port("note", "value"));
    expect(choices[0]!.key).toBe(choiceKey(1, undefined, "note"));
  });

  it("shows the wire drawn last where several land on one input", () => {
    const wired = detail(one.placements, [
      { id: 1, fromPlacementId: 1, fromPortName: "note", toPlacementId: 2, toPortName: "note" },
      { id: 2, fromPlacementId: 1, fromPortName: "other", toPlacementId: 2, toPortName: "note" },
    ]);
    expect(wireInto(wired, 2, "note")?.id).toBe(2);
    expect(wireInto(wired, 2, "missing")).toBeUndefined();
  });
});
