// The rule for what a card says about its classification (`AMB-D-650` / `AMB-D-651`): the flag decides,
// the grouping axis is left to the column heading, and a task shows only the values it actually has.
import { describe, expect, it } from "vitest";
import type { DimensionDto } from "../bindings/bindings";
import { cardChips } from "./cardChips";

const dim = (
  id: number,
  name: string,
  showOnCard: boolean,
  values: Array<[number, string]>,
  cardinality: DimensionDto["cardinality"] = "single",
): DimensionDto => ({
  id,
  name,
  notes: "",
  cardinality,
  role: "none",
  ordered: false,
  showOnCard,
  required: false,
  appliesTo: "both" as const,
  values: values.map(([vid, vname]) => ({ id: vid, name: vname, closed: false })),
});

const PRODUCT = dim(1, "プロダクト", true, [[11, "Amenbo本体"], [12, "サイト"]]);
const PHASE = dim(2, "フェーズ", true, [[21, "運用第2期"]]);
const AREA = dim(3, "領域", false, [[31, "GUI"]]);

describe("cardChips", () => {
  it("draws the flagged axes and leaves the unflagged ones off", () => {
    const chips = cardChips([PRODUCT, AREA], { "7": { 1: [11], 3: [31] } }, null);
    expect(chips["7"]).toEqual([{ dimId: 1, valueId: 11, axis: "プロダクト", value: "Amenbo本体" }]);
  });

  it("draws nothing at all when no axis is flagged", () => {
    expect(cardChips([AREA], { "7": { 3: [31] } }, null)).toEqual({});
  });

  // The column heading over the card already says that value; on the card it would be pure duplication.
  it("leaves out the axis the columns are grouped by", () => {
    const chips = cardChips([PRODUCT, PHASE], { "7": { 1: [11], 2: [21] } }, 1);
    expect(chips["7"]).toEqual([{ dimId: 2, valueId: 21, axis: "フェーズ", value: "運用第2期" }]);
  });

  it("keeps the order the project lists the axes in", () => {
    const chips = cardChips([PHASE, PRODUCT], { "7": { 1: [11], 2: [21] } }, null);
    expect(chips["7"]?.map((c) => c.axis)).toEqual(["フェーズ", "プロダクト"]);
  });

  // A card carries what it was given — never a placeholder for an axis it sits on no value of.
  it("skips an axis the task has no value on, and omits a task with nothing to draw", () => {
    const chips = cardChips([PRODUCT, PHASE], { "7": { 2: [21] }, "8": {} }, null);
    expect(chips["7"]).toEqual([{ dimId: 2, valueId: 21, axis: "フェーズ", value: "運用第2期" }]);
    expect(chips["8"]).toBeUndefined();
  });

  // A value deleted out from under an assignment the read still carries: name nothing rather than a blank chip.
  it("skips an assignment naming a value the axis no longer has", () => {
    expect(cardChips([PRODUCT], { "7": { 1: [999] } }, null)).toEqual({});
  });

  // An axis admitting several at once (`AMB-D-826`): every value it was given gets a chip, in the axis's
  // own order rather than the order the assignments arrived in.
  it("draws one chip per value on an axis carrying several, in the axis's order", () => {
    const multi = dim(1, "プロダクト", true, [[11, "Amenbo本体"], [12, "サイト"]], "multi");
    const chips = cardChips([multi], { "7": { 1: [12, 11] } }, null);
    expect(chips["7"]).toEqual([
      { dimId: 1, valueId: 11, axis: "プロダクト", value: "Amenbo本体" },
      { dimId: 1, valueId: 12, axis: "プロダクト", value: "サイト" },
    ]);
  });
});
