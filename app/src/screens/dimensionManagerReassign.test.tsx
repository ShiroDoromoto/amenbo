// @vitest-environment jsdom
// Removing a value of a required axis is the one removal in this panel that cannot just be confirmed:
// core will not let the value take its tasks' answers with it (`AMB-D-752`), so the panel has to ask
// where they go — and the axis's last value it must not offer to remove at all.
//
// What these guard: a value tasks answer with opens the destination picker instead of deleting, the
// chosen value rides along to the write, a value nobody answers with is still the plain confirm, and
// the last value of a required axis is out of reach with the reason on the button.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

// The mock factories run before the module body, so the two values live inside the hoisted block and are
// read back out of it — a const up here is not yet initialised when `vi.mock` reaches for it.
const hoisted = vi.hoisted(() => {
  const MAIN = { id: 901, name: "メイン", slug: "main" };
  const THEME = { id: 902, name: "検索の作り直し", slug: "search" };
  return {
  MAIN,
  THEME,
  /** Which value ids the assignments read answers with — the tasks standing on the axis. */
  answered: [] as number[],
  /** The axis as the panel is to draw it. */
  axis: { required: true, values: [MAIN, THEME] } as { required: boolean; values: (typeof MAIN)[] },
  /** Every removal the panel asked for, as `<valueId>-><destination|nowhere>`. */
  removed: [] as string[],
  /** What the confirmation answered, and the sentence it was asked with. */
  confirmed: true,
  asked: [] as string[],
  };
});

const { MAIN, THEME } = hoisted;

vi.mock("../core/mutations", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/mutations")>();
  return {
    ...orig,
    fetchProjectDimensionAssignments: async () =>
      hoisted.answered.map((valueId, i) => ({ taskId: i + 1, valueId })),
    removeDimensionValue: async (valueId: number, reassignTo: number | null = null) => {
      hoisted.removed.push(`${valueId}->${reassignTo ?? "nowhere"}`);
    },
  };
});

// The panel asks through the native dialog, which has no window in jsdom.
vi.mock("../core/dialog", () => ({
  confirmDialog: async (message: string) => {
    hoisted.asked.push(message);
    return hoisted.confirmed;
  },
}));

vi.mock("../core/snapshot", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/snapshot")>();
  let from: unknown;
  let axisFrom: unknown;
  let withAxis: ReturnType<typeof orig.getSnapshot>;
  return {
    ...orig,
    getSnapshot: () => {
      const snap = orig.getSnapshot();
      // Keyed on the axis as well as on the snapshot underneath: `useSyncExternalStore` compares by
      // identity, so the rewrite has to be handed back the same object until something really changed —
      // and between tests the axis is what changes while the snapshot underneath does not.
      if (snap !== from || axisFrom !== hoisted.axis) {
        from = snap;
        axisFrom = hoisted.axis;
        const dim = {
          id: 900, name: "テーマ", slug: "theme", notes: "", role: "none" as const, ordered: false,
          showOnCard: false, required: hoisted.axis.required, values: hoisted.axis.values,
        };
        withAxis = { ...snap, projects: snap.projects.map((p) => (p.id === 1 ? { ...p, dimensions: [dim] } : p)) };
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

/** The delete button of the value drawn second — `THEME`, the one the tasks answer with. */
const themeDelete = () => container.querySelectorAll<HTMLButtonElement>(".dimmgr__val button.dimmgr__danger")[1];
const picker = () => container.querySelector<HTMLSelectElement>("select.dimmgr__reassignpick");
/** The delete button inside the picker, which is the second one in that value's pill once it is open. */
const confirmMove = () => container.querySelectorAll<HTMLButtonElement>(".dimmgr__reassign button.dimmgr__danger")[0];

async function press(button: HTMLButtonElement) {
  await act(async () => { button.click(); });
  await act(async () => { await new Promise((r) => setTimeout(r, 0)); });
}

beforeAll(async () => {
  await loadSnapshot();
});

beforeEach(() => {
  hoisted.answered = [THEME.id];
  hoisted.axis = { required: true, values: [MAIN, THEME] };
  hoisted.removed = [];
  hoisted.confirmed = true;
  hoisted.asked = [];
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

function open() {
  act(() => root.render(createElement(
    StoreProvider, null,
    createElement(DimensionManager, { projectId: 1, onClose: () => {} }),
  )));
}

describe("removing a value of a required category", () => {
  it("asks where the tasks go instead of deleting, and carries the answer to the write", async () => {
    open();

    await press(themeDelete());
    expect(hoisted.removed).toEqual([]); // Nothing went yet — the question came first
    expect(hoisted.asked).toEqual([]);
    const pick = picker()!;
    expect([...pick.options].map((o) => o.value)).toEqual(["", String(MAIN.id)]); // Its own value is not a destination
    expect(confirmMove().disabled).toBe(true); // …and nothing goes until one is chosen

    await act(async () => {
      pick.value = String(MAIN.id);
      pick.dispatchEvent(new Event("change", { bubbles: true }));
    });
    await press(confirmMove());

    expect(hoisted.asked[0]).toContain(MAIN.name); // The confirmation names where they land
    expect(hoisted.removed).toEqual([`${THEME.id}->${MAIN.id}`]);
  });

  it("leaves the value alone when the confirmation is declined", async () => {
    hoisted.confirmed = false;
    open();

    await press(themeDelete());
    const pick = picker()!;
    await act(async () => {
      pick.value = String(MAIN.id);
      pick.dispatchEvent(new Event("change", { bubbles: true }));
    });
    await press(confirmMove());

    expect(hoisted.removed).toEqual([]);
    expect(picker()).toBeNull(); // …and the panel is back to its ordinary row
  });

  it("is the plain confirmation when no task answers with the value", async () => {
    hoisted.answered = [MAIN.id];
    open();

    await press(themeDelete());

    expect(picker()).toBeNull();
    expect(hoisted.removed).toEqual([`${THEME.id}->nowhere`]);
  });

  it("does not offer to remove the last value, and says why", () => {
    hoisted.axis = { required: true, values: [THEME] };
    open();

    const only = container.querySelectorAll<HTMLButtonElement>(".dimmgr__val button.dimmgr__danger")[0];
    expect(only.disabled).toBe(true);
    expect(only.title).not.toBe("");
  });

  it("still lets an ordinary category's value go with its tasks", async () => {
    hoisted.axis = { required: false, values: [MAIN, THEME] };
    open();

    await press(themeDelete());

    expect(picker()).toBeNull();
    expect(hoisted.removed).toEqual([`${THEME.id}->nowhere`]);
  });
});
