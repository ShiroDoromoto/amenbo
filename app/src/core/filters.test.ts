import { describe, it, expect } from "vitest";
import { filterDimensions, parseRefQuery, passesFilters, selectionKey } from "./filters";
import type { TaskCard } from "../mock/types";

describe("filters: user-defined classifications (unified dimension)", () => {
  const dim = {
    id: 1, name: "カテゴリー", notes: "", role: "none" as const, ordered: false, showOnCard: false, required: false,
    values: [{ id: 11, name: "バグ" }, { id: 12, name: "機能" }],
  };
  const assign = { t1: { 1: 11 }, t2: { 1: 12 } };
  const dims = filterDimensions([dim], assign);
  const custom = dims.find((d) => d.id === "dim:1")!;

  it("appends a user-defined classification that has values as a trailing filter dimension", () => {
    expect(custom).toBeTruthy();
    expect(custom.label()).toBe("カテゴリー");
    expect(custom.options.map((o) => o.value)).toEqual(["11", "12"]);
    expect(custom.options.map((o) => o.label())).toEqual(["バグ", "機能"]);
  });

  it("passes only tasks assigned to the chosen value", () => {
    const t1 = { id: "t1", title: "t" } as unknown as TaskCard;
    const t2 = { id: "t2", title: "t" } as unknown as TaskCard;
    const t3 = { id: "t3", title: "t" } as unknown as TaskCard; // unassigned
    expect(passesFilters(t1, dims, { "dim:1": ["11"] })).toBe(true);
    expect(passesFilters(t2, dims, { "dim:1": ["11"] })).toBe(false);
    expect(passesFilters(t3, dims, { "dim:1": ["11"] })).toBe(false);
    expect(passesFilters(t3, dims, {})).toBe(true); // an unset filter narrows nothing
    expect(passesFilters(t3, dims, { "dim:1": [] })).toBe(true); // nor does one with nothing chosen on it
  });

  it("does not surface a classification axis with no values as a filter dimension (nothing to narrow)", () => {
    const empty = { ...dim, id: 2, values: [] };
    const only = filterDimensions([empty], {});
    expect(only.find((d) => d.id === "dim:2")).toBeUndefined();
  });
});

describe("filters: an axis narrows to the set chosen on it (AMB-D-655)", () => {
  const dims = filterDimensions();
  const task = (status: string, priority: string | null) =>
    ({ id: status + priority, title: "t", status, priority }) as unknown as TaskCard;
  const done = task("done", "high");
  const rejected = task("rejected", "low");
  const todo = task("todo", "high");

  it("passes a task matching any of the values chosen on one axis", () => {
    const closed = { status: ["done", "rejected"] };
    expect(passesFilters(done, dims, closed)).toBe(true);
    expect(passesFilters(rejected, dims, closed)).toBe(true);
    expect(passesFilters(todo, dims, closed)).toBe(false);
  });

  it("holds every value of the status axis and no word standing for a group of them", () => {
    const status = dims.find((d) => d.id === "status")!;
    expect(status.options.map((o) => o.value)).toEqual(["todo", "in_progress", "blocked", "done", "rejected"]);
  });

  it("still ANDs the axes against each other", () => {
    expect(passesFilters(done, dims, { status: ["done", "rejected"], priority: ["high"] })).toBe(true);
    expect(passesFilters(rejected, dims, { status: ["done", "rejected"], priority: ["high"] })).toBe(false);
  });

  it("narrows nothing on an axis whose chosen values have all gone stale", () => {
    // A value left behind by a deleted dimension value must not empty the board.
    expect(passesFilters(todo, dims, { status: ["retired"] })).toBe(true);
  });
});

describe("selectionKey: the same set is the same key", () => {
  it("reads the same whichever order the values were pressed in", () => {
    expect(selectionKey({ status: ["rejected", "done"] })).toBe(selectionKey({ status: ["done", "rejected"] }));
  });

  it("drops the axes narrowing nothing, and sorts the ones that do", () => {
    expect(selectionKey({ priority: ["high"], status: [], "dim:1": ["11", "10"] })).toBe("dim:1=10,11&priority=high");
    expect(selectionKey({})).toBe("");
  });
});

describe("parseRefQuery: recognizing ref numbers in the search box", () => {
  it("`#123` / `T-123` are task numbers", () => {
    expect(parseRefQuery("#123")).toEqual({ num: 123, space: "task" });
    expect(parseRefQuery("T-45")).toEqual({ num: 45, space: "task" });
    expect(parseRefQuery("t-45")).toEqual({ num: 45, space: "task" });
  });

  it("`D-80` is a decision number (case-insensitive)", () => {
    expect(parseRefQuery("D-80")).toEqual({ num: 80, space: "decision" });
    expect(parseRefQuery("d-80")).toEqual({ num: 80, space: "decision" });
  });

  it("ignores surrounding whitespace", () => {
    expect(parseRefQuery("  #7  ")).toEqual({ num: 7, space: "task" });
  });

  it("non-ref words, bare numbers, and partial matches are null (fall back to text search)", () => {
    expect(parseRefQuery("")).toBeNull();
    expect(parseRefQuery("バグ修正")).toBeNull();
    expect(parseRefQuery("123")).toBeNull();
    expect(parseRefQuery("#12a")).toBeNull();
    expect(parseRefQuery("fix #12")).toBeNull();
    expect(parseRefQuery("D-")).toBeNull();
  });
});
