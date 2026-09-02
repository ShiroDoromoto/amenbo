// @vitest-environment jsdom
// The panel's half of closing a value (`AMB-D-829`). It holds two controls nothing else does: the box
// that nominates an axis closable, and — only there — the button that closes one of its values and the
// one that opens it again. This is also the one face that shows a closed value at all, so if it does not
// draw the way back, nothing does.
//
// What these guard: the button appears only under the role, it asks for the direction the value is not
// already in, and the box that grants the role asks for the role rather than for the time axis.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { t } from "../core/i18n";

const hoisted = vi.hoisted(() => ({
  /** Every close the panel asked for, as `<valueId>:<closed>`. */
  asked: [] as string[],
  /** Every role the panel asked for, as `<axisId>:<closable>`. */
  roleAsked: [] as string[],
  /** What the axis on screen holds — a test moves either to see the other side of the panel. */
  role: "none" as "none" | "closable",
  closed: false,
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
      const state = `${hoisted.role}:${hoisted.closed}`;
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

/** The second value's row — the one a test closes, so the first stays open beside it. */
const row = () => container.querySelectorAll<HTMLDivElement>(".dimmgr__val")[1];

/** The buttons on that row, by the label they carry. */
const buttons = () => [...row().querySelectorAll<HTMLButtonElement>("button")].map((b) => b.textContent);

const button = (label: string) =>
  [...row().querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === label);

/** The box that nominates the axis closable, found by the label it sits under. */
const closableBox = () =>
  [...container.querySelectorAll<HTMLLabelElement>("label.dimmgr__ordered")]
    .find((l) => l.textContent?.includes(t("dimmgr.closable")))!
    .querySelector<HTMLInputElement>("input[type=checkbox]")!;

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
  hoisted.role = "none";
  hoisted.closed = false;
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

    expect(buttons()).toContain(t("dimmgr.reopenValue"));
    await press(t("dimmgr.reopenValue"));

    expect(hoisted.asked).toEqual(["902:false"]);
  });

  it("still shows a closed value, since this is the only face that can bring it back", () => {
    hoisted.role = "closable";
    hoisted.closed = true;
    open();

    expect(container.querySelectorAll(".dimmgr__val").length).toBe(2);
    expect(container.querySelectorAll(".dimmgr__val--closed").length).toBe(1);
  });
});
