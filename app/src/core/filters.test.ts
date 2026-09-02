import { describe, it, expect } from "vitest";
import { decisionFilterDimensions, filterDimensions, parseRefQuery, passesFilters, selectionKey } from "./filters";
import type { DecisionDto } from "../bindings/bindings";
import type { TaskCard } from "../mock/types";

describe("filters: user-defined classifications (unified dimension)", () => {
  const dim = {
    id: 1, name: "カテゴリー", notes: "", cardinality: "single" as const, role: "none" as const, ordered: false,
    showOnCard: false, required: false,
    appliesTo: "both" as const,
    values: [{ id: 11, name: "バグ", closed: false }, { id: 12, name: "機能", closed: false }],
  };
  const assign = { t1: { 1: [11] }, t2: { 1: [12] } };
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

  // Closing a value retires it from what a record is newly filed under and from nothing else
  // (`AMB-D-829`). Asking what carried a finished release is the whole reason to close one rather than
  // delete it, so the filter is the face that draws no distinction — unlike the picker and the board.
  it("offers a closed value as a filter option like any other, and still matches on it", () => {
    const retired = { ...dim, values: [dim.values[0], { ...dim.values[1], closed: true }] };
    const withClosed = filterDimensions([retired], assign);
    const axis = withClosed.find((d) => d.id === "dim:1")!;
    const t2 = { id: "t2", title: "t" } as unknown as TaskCard;

    expect(axis.options.map((o) => o.label())).toEqual(["バグ", "機能"]);
    expect(passesFilters(t2, withClosed, { "dim:1": ["12"] })).toBe(true);
  });

  it("does not surface a classification axis with no values as a filter dimension (nothing to narrow)", () => {
    const empty = { ...dim, id: 2, values: [] };
    const only = filterDimensions([empty], {});
    expect(only.find((d) => d.id === "dim:2")).toBeUndefined();
  });

  // A task on several values of one axis (`AMB-D-826`) answers to each of them, and the values within one
  // axis are ORed — so choosing either one keeps it, and it is not counted twice.
  it("keeps a task carrying several values on one axis under any of them", () => {
    const both = filterDimensions([{ ...dim, cardinality: "multi" as const }], { t9: { 1: [11, 12] } });
    const t9 = { id: "t9", title: "t" } as unknown as TaskCard;
    expect(passesFilters(t9, both, { "dim:1": ["11"] })).toBe(true);
    expect(passesFilters(t9, both, { "dim:1": ["12"] })).toBe(true);
    expect(passesFilters(t9, both, { "dim:1": ["11", "12"] })).toBe(true);
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

describe("filters: the decisions tab narrows the same way the board does", () => {
  const dim = {
    id: 1, name: "テーマ", notes: "", cardinality: "single" as const, role: "none" as const, ordered: false,
    showOnCard: false, required: false,
    appliesTo: "both" as const,
    values: [{ id: 11, name: "メイン", closed: false }, { id: 12, name: "会話の窓", closed: false }],
  };
  const assign = { 1: { 1: [11] }, 2: { 1: [12] } };
  const dims = decisionFilterDimensions([dim], assign);
  const decision = (id: number, status: string, supersededBy: unknown[] = []) =>
    ({ id, status, supersededBy }) as unknown as DecisionDto;

  it("offers the status axis, then the project's own axes", () => {
    expect(dims.map((d) => d.id)).toEqual(["status", "dim:1"]);
    expect(dims[0].options.map((o) => o.value)).toEqual(["proposed", "accepted", "rejected", "superseded"]);
  });

  it("narrows by classification, and an axis with nothing chosen narrows nothing", () => {
    const mine = decision(1, "accepted");
    const theirs = decision(2, "accepted");
    const unfiled = decision(3, "accepted");
    expect(passesFilters(mine, dims, { "dim:1": ["11"] })).toBe(true);
    expect(passesFilters(theirs, dims, { "dim:1": ["11"] })).toBe(false);
    expect(passesFilters(unfiled, dims, { "dim:1": ["11"] })).toBe(false);
    expect(passesFilters(unfiled, dims, {})).toBe(true);
    expect(passesFilters(unfiled, dims, { "dim:1": [] })).toBe(true);
  });

  it("reads superseded off the edge and not off the status, and ANDs the two axes", () => {
    const overturned = decision(1, "accepted", [{ id: 9, name: null }]);
    const standing = decision(2, "accepted");
    expect(passesFilters(overturned, dims, { status: ["superseded"] })).toBe(true);
    expect(passesFilters(standing, dims, { status: ["superseded"] })).toBe(false);
    // The two axes are ANDed: the overturned one is filed under the first value, so it survives that
    // pairing and drops out of the other.
    expect(passesFilters(overturned, dims, { status: ["superseded"], "dim:1": ["11"] })).toBe(true);
    expect(passesFilters(overturned, dims, { status: ["superseded"], "dim:1": ["12"] })).toBe(false);
  });

  it("does not surface an axis with no values (nothing to narrow)", () => {
    expect(decisionFilterDimensions([{ ...dim, id: 2, values: [] }], {}).map((d) => d.id)).toEqual(["status"]);
  });
});
