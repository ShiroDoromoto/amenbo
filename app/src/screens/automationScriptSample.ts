// What a script step's program is handed and what it writes back, drawn as a sample from what the
// step declares (`AMB-D-1016`).
//
// **The shapes are core's own**, not a sketch of them: `input.json` is `{"version":1,"ins":{…}}`
// (`amenbo_core::ops::automation_script`), and `output.json` is `{"version":1,"exit":…,"outs":{…},
// "report":…}` as `amenbo_core::ops::automation_report` reads it. Every value on both sides is a
// string — a file in is the path it was written to, a file out is the name it was left under beside
// `output.json`, and a task is its `AMB-T-<n>`.
//
// **The error way out has no sample**: a program never names it, it is where the step leaves when the
// program could not say anything at all.
//
// **The task the run works is not an output a script can hand on** — core refuses it in `output.json`
// — so it is left out of the sample rather than drawn as something to write.
import type { AutomationExitDto, AutomationPortDto } from "../bindings/bindings";
import { ERROR_EXIT } from "./automationLayout";

/** The JSON text of one sample, laid out as a reader copies it. */
const json = (value: unknown) => JSON.stringify(value, null, 2);

/** What a value stands in for, by the kind of the port it goes through. */
function inSample(kind: AutomationPortDto["kind"]): string {
  switch (kind) {
    case "file":
      return "/…/in/…";
    case "task_take":
    case "task_make":
      return "AMB-T-…";
    default:
      return "…";
  }
}

/** `input.json` as a step with these inputs is handed it. */
export function inputSample(inputs: AutomationPortDto[]): string {
  const ins: Record<string, string> = {};
  for (const one of inputs) ins[one.name] = inSample(one.kind);
  return json({ version: 1, ins });
}

/** One way out's `output.json`, and the outputs it may not leave out. */
export interface OutputSample {
  exit: string;
  required: string[];
  text: string;
}

/** `output.json` for each way out a program can name, in the order the ways out are given. */
export function outputSamples(exits: AutomationExitDto[]): OutputSample[] {
  return exits
    .filter((one) => one.name !== ERROR_EXIT)
    .map((exit) => {
      const ports = exit.outputs.filter((one) => one.kind !== "task_take");
      const outs: Record<string, string> = {};
      for (const one of ports) outs[one.name] = one.kind === "task_make" ? "AMB-T-…" : "…";
      return {
        exit: exit.name,
        required: ports.filter((one) => one.required).map((one) => one.name),
        text: json({ version: 1, exit: exit.name, outs, report: "…" }),
      };
    });
}
