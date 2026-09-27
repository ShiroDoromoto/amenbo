// @vitest-environment jsdom
// The panel's half of making tasks wait along an axis's order (`AMB-D-990`). It holds the one switch
// that raises the setting, and it is where a value carrying unfinished tasks is refused its closing.
//
// What these guard: the box is held down, saying why, on an axis the setting does not fit; raising it
// asks first with how many tasks it holds back now, and writes nothing when the reader says no; turning
// it off asks nothing; and a closing refused for unfinished tasks is said on the value's row, with the
// count and a way to the tasks left.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { t, tf } from "../core/i18n";

const hoisted = vi.hoisted(() => ({
  /** Every setting the panel asked for, as `<axisId>:<sequential>`. */
  asked: [] as string[],
  /** Every close the panel asked for, as `<valueId>:<closed>`. */
  closeAsked: [] as string[],
  /** What each confirmation said, and what the reader answers. */
  confirms: [] as string[],
  answer: true,
  /** How many tasks the read says raising the setting would hold. */
  held: 3,
  /** Whether closing is refused for unfinished tasks. */
  refuseClose: false,
  /** The unfinished tasks the panel was asked to show, as `<axisId>:<valueId>`. */
  shown: [] as string[],
  /** What the axis on screen holds. */
  ordered: true,
  sequential: false,
}));

vi.mock("../core/mutations", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/mutations")>();
  return {
    ...orig,
    setDimensionSequential: async (id: number, sequential: boolean) => {
      hoisted.asked.push(`${id}:${sequential}`);
    },
    fetchSequentialHeld: async () => hoisted.held,
    setDimensionValueClosed: async (valueId: number, closed: boolean) => {
      hoisted.closeAsked.push(`${valueId}:${closed}`);
      if (hoisted.refuseClose) {
        throw {
          code: "invalid_dimension_close_unfinished",
          message_en: "2 task(s) on 'v1' are not finished",
          fields: { name: "リリース", value: "v1", count: "2", filter: "dim:リリース=v1 status:todo,in_progress,blocked" },
        };
      }
    },
  };
});

vi.mock("../core/dialog", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/dialog")>();
  return {
    ...orig,
    confirmDialog: async (message: string) => {
      hoisted.confirms.push(message);
      return hoisted.answer;
    },
  };
});

// The mock store has no axes, so one is hung on project 1, rebuilt whenever what it holds moves.
vi.mock("../core/snapshot", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/snapshot")>();
  let from: unknown;
  let at: string;
  let withAxis: ReturnType<typeof orig.getSnapshot>;
  return {
    ...orig,
    getSnapshot: () => {
      const snap = orig.getSnapshot();
      const state = `${hoisted.ordered}:${hoisted.sequential}`;
      if (snap !== from || at !== state) {
        from = snap;
        at = state;
        const axis = {
          id: 900, name: "リリース", slug: "release", notes: "", role: "closable" as const,
          cardinality: "single" as const, ordered: hoisted.ordered, showOnCard: false, required: false,
          sequential: hoisted.sequential, appliesTo: "both" as const,
          values: [
            { id: 901, name: "v1", slug: "v1", closed: false },
            { id: 902, name: "v2", slug: "v2", closed: false },
          ],
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

/** The label the setting's box sits under. */
const label = () =>
  [...container.querySelectorAll<HTMLLabelElement>("label.dimmgr__ordered")]
    .find((l) => l.textContent?.includes(t("dimmgr.sequential")))!;

const box = () => label().querySelector<HTMLInputElement>("input[type=checkbox]")!;

/** The first value's row — the one a test closes. */
const row = () => container.querySelectorAll<HTMLDivElement>(".dimmgr__val")[0];

const button = (label: string) =>
  [...row().querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === label);

async function settle() {
  await act(async () => { await new Promise((r) => setTimeout(r, 0)); });
}

function open() {
  act(() => root.render(createElement(
    StoreProvider, null,
    createElement(DimensionManager, {
      projectId: 1,
      onClose: () => {},
      onShowUnfinished: (dimensionId: number, valueId: number) => hoisted.shown.push(`${dimensionId}:${valueId}`),
    }),
  )));
}

beforeAll(async () => {
  await loadSnapshot();
});

beforeEach(() => {
  hoisted.asked = [];
  hoisted.closeAsked = [];
  hoisted.confirms = [];
  hoisted.answer = true;
  hoisted.held = 3;
  hoisted.refuseClose = false;
  hoisted.shown = [];
  hoisted.ordered = true;
  hoisted.sequential = false;
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("DimensionManager making tasks wait along the order", () => {
  it("holds the box down on an axis the setting does not fit, saying why", () => {
    hoisted.ordered = false;
    open();

    expect(box().disabled).toBe(true);
    expect(label().title).toBe(t("dimmgr.sequentialUnfitHint"));
  });

  it("asks first with how many tasks it holds back now, then raises it", async () => {
    open();

    expect(box().disabled).toBe(false);
    await act(async () => { box().click(); });
    await settle();

    expect(hoisted.confirms).toEqual([tf("dimmgr.confirmSequential", { name: "リリース", count: "3" })]);
    expect(hoisted.asked).toEqual(["900:true"]);
  });

  it("writes nothing when the reader says no", async () => {
    hoisted.answer = false;
    open();

    await act(async () => { box().click(); });
    await settle();

    expect(hoisted.asked).toEqual([]);
  });

  it("turns it off without asking, even where it no longer fits", async () => {
    hoisted.sequential = true;
    hoisted.ordered = false;
    open();

    expect(box().disabled).toBe(false);
    await act(async () => { box().click(); });
    await settle();

    expect(hoisted.confirms).toEqual([]);
    expect(hoisted.asked).toEqual(["900:false"]);
  });

  it("says a closing refused for unfinished tasks on the row, with a way to them", async () => {
    hoisted.sequential = true;
    hoisted.refuseClose = true;
    open();

    await act(async () => { button(t("dimmgr.closeValue"))!.click(); });
    await settle();

    expect(hoisted.closeAsked).toEqual(["901:true"]);
    const refusal = row().querySelector(".dimmgr__refusal");
    expect(refusal?.textContent).toContain("2");
    await act(async () => { button(t("dimmgr.showUnfinished"))!.click(); });

    expect(hoisted.shown).toEqual(["900:901"]);
  });
});
