// @vitest-environment jsdom
// The panel's half of closing a value (`AMB-D-829`). It holds two controls nothing else does: the box
// that nominates an axis closable, and — only there — the button that closes one of its values and the
// one that opens it again. This is also the one face that shows a closed value at all, so if it does not
// draw the way back, nothing does.
//
// What these guard: the button appears only under the role, it asks for the direction the value is not
// already in, the box that grants the role asks for the role rather than for the time axis, and the
// fold the closed values sit behind counts them and opens on a press — showing them folded is still
// showing them, but only while the way back stays one press away.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { t, tn } from "../core/i18n";

const hoisted = vi.hoisted(() => ({
  /** Every close the panel asked for, as `<valueId>:<closed>`. */
  asked: [] as string[],
  /** Every role the panel asked for, as `<axisId>:<closable>`. */
  roleAsked: [] as string[],
  /** Every reorder the panel asked for, as `<valueId>:before|after:<anchorId>`. */
  moved: [] as string[],
  /** What the axis on screen holds — a test moves either to see the other side of the panel. */
  role: "none" as "none" | "closable",
  closed: false,
  /** Whether a third value sits below the other two, so the closed one can be read as an in-between. */
  third: false,
}));

// Only the two writes these controls drive are stood in for; the panel, the store and the snapshot run
// for real, so what is under test is the whole path from the button to the value that leaves the app.
vi.mock("../core/mutations", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/mutations")>();
  return {
    ...orig,
    setDimensionValueClosed: async (valueId: number, closed: boolean) => {
      hoisted.asked.push(`${valueId}:${closed}`);
    },
    setDimensionClosable: async (id: number, closable: boolean) => {
      hoisted.roleAsked.push(`${id}:${closable}`);
    },
    moveDimensionValue: async (valueId: number, pos: { before?: number; after?: number }) => {
      const [side, anchor] = pos.before === undefined ? ["after", pos.after] : ["before", pos.before];
      hoisted.moved.push(`${valueId}:${side}:${anchor}`);
    },
  };
});

// The mock store has no axes, so one is hung on project 1. Rebuilt whenever the snapshot, the role or the
// value's own state moves: `useSyncExternalStore` compares by identity.
vi.mock("../core/snapshot", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/snapshot")>();
  let from: unknown;
  let at: string;
  let withAxis: ReturnType<typeof orig.getSnapshot>;
  return {
    ...orig,
    getSnapshot: () => {
      const snap = orig.getSnapshot();
      const state = `${hoisted.role}:${hoisted.closed}:${hoisted.third}`;
      if (snap !== from || at !== state) {
        from = snap;
        at = state;
        const axis = {
          id: 900, name: "リリース", slug: "release", notes: "", role: hoisted.role,
          cardinality: "single" as const, ordered: true, showOnCard: false, required: false,
          appliesTo: "both" as const,
          values: [
            { id: 901, name: "v19", slug: "v19", closed: false },
            { id: 902, name: "v18", slug: "v18", closed: hoisted.closed },
            ...(hoisted.third ? [{ id: 903, name: "v17", slug: "v17", closed: false }] : []),
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

/** The value rows on screen, in the order the panel drew them. */
const rows = () => [...container.querySelectorAll<HTMLDivElement>(".dimmgr__val")];

/** The second value's row — the one a test closes, so the first stays open beside it. */
const row = () => rows()[1];

/** The fold the closed values sit behind, absent while the axis has none. */
const fold = () => container.querySelector<HTMLButtonElement>(".dimmgr__closedfold");

/** The buttons on that row, by the label they carry. */
const buttons = () => [...row().querySelectorAll<HTMLButtonElement>("button")].map((b) => b.textContent);

const button = (label: string) =>
  [...row().querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === label);

/** The box that nominates the axis closable, found by the label it sits under. */
const closableBox = () =>
  [...container.querySelectorAll<HTMLLabelElement>("label.dimmgr__ordered")]
    .find((l) => l.textContent?.includes(t("dimmgr.closable")))!
    .querySelector<HTMLInputElement>("input[type=checkbox]")!;

/** Opens the fold, so the rows underneath are the axis's whole list. */
async function unfold() {
  const b = fold();
  if (!b) throw new Error("nothing is folded away");
  await act(async () => { b.click(); });
}

async function press(label: string) {
  const b = button(label);
  if (!b) throw new Error(`no button reads ${label}`);
  await act(async () => { b.click(); });
  await act(async () => { await new Promise((r) => setTimeout(r, 0)); });
}

beforeAll(async () => {
  await loadSnapshot();
});

beforeEach(() => {
  hoisted.asked = [];
  hoisted.roleAsked = [];
  hoisted.moved = [];
  hoisted.role = "none";
  hoisted.closed = false;
  hoisted.third = false;
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("DimensionManager closing a value", () => {
  function open() {
    act(() => root.render(createElement(
      StoreProvider, null,
      createElement(DimensionManager, { projectId: 1, onClose: () => {} }),
    )));
  }

  it("offers no close button on an axis nobody nominated closable", () => {
    open();

    expect(buttons()).not.toContain(t("dimmgr.closeValue"));
    expect(buttons()).toContain(t("dimmgr.removeValue"));
  });

  it("asks for the role when the reader ticks the box", async () => {
    open();

    await act(async () => { closableBox().click(); });

    expect(hoisted.roleAsked).toEqual(["900:true"]);
  });

  it("asks to close an open value on a closable axis", async () => {
    hoisted.role = "closable";
    open();

    await press(t("dimmgr.closeValue"));

    expect(hoisted.asked).toEqual(["902:true"]);
  });

  it("offers the way back on a closed value, and asks for it", async () => {
    hoisted.role = "closable";
    hoisted.closed = true;
    open();
    await unfold();

    expect(buttons()).toContain(t("dimmgr.reopenValue"));
    await press(t("dimmgr.reopenValue"));

    expect(hoisted.asked).toEqual(["902:false"]);
  });

  it("folds a closed value away, saying how many it holds", () => {
    hoisted.role = "closable";
    hoisted.closed = true;
    open();

    expect(rows().length).toBe(1);
    expect(fold()?.textContent).toBe(tn("dimmgr.showClosed", 1));
  });

  it("still brings a closed value back, since this is the only face that can", async () => {
    hoisted.role = "closable";
    hoisted.closed = true;
    open();
    await unfold();

    expect(rows().length).toBe(2);
    expect(container.querySelectorAll(".dimmgr__val--closed").length).toBe(1);
    expect(fold()?.textContent).toBe(t("dimmgr.hideClosed"));
  });

  it("offers no fold on an axis with nothing closed", () => {
    hoisted.role = "closable";
    open();

    expect(rows().length).toBe(2);
    expect(fold()).toBe(null);
  });

  it("reorders by the row above on screen, clearing the closed value the fold hides", async () => {
    hoisted.role = "closable";
    hoisted.closed = true;
    hoisted.third = true;
    open();

    // On screen: v19, v17 — v18 is closed and folded away between them. So "up" on the second row
    // anchors on v19, the row a reader sees above it, and not on the v18 they do not.
    const up = rows()[1].querySelector<HTMLButtonElement>(".dimmgr__movebtn")!;
    await act(async () => { up.click(); });

    expect(hoisted.moved).toEqual(["903:before:901"]);
  });
});
