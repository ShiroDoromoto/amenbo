import { describe, expect, it } from "vitest";
import { coversDay, currentTimeAxisValueId, isTimeAxis } from "./timeAxis";
import type { DimensionDto, DimensionValueDto } from "../bindings/bindings";

const value = (v: Partial<DimensionValueDto> & { id: number; label?: string }): DimensionValueDto => ({
  name: v.label ?? String(v.id),
  ...v,
});
const dim = (role: DimensionDto["role"], values: DimensionValueDto[]): DimensionDto => ({
  id: 1,
  name: "時代",
  notes: "",
  role,
  ordered: true,
  showOnCard: false,
  required: false,
  appliesTo: "both" as const,
  values,
});

describe("coversDay", () => {
  it("today falls inside an inclusive-both-ends window", () => {
    const v = value({ id: 1, startOn: "2026-06-20", endOn: "2026-07-07" });
    expect(coversDay(v, "2026-06-20")).toBe(true);
    expect(coversDay(v, "2026-07-07")).toBe(true);
    expect(coversDay(v, "2026-06-19")).toBe(false);
    expect(coversDay(v, "2026-07-08")).toBe(false);
  });

  it("an empty end date means ongoing: everything on or after the start date is covered", () => {
    const v = value({ id: 2, startOn: "2026-07-08" });
    expect(coversDay(v, "2026-07-08")).toBe(true);
    expect(coversDay(v, "2999-01-01")).toBe(true);
    expect(coversDay(v, "2026-07-07")).toBe(false);
  });

  it("a value with both ends empty claims no era (no period)", () => {
    expect(coversDay(value({ id: 3 }), "2026-07-10")).toBe(false);
  });
});

describe("currentTimeAxisValueId", () => {
  const values = [
    value({ id: 10, label: "dev", startOn: "2026-06-20", endOn: "2026-07-07" }),
    value({ id: 11, label: "ops", startOn: "2026-07-08" }),
    value({ id: 12, label: "unset" }),
  ];

  it("returns the value that contains today", () => {
    expect(currentTimeAxisValueId(dim("time_axis", values), "2026-07-10")).toBe(11);
    expect(currentTimeAxisValueId(dim("time_axis", values), "2026-06-21")).toBe(10);
  });

  it("null when it falls in no window", () => {
    expect(currentTimeAxisValueId(dim("time_axis", values), "2026-01-01")).toBeNull();
  });

  it("a non-time_axis dimension has no current era even when dates are present (gatekeeper)", () => {
    expect(currentTimeAxisValueId(dim("none", values), "2026-07-10")).toBeNull();
    expect(isTimeAxis(dim("none", values))).toBe(false);
  });
});
