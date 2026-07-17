// Pure front-end readings of a `role: time_axis` dimension. A period `[startOn, endOn]` (inclusive,
// `YYYY-MM-DD`) is payload of the time_axis role, not a generic attribute of any dimension value —
// so the date fields and the "current era" test apply to that role only. This module is the gate.

import type { DimensionDto, DimensionValueDto } from "../bindings/bindings";

/** Can this dimension's values carry a period? Only a time_axis can. */
export function isTimeAxis(dim: Pick<DimensionDto, "role">): boolean {
  return dim.role === "time_axis";
}

/** One end is enough to make a window; a value with neither end set claims no era at all. */
export function hasPeriod(v: DimensionValueDto): boolean {
  return !!v.startOn || !!v.endOn;
}

/** `today ∈ [startOn, endOn]` — inclusive, and an empty end is an open one. */
export function coversDay(v: DimensionValueDto, today: string): boolean {
  if (!hasPeriod(v)) return false;
  return (!v.startOn || v.startOn <= today) && (!v.endOn || v.endOn >= today);
}

/**
 * The "current era" — the first value on the time_axis whose window covers today. Same rule as
 * core's `current_time_axis_value`. Windows are assumed not to overlap, so at most one value
 * matches; if they do overlap, the one that comes first in the dimension's order wins.
 */
export function currentTimeAxisValueId(dim: DimensionDto, today: string): number | null {
  if (!isTimeAxis(dim)) return null;
  return dim.values.find((v) => coversDay(v, today))?.id ?? null;
}
