// @vitest-environment jsdom
// What the value picker offers once an axis can close its values (`AMB-D-829`). Closing retires a value
// from what a record is newly filed under, and this field is exactly that door — so a closed value drops
// out of it.
//
// The exception is what makes closing safe: a record already carrying the value goes on showing it. Drop
// that and a task filed under a closed release could neither be read off the pane nor moved to another
// value, which is the opposite of what closing rather than deleting is for.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { DimensionField } from "./DimensionField";
import type { DimensionDto, DimensionValueDto } from "../bindings/bindings";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const value = (id: number, name: string, closed = false): DimensionValueDto => ({ id, name, closed });

const AXIS: DimensionDto = {
  id: 900, name: "リリース", notes: "", cardinality: "single", role: "closable", ordered: true,
  showOnCard: false, required: false, appliesTo: "both",
  values: [value(901, "v19"), value(902, "v18", true)],
};
const MULTI: DimensionDto = { ...AXIS, cardinality: "multi" };

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

function draw(dim: DimensionDto, selected: number[]) {
  act(() => root.render(createElement(DimensionField, {
    dim,
    selected,
    onSet: () => {},
    onUnset: () => {},
  })));
}

/** The names the select offers, the empty "none" arm dropped. */
const options = () =>
  [...container.querySelectorAll("option")].map((o) => o.textContent).filter((n) => n && !n.startsWith("＋"));

/** The values drawn as chips — what a multi-select record carries. */
const chips = () => [...container.querySelectorAll(".chip--dim")].map((c) => c.textContent?.replace("×", ""));

describe("DimensionField on an axis that closes its values", () => {
  it("leaves a closed value out of the select", () => {
    draw(AXIS, []);

    expect(options()).toContain("v19");
    expect(options()).not.toContain("v18");
  });

  it("offers the closed value the record already carries, so it can still be replaced", () => {
    draw(AXIS, [902]);

    expect(options()).toContain("v18");
  });

  it("draws a carried closed value as a chip on a multi-select axis, and does not offer it again", () => {
    draw(MULTI, [902]);

    expect(chips()).toEqual(["v18"]);
    expect(options()).toEqual(["v19"]);
  });

  it("offers only the open values to a multi-select record carrying none", () => {
    draw(MULTI, []);

    expect(chips()).toEqual([]);
    expect(options()).toEqual(["v19"]);
  });
});
