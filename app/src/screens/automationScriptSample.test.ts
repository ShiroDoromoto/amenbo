// The `input.json` and `output.json` a script step's panel shows, built from what the step declares
// (`AMB-D-1016`).
//
// What these guard: **the shapes are core's** — `{"version":1,"ins":{…}}` in, and
// `{"version":1,"exit":…,"outs":{…},"report":…}` out — with **every value a string**, a task its
// `AMB-T-<n>`; **the error way out has no sample**, a program never naming it; **the task the run
// works is not offered as an output**, core refusing it from a script; and **the outputs a way out
// cannot leave out are named beside it**.
import { describe, expect, it } from "vitest";
import { inputSample, outputSamples } from "./automationScriptSample";

describe("input.json", () => {
  it("names every input declared, a string each, under version 1", () => {
    expect(
      JSON.parse(
        inputSample([
          { name: "run ID", kind: "value", required: true },
          { name: "notes", kind: "file", required: false },
          { name: "task", kind: "task_take", required: true },
        ]),
      ),
    ).toEqual({ version: 1, ins: { "run ID": "…", notes: "/…/in/…", task: "AMB-T-…" } });
  });

  it("holds an empty ins where nothing is declared", () => {
    expect(JSON.parse(inputSample([]))).toEqual({ version: 1, ins: {} });
  });
});

describe("output.json", () => {
  const samples = outputSamples([
    { id: 1, name: "完了", outputs: [] },
    {
      id: 2,
      name: "red",
      outputs: [
        { name: "failures", kind: "file", required: true },
        { name: "URL", kind: "value", required: false },
        { name: "made", kind: "task_make", required: true },
        { name: "taken", kind: "task_take", required: true },
      ],
    },
    { id: 3, name: "*", outputs: [] },
  ]);

  it("draws one for each way out but the error one", () => {
    expect(samples.map((one) => one.exit)).toEqual(["完了", "red"]);
  });

  it("names the way out and every output it hands on but the task the run works", () => {
    expect(JSON.parse(samples[0].text)).toEqual({ version: 1, exit: "完了", outs: {}, report: "…" });
    expect(JSON.parse(samples[1].text)).toEqual({
      version: 1,
      exit: "red",
      outs: { failures: "…", URL: "…", made: "AMB-T-…" },
      report: "…",
    });
  });

  it("says which outputs may not be left out", () => {
    expect(samples[0].required).toEqual([]);
    expect(samples[1].required).toEqual(["failures", "made"]);
  });
});
