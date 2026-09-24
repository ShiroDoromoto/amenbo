// The shape a setting's answer is kept in, both ways (`AMB-T-5256`).
//
// What these guard: **a task filter is an object naming each part**, which is the shape core and the
// command line both write, so an answer taken on the rows here is one a run can read; **a row that
// takes one answer replaces rather than collects**, `ready` being a yes or a no; **a row emptied
// leaves no key behind**, so a filter nobody pressed anything on is unanswered rather than an object
// with empty lists in it; and **an answer nothing can read draws as nothing**, because a definition
// hand-written from the command line can hold one. **The order an answer is taken in survives a press
// on a row** (`AMB-T-5412`): it is kept beside the parts, and a write that dropped it would put an
// order written on the command line back to the default without anybody choosing that. **The built-in
// that takes a task draws no row for what it always puts on** (`AMB-T-5459`), and every other spot
// draws them all.
import { describe, expect, it } from "vitest";
import {
  FILTER_ROWS,
  filterRows,
  pressed,
  readFilter,
  readNumber,
  readSort,
  readText,
  sortChoices,
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

  it("keeps the order beside the parts, and leaves the default unwritten", () => {
    const filter = { status: ["todo"] };
    expect(writeFilter(filter, "due")).toBe('{"status":["todo"],"sort":"due"}');
    // An answer that names no order is taken highest priority first, so writing that one would only
    // stop the answer following the default.
    expect(writeFilter(filter, "priority")).toBe('{"status":["todo"]}');
    // The order is not a part: it says which task comes first, not which are in.
    expect(readFilter('{"status":["todo"],"sort":"due"}')).toEqual(filter);
    expect(readSort('{"status":["todo"],"sort":"due"}')).toBe("due");
    expect(readSort('{"status":["todo"]}')).toBe("priority");
    expect(readSort(undefined)).toBe("priority");
  });

  it("keeps an order written on the command line through a press on a row", () => {
    const was = '{"status":["todo"],"sort":"-created"}';
    const next = writeFilter(pressed(readFilter(was), "ready", "yes", true), readSort(was));
    expect(next).toBe('{"status":["todo"],"ready":["yes"],"sort":"-created"}');
    // And the list offers it, so it is shown as the order it is rather than as the first of the usual.
    expect(sortChoices("-created")).toEqual(["priority", "due", "created", "-created"]);
    expect(sortChoices("due")).toEqual(["priority", "due", "created"]);
  });

  it("draws the three rows a person reaches for first, premises taking one answer", () => {
    expect(FILTER_ROWS.map((one) => one.key)).toEqual(["assignee", "status", "ready"]);
    expect(FILTER_ROWS.find((one) => one.key === "ready")?.single).toBe(true);
  });

  it("leaves out the rows the built-in that takes a task puts on itself", () => {
    expect(filterRows("take_task").map((one) => one.key)).toEqual(["assignee"]);
    expect(filterRows(undefined)).toBe(FILTER_ROWS);
    expect(filterRows("close_task")).toBe(FILTER_ROWS);
  });
});
