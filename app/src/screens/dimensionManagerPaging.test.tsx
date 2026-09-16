// @vitest-environment jsdom
// The panel's half of an axis that keeps growing. The fold holds the closed values down (`AMB-D-829`),
// and nothing holds the open ones down — an axis is free to carry hundreds, all of them on offer — so
// the values are read a page at a time, the way the flat lists are.
//
// What these guard: a page's worth of rows and no more, the reordering anchors read off the whole of
// what is shown rather than off the page (or the value could never leave the page it is on), the page
// following the value across that boundary, and the fold starting the paging over — what it opens is a
// different list, not a longer one.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { t, tf, tn } from "../core/i18n";
import { PAGE_SIZE } from "../components/Pager";

/** An axis long enough to page, with the closed ones scattered among the open. */
const OPEN = PAGE_SIZE + 10;
const CLOSED = 3;

const hoisted = vi.hoisted(() => ({
  /** Every reorder the panel asked for, as `<valueId>:before|after:<anchorId>`. */
  moved: [] as string[],
}));

vi.mock("../core/mutations", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/mutations")>();
  return {
    ...orig,
    moveDimensionValue: async (valueId: number, pos: { before?: number; after?: number }) => {
      const [side, anchor] = pos.before === undefined ? ["after", pos.after] : ["before", pos.before];
      hoisted.moved.push(`${valueId}:${side}:${anchor}`);
    },
  };
});

// The mock store has no axes, so one is hung on project 1. Ids run 900 + n so a row's id says where it
// stands, and the closed ones sit at the front, where the fold takes them out from under the paging.
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
        const values = [...Array(CLOSED + OPEN)].map((_, n) => ({
          id: 900 + n,
          name: `v${n}`,
          slug: `v${n}`,
          closed: n < CLOSED,
        }));
        const axis = {
          id: 900, name: "リリース", slug: "release", notes: "", role: "closable" as const,
          cardinality: "single" as const, ordered: true, showOnCard: false, required: false,
          appliesTo: "both" as const, values,
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

const rows = () => [...container.querySelectorAll<HTMLDivElement>(".dimmgr__val")];

/** The names on the rows drawn, in the order the panel drew them. */
const names = () =>
  rows().map((r) => r.querySelector<HTMLInputElement>(".dimmgr__valname")!.value);

/** What the pager says it is showing, or nothing where everything fits on one page. */
const pagerInfo = () => container.querySelector(".pager__info")?.textContent;

const press = async (label: string) => {
  const b = [...container.querySelectorAll<HTMLButtonElement>("button")]
    .find((x) => x.textContent === label || x.getAttribute("aria-label") === label);
  if (!b) throw new Error(`no button reads ${label}`);
  await act(async () => { b.click(); });
};

/** The up arrow on the row at `n` of the page. */
const up = async (n: number) => {
  const b = rows()[n].querySelectorAll<HTMLButtonElement>(".dimmgr__movebtn")[0];
  await act(async () => { b.click(); });
};

/** The down arrow on the row at `n` of the page. */
const down = async (n: number) => {
  const b = rows()[n].querySelectorAll<HTMLButtonElement>(".dimmgr__movebtn")[1];
  await act(async () => { b.click(); });
};

beforeAll(async () => {
  await loadSnapshot();
});

beforeEach(() => {
  hoisted.moved = [];
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("DimensionManager paging an axis's values", () => {
  function open() {
    act(() => root.render(createElement(
      StoreProvider, null,
      createElement(DimensionManager, { projectId: 1, onClose: () => {} }),
    )));
  }

  it("draws one page of the values on offer, and says which page", () => {
    open();

    expect(rows().length).toBe(PAGE_SIZE);
    expect(pagerInfo()).toContain(tf("pager.range", { from: 1, to: PAGE_SIZE, total: OPEN }));
  });

  it("pages over what the fold holds down, not over the whole axis", () => {
    open();

    // The closed ones are at the front of the axis and out of the paging entirely: the first row is
    // the first value still on offer, and the count the pager names is what the fold left.
    expect(names()[0]).toBe(`v${CLOSED}`);
    expect(pagerInfo()).toContain(tf("pager.range", { from: 1, to: PAGE_SIZE, total: OPEN }));
  });

  it("starts the paging over when the fold opens, the list being a different one", async () => {
    open();
    await press("next");
    expect(pagerInfo()).toContain(tf("pager.page", { page: 2, pages: 2 }));

    await press(tn("dimmgr.showClosed", CLOSED));

    expect(pagerInfo()).toContain(tf("pager.page", { page: 1, pages: 2 }));
    expect(names()[0]).toBe("v0");
    await press(t("dimmgr.hideClosed"));
  });

  it("anchors the first row of a page on the last row of the one before, and follows it there", async () => {
    open();
    await press("next");
    expect(names()[0]).toBe(`v${CLOSED + PAGE_SIZE}`);

    await up(0);

    // Anchored on the row above it in the whole of what is shown — the last row of page 1 — and the
    // page went with it, so the value the reader pressed is still in front of them.
    expect(hoisted.moved).toEqual([`${900 + CLOSED + PAGE_SIZE}:before:${900 + CLOSED + PAGE_SIZE - 1}`]);
    expect(pagerInfo()).toContain(tf("pager.page", { page: 1, pages: 2 }));
  });

  it("anchors the last row of a page on the first row of the one after, and follows it there", async () => {
    open();
    expect(names()[PAGE_SIZE - 1]).toBe(`v${CLOSED + PAGE_SIZE - 1}`);

    await down(PAGE_SIZE - 1);

    // The other side of the same boundary, and the reading is the mirror image: anchored on the row
    // below it in the whole of what is shown — the first row of page 2 — with the page following
    // forward so the value the reader pressed has not left the screen.
    expect(hoisted.moved).toEqual([`${900 + CLOSED + PAGE_SIZE - 1}:after:${900 + CLOSED + PAGE_SIZE}`]);
    expect(pagerInfo()).toContain(tf("pager.page", { page: 2, pages: 2 }));
  });

  it("holds the arrow at the very first row, which has nowhere above it", () => {
    open();

    expect(rows()[0].querySelectorAll<HTMLButtonElement>(".dimmgr__movebtn")[0].disabled).toBe(true);
    expect(rows()[1].querySelectorAll<HTMLButtonElement>(".dimmgr__movebtn")[0].disabled).toBe(false);
  });
});
