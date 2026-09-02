// @vitest-environment jsdom
// The box that decides how many of an axis's values one record may hold (`AMB-D-826`). Until it
// existed the only door onto `cardinality` was `dimension update --cardinality`, so a person who only
// ever opens the app could not raise a multi-select axis at all.
//
// What these guard: the box draws the axis's stored cardinality rather than a default, and moving it
// asks for the direction the reader chose — both ways, since lowering it is the direction core can
// refuse and the panel must still be able to ask for it.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { t } from "../core/i18n";

const hoisted = vi.hoisted(() => ({
  /** Every cardinality the panel asked for, as `<axisId>:<multi>`. */
  asked: [] as string[],
  /** What the axis on screen currently holds — flipped by a test that needs the other side. */
  cardinality: "single" as "single" | "multi",
}));

// Only the one write this box drives is stood in for; the panel, the store and the snapshot run for
// real, so what is under test is the whole path from the box to the value that leaves the app.
vi.mock("../core/mutations", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/mutations")>();
  return {
    ...orig,
    setDimensionMulti: async (id: number, multi: boolean) => {
      hoisted.asked.push(`${id}:${multi}`);
    },
  };
});

// The mock store has no axes, so one is hung on project 1. Rebuilt whenever the underlying snapshot or
// the axis's cardinality moves: `useSyncExternalStore` compares by identity.
vi.mock("../core/snapshot", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/snapshot")>();
  let from: unknown;
  let at: string;
  let withAxis: ReturnType<typeof orig.getSnapshot>;
  return {
    ...orig,
    getSnapshot: () => {
      const snap = orig.getSnapshot();
      if (snap !== from || at !== hoisted.cardinality) {
        from = snap;
        at = hoisted.cardinality;
        const axis = {
          id: 900, name: "プロダクト", slug: "product", notes: "", role: "none" as const,
          cardinality: hoisted.cardinality, ordered: false, showOnCard: false, required: false,
          appliesTo: "both" as const,
          values: [{ id: 901, name: "Amenbo本体", slug: "amenbo", closed: false }],
        };
        withAxis = { ...snap, projects: snap.projects.map((p) => (p.id === 1 ? { ...p, dimensions: [axis] } : p)) };
      }
      return withAxis;
    },
  };
});

import { DimensionManager } from "./DimensionManager";
import { StoreProvider } from "../store/store";
import { loadSnapshot } from "../core/snapshot";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

/** The multi-select box, found by the label it sits under rather than by its place among the boxes. */
const multiBox = () =>
  [...container.querySelectorAll<HTMLLabelElement>("label.dimmgr__ordered")]
    .find((l) => l.textContent?.includes(t("dimmgr.multi")))!
    .querySelector<HTMLInputElement>("input[type=checkbox]")!;

/** Move the box to `on` the way a reader does — a click, which is the event React reads a box's
 * change off. */
async function tick(on: boolean) {
  const box = multiBox();
  if (box.checked === on) throw new Error(`the box is already ${on}`);
  await act(async () => { box.click(); });
  await act(async () => { await new Promise((r) => setTimeout(r, 0)); });
}

beforeAll(async () => {
  await loadSnapshot();
});

beforeEach(() => {
  hoisted.asked = [];
  hoisted.cardinality = "single";
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("DimensionManager multi-select box", () => {
  function open() {
    act(() => root.render(createElement(
      StoreProvider, null,
      createElement(DimensionManager, { projectId: 1, onClose: () => {} }),
    )));
  }

  it("is off on an axis one record answers with one value", () => {
    open();

    expect(multiBox().checked).toBe(false);
  });

  it("asks to raise it when the reader ticks it", async () => {
    open();

    await tick(true);

    expect(hoisted.asked).toEqual(["900:true"]);
  });

  it("draws a multi-select axis ticked, and asks to lower it when the reader unticks it", async () => {
    hoisted.cardinality = "multi";
    open();

    expect(multiBox().checked).toBe(true);
    await tick(false);

    expect(hoisted.asked).toEqual(["900:false"]);
  });
});
