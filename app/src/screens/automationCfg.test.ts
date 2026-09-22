// The shape a setting's answer is kept in, both ways (`AMB-T-5256`).
//
// What these guard: **a task filter is an object naming each part**, which is the shape core and the
// command line both write, so an answer taken on the rows here is one a run can read; **a row that
// takes one answer replaces rather than collects**, `ready` being a yes or a no; **a row emptied
// leaves no key behind**, so a filter nobody pressed anything on is unanswered rather than an object
// with empty lists in it; and **an answer nothing can read draws as nothing**, because a definition
// hand-written from the command line can hold one.
import { describe, expect, it } from "vitest";
import {
  FILTER_ROWS,
  pressed,
  readFilter,
  readNumber,
  readText,
  writeFilter,
  writeNumber,
  writeText,
} from "./automationCfg";

describe("a setting's answer", () => {
  it("reads a folder, a choice and a text back out of the string it is kept as", () => {
    expect(readText(JSON.stringify("/Users/x/work"))).toBe("/Users/x/work");
    expect(readText(undefined)).toBe("");
    expect(readText("not json")).toBe("");
    // A number is not a text: the box for one draws nothing rather than the digits.
    expect(readText("7")).toBe("");
  });

  it("reads a number back, and nothing where the answer is of another shape", () => {
    expect(readNumber("7")).toBe(7);
    expect(readNumber(JSON.stringify("7"))).toBeNull();
    expect(readNumber(undefined)).toBeNull();
  });

  it("writes an emptied box as unanswered rather than as an empty value", () => {
    expect(writeText("")).toBeNull();
    expect(writeNumber("  ")).toBeNull();
    expect(writeNumber("three")).toBeNull();
    expect(writeText("x")).toBe('"x"');
    expect(writeNumber("3")).toBe("3");
  });

  it("keeps a task filter as an object naming each part", () => {
    const filter = { assignee: ["me-ai"], status: ["todo", "in_progress"] };
    expect(writeFilter(filter)).toBe('{"assignee":["me-ai"],"status":["todo","in_progress"]}');
    expect(readFilter(writeFilter(filter) ?? undefined)).toEqual(filter);
  });

  it("draws an unreadable answer, and one of the wrong shape, as nothing pressed", () => {
    expect(readFilter("not json")).toEqual({});
    expect(readFilter(JSON.stringify("todo"))).toEqual({});
    expect(readFilter(JSON.stringify({ status: "todo" }))).toEqual({});
  });

  it("collects on a row that takes several and replaces on the one that takes one", () => {
    const many = pressed(pressed({}, "status", "todo", false), "status", "blocked", false);
    expect(many).toEqual({ status: ["todo", "blocked"] });
    const one = pressed(pressed({}, "ready", "yes", true), "ready", "no", true);
    expect(one).toEqual({ ready: ["no"] });
  });

  it("leaves no key behind on a row emptied, so nothing pressed is unanswered", () => {
    const off = pressed({ status: ["todo"] }, "status", "todo", false);
    expect(off).toEqual({});
    expect(writeFilter(off)).toBeNull();
  });

  it("draws the three rows a person reaches for first, premises taking one answer", () => {
    expect(FILTER_ROWS.map((one) => one.key)).toEqual(["assignee", "status", "ready"]);
    expect(FILTER_ROWS.find((one) => one.key === "ready")?.single).toBe(true);
  });
});
