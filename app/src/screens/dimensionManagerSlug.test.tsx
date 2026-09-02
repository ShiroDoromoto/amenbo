// @vitest-environment jsdom
// The keys the classification panel edits (`AMB-D-735`) are the one field there that core refuses for
// reasons the panel cannot see coming: a shape it cannot carry outside Amenbo, or a key a sibling
// already answers to. Nothing moves on a refusal, so without a rollback the box would sit there showing
// a key nothing was saved under, next to a toast saying it was not.
//
// What these guard: the panel puts the axis's key and each value's key on screen, a refused key **puts
// the field back**, and one that lands **stays**.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

const AXIS = {
  id: 900, name: "フェーズ", slug: "phase", notes: "", cardinality: "single" as const,
  role: "none" as const, ordered: false,
  showOnCard: false, required: false, appliesTo: "both" as const,
  values: [{ id: 901, name: "運用第2期", slug: "ops2" }],
};

const hoisted = vi.hoisted(() => ({
  /** Set it to make both key writes refuse, the way core refuses a shape or a key already taken. */
  slugFails: false,
  /** Every key the panel asked for, as `<axis|value>:<id>:<key>`. */
  asked: [] as string[],
}));

// Only the two writes the key fields drive are stood in for; the panel, the store and the snapshot run
// for real — the rollback lives in the wiring between them.
vi.mock("../core/mutations", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/mutations")>();
  const refuse = { code: "invalid_dimension_slug_taken", message_en: "refused" };
  return {
    ...orig,
    setDimensionSlug: async (id: number, slug: string) => {
      hoisted.asked.push(`axis:${id}:${slug}`);
      if (hoisted.slugFails) throw refuse;
    },
    setDimensionValueSlug: async (valueId: number, slug: string) => {
      hoisted.asked.push(`value:${valueId}:${slug}`);
      if (hoisted.slugFails) throw refuse;
    },
  };
});

// The mock store has no axes, so one is hung on project 1 — the panel renders straight off the snapshot.
// The panel reads it through `useSyncExternalStore`, which compares the snapshot by identity, so the
// rewritten one is built once per underlying snapshot and handed back the same object after that: a
// fresh object every call reads as a change on every render and never settles.
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

const axisSlug = () => container.querySelector<HTMLInputElement>("input.dimmgr__slug")!;
const valueSlug = () => container.querySelector<HTMLInputElement>("input.dimmgr__valslug")!;

/** Type a key and leave the field, the way a reader does. `onBlur` is React's name for `focusout` —
 * `blur` itself does not bubble, so React never hears the one dispatched here. */
async function retype(field: HTMLInputElement, key: string) {
  await act(async () => {
    field.value = key;
    field.dispatchEvent(new FocusEvent("focusout", { bubbles: true }));
  });
  await settle();
}

/** Wait for the write's answer to come back. */
async function settle() {
  await act(async () => { await new Promise((r) => setTimeout(r, 0)); });
}

beforeAll(async () => {
  await loadSnapshot();
});

beforeEach(() => {
  hoisted.slugFails = false;
  hoisted.asked = [];
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("DimensionManager key fields", () => {
  function open() {
    act(() => root.render(createElement(
      StoreProvider, null,
      createElement(DimensionManager, { projectId: 1, onClose: () => {} }),
    )));
  }

  it("puts the stored key of the axis and of its value on screen", () => {
    open();

    expect(axisSlug().value).toBe("phase");
    expect(valueSlug().value).toBe("ops2");
  });

  it("puts the field back when the key is refused", async () => {
    hoisted.slugFails = true;
    open();

    await retype(axisSlug(), "release");
    await retype(valueSlug(), "era2");

    expect(hoisted.asked).toEqual(["axis:900:release", "value:901:era2"]); // Both did try
    expect(axisSlug().value).toBe("phase"); // …and both refusals took the field back
    expect(valueSlug().value).toBe("ops2");
  });

  it("leaves the key the reader typed when the write lands", async () => {
    open();

    await retype(axisSlug(), "release");

    expect(hoisted.asked).toEqual(["axis:900:release"]);
    expect(axisSlug().value).toBe("release");
  });
});
