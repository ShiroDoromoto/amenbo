import { describe, it, expect } from "vitest";
import { filterDimensions, parseRefQuery, passesFilters } from "./filters";
import type { TaskCard } from "../mock/types";

describe("filters: user-defined classifications (unified dimension)", () => {
  const dim = {
    id: 1, name: "カテゴリー", notes: "", role: "none" as const, ordered: false,
    values: [{ id: 11, name: "バグ" }, { id: 12, name: "機能" }],
  };
  const assign = { t1: { 1: 11 }, t2: { 1: 12 } };
  const dims = filterDimensions("me", [dim], assign);
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
    expect(passesFilters(t1, dims, { "dim:1": "11" })).toBe(true);
    expect(passesFilters(t2, dims, { "dim:1": "11" })).toBe(false);
    expect(passesFilters(t3, dims, { "dim:1": "11" })).toBe(false);
    expect(passesFilters(t3, dims, {})).toBe(true); // an unset filter narrows nothing
  });

  it("does not surface a classification axis with no values as a filter dimension (nothing to narrow)", () => {
    const empty = { ...dim, id: 2, values: [] };
    const only = filterDimensions("me", [empty], {});
    expect(only.find((d) => d.id === "dim:2")).toBeUndefined();
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
