// @vitest-environment jsdom
// The box that nominates an axis required (`AMB-D-734`), and the one thing it has to agree with core
// about: which values count. Core raises `required` against the values the axis still offers — a closed
// one takes no new record (`AMB-D-829`) — so an axis whose values are all closed is as unanswerable as
// one holding none, and core refuses it there.
//
// What these guard: the box is held down on an axis offering nothing open, whether that is no values at
// all or none left open, and it is live again as soon as one value is open. A box that reads the plain
// count instead is pressable on an all-closed axis and the press comes back as a refusal.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { t } from "../core/i18n";

const hoisted = vi.hoisted(() => ({
  /** Every required the panel asked for, as `<axisId>:<required>`. */
  asked: [] as string[],
  /** What the axis on screen offers — the three shapes the box has to tell apart. */
  offers: "one open" as "one open" | "all closed" | "none",
}));

vi.mock("../core/mutations", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/mutations")>();
  return {
    ...orig,
    setDimensionRequired: async (id: number, required: boolean) => {
      hoisted.asked.push(`${id}:${required}`);
    },
  };
});

// The mock store has no axes, so one is hung on project 1. Rebuilt whenever the snapshot or what the
// axis offers moves: `useSyncExternalStore` compares by identity.
vi.mock("../core/snapshot", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/snapshot")>();
  let from: unknown;
  let at: string;
  let withAxis: ReturnType<typeof orig.getSnapshot>;
  return {
    ...orig,
    getSnapshot: () => {
      const snap = orig.getSnapshot();
      if (snap !== from || at !== hoisted.offers) {
        from = snap;
        at = hoisted.offers;
        const values = hoisted.offers === "none"
          ? []
          : [
            { id: 901, name: "v19", slug: "v19", closed: hoisted.offers === "all closed" },
            { id: 902, name: "v18", slug: "v18", closed: true },
          ];
        const axis = {
          id: 900, name: "リリース", slug: "release", notes: "", role: "closable" as const,
          cardinality: "single" as const, ordered: true, showOnCard: false, required: false,
          appliesTo: "both" as const,
          values,
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

/** The label the box sits under, found by the word on it. */
const requiredLabel = () =>
  [...container.querySelectorAll<HTMLLabelElement>("label.dimmgr__ordered")]
    .find((l) => l.textContent?.includes(t("dimmgr.required")))!;

const requiredBox = () => requiredLabel().querySelector<HTMLInputElement>("input[type=checkbox]")!;

beforeAll(async () => {
  await loadSnapshot();
});

beforeEach(() => {
  hoisted.asked = [];
  hoisted.offers = "one open";
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("DimensionManager nominating an axis required", () => {
  function open() {
    act(() => root.render(createElement(
      StoreProvider, null,
      createElement(DimensionManager, { projectId: 1, onClose: () => {} }),
    )));
  }

  it("asks for it on an axis that still offers a value", async () => {
    open();

    expect(requiredBox().disabled).toBe(false);
    await act(async () => { requiredBox().click(); });

    expect(hoisted.asked).toEqual(["900:true"]);
    expect(requiredLabel().title).toBe(t("dimmgr.requiredHint"));
  });

  it("holds the box down on an axis whose values are all closed, the way core would refuse it", () => {
    hoisted.offers = "all closed";
    open();

    expect(requiredBox().disabled).toBe(true);
    expect(requiredLabel().title).toBe(t("dimmgr.requiredNoValuesHint"));
  });

  it("holds it down on an axis holding no values at all", () => {
    hoisted.offers = "none";
    open();

    expect(requiredBox().disabled).toBe(true);
    expect(requiredLabel().title).toBe(t("dimmgr.requiredNoValuesHint"));
  });
});
