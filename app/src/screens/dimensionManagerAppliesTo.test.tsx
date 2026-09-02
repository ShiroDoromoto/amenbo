// @vitest-environment jsdom
// The one control on the classification panel with three states rather than two (`AMB-D-789`). Unlike
// the four boxes beside it, it starts on the *wide* side: an axis nobody narrowed classifies tasks and
// decisions alike, so what the select has to show first is `both`.
//
// What these guard: the select draws the side the axis stands on, and moving it **asks for the word
// core takes** — not a boolean, and not the label a reader sees.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

const AXIS = {
  id: 900, name: "占有", slug: "occupancy", notes: "", role: "none" as const, cardinality: "single" as const, ordered: false,
  showOnCard: false, required: false, appliesTo: "both" as const,
  values: [{ id: 901, name: "iOS", slug: "ios" }],
};

const hoisted = vi.hoisted(() => ({
  /** Every side the panel asked for, as `<axisId>:<side>`. */
  asked: [] as string[],
}));

// Only the one write this control drives is stood in for; the panel, the store and the snapshot run for
// real, so what is under test is the whole path from the select to the word that leaves the app.
vi.mock("../core/mutations", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/mutations")>();
  return {
    ...orig,
    setDimensionAppliesTo: async (id: number, appliesTo: string) => {
      hoisted.asked.push(`${id}:${appliesTo}`);
    },
  };
});

// The mock store has no axes, so one is hung on project 1. Rebuilt once per underlying snapshot for the
// reason the key panel's own harness gives: `useSyncExternalStore` compares by identity.
vi.mock("../core/snapshot", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/snapshot")>();
  let from: unknown;
  let withAxis: ReturnType<typeof orig.getSnapshot>;
  return {
    ...orig,
    getSnapshot: () => {
      const snap = orig.getSnapshot();
      if (snap !== from) {
        from = snap;
        withAxis = { ...snap, projects: snap.projects.map((p) => (p.id === 1 ? { ...p, dimensions: [AXIS] } : p)) };
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

/** The axis's side select — the panel's only select that offers the three sides. */
const sideSelect = () =>
  [...container.querySelectorAll<HTMLSelectElement>("select.inlineselect")].find((s) =>
    [...s.options].some((o) => o.value === "decision"),
  )!;

async function choose(side: string) {
  const select = sideSelect();
  await act(async () => {
    select.value = side;
    select.dispatchEvent(new Event("change", { bubbles: true }));
  });
  await act(async () => { await new Promise((r) => setTimeout(r, 0)); });
}

beforeAll(async () => {
  await loadSnapshot();
});

beforeEach(() => {
  hoisted.asked = [];
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("DimensionManager applies-to select", () => {
  function open() {
    act(() => root.render(createElement(
      StoreProvider, null,
      createElement(DimensionManager, { projectId: 1, onClose: () => {} }),
    )));
  }

  it("draws the wide side an axis nobody narrowed stands on, and offers all three", () => {
    open();

    expect(sideSelect().value).toBe("both");
    expect([...sideSelect().options].map((o) => o.value)).toEqual(["both", "task", "decision"]);
  });

  it("asks for the word core takes when the reader narrows it", async () => {
    open();

    await choose("task");

    expect(hoisted.asked).toEqual(["900:task"]);
  });
});
