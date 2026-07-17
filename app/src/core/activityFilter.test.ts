import { describe, it, expect } from "vitest";
import { matchesActivityFilter } from "./activityFilter";
import type { ActivityItem } from "../mock/types";

function item(kind: "system" | "comment", facet: "human" | "ai"): ActivityItem {
  return { kind, author: { kind: facet } } as unknown as ActivityItem;
}

describe("activityFilter: the two axes of kind × actor", () => {
  it("all/all passes everything", () => {
    for (const k of ["system", "comment"] as const)
      for (const f of ["human", "ai"] as const)
        expect(matchesActivityFilter(item(k, f), "all", "all")).toBe(true);
  });

  it("specifying only the kind passes just that kind (actor unspecified)", () => {
    expect(matchesActivityFilter(item("system", "ai"), "system", "all")).toBe(true);
    expect(matchesActivityFilter(item("comment", "ai"), "system", "all")).toBe(false);
    expect(matchesActivityFilter(item("comment", "human"), "comment", "all")).toBe(true);
  });

  it("specifying only the actor passes just that facet (kind unspecified)", () => {
    expect(matchesActivityFilter(item("comment", "ai"), "all", "ai")).toBe(true);
    expect(matchesActivityFilter(item("system", "ai"), "all", "ai")).toBe(true);
    expect(matchesActivityFilter(item("comment", "human"), "all", "ai")).toBe(false);
  });

  it("specifying both axes is AND (passes only when both match)", () => {
    expect(matchesActivityFilter(item("comment", "ai"), "comment", "ai")).toBe(true);
    expect(matchesActivityFilter(item("system", "ai"), "comment", "ai")).toBe(false);
    expect(matchesActivityFilter(item("comment", "human"), "comment", "ai")).toBe(false);
  });
});
