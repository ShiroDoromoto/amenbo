// @vitest-environment jsdom
// The startup health banner has two layers: what is wrong inside the store (snapshot's `startupHealth`) and what
// is wrong with a bound folder's `.amenbo` (legacy format, or gone — core is asked once, at startup).
// Only the boundaries are stubbed (the snapshot and core's detection); the banner's own rendering and branching run for real.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { DoctorIssueDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  /** The issues `pointer_issues` returns (the one environment scan done at startup). */
  pointers: [] as DoctorIssueDto[],
  /** How many times `fetchPointerIssues` was called — the evidence that it happens exactly once, at startup. */
  calls: 0,
  /** The in-store doctor issues the snapshot carries. */
  storeIssues: [] as DoctorIssueDto[],
}));

vi.mock("../core/snapshot", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/snapshot")>();
  // `useSyncExternalStore` demands the same reference every time — hand back a fresh object and re-rendering never stops.
  let cached: unknown = null;
  let cachedFor: unknown = null;
  return {
    ...orig,
    inTauri: () => true,
    subscribe: () => () => {},
    getSnapshot: () => {
      if (cachedFor !== hoisted.storeIssues) {
        cachedFor = hoisted.storeIssues;
        cached = { ...orig.getSnapshot(), startupHealth: { issues: hoisted.storeIssues } };
      }
      return cached;
    },
  };
});
vi.mock("../core/mutations", () => ({
  fetchPointerIssues: () => {
    hoisted.calls += 1;
    return Promise.resolve(hoisted.pointers);
  },
  repairPointers: () => {
    const repaired = hoisted.pointers.filter((p) => p.params.project).map((p) => p.params.dir ?? p.target);
    const unresolved = hoisted.pointers.filter((p) => !p.params.project).map((p) => p.params.dir ?? p.target);
    hoisted.pointers = hoisted.pointers.filter((p) => !p.params.project);
    return Promise.resolve({ repaired, unresolved });
  },
  // The remaining boundaries the AppShell module (where the banner lives) imports; unused by this test.
  fetchStaleManagedBlocks: () => Promise.resolve([]),
  fetchOrphanBindings: () => Promise.resolve([]),
  resyncManagedBlocks: () => Promise.resolve({ scanned: 0, updated: [] }),
  forgetOrphanBindings: () => Promise.resolve(0),
  openLatestInstaller: () => Promise.resolve(),
}));

import { HealthBanner } from "./HealthBanner";
import { doctorText } from "../core/i18n";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

// core carries no prose — only a kind and its params arrive, and the banner composes each line itself, in the UI language.
function pointer(over: Partial<DoctorIssueDto> = {}): DoctorIssueDto {
  return {
    kind: "missing_pointer",
    severity: "warning",
    target: "/w/案件X",
    params: { dir: "/w/案件X", project: "3" },
    ...over,
  };
}

async function render() {
  await act(async () => {
    root.render(createElement(HealthBanner));
  });
}

const lines = () => [...container.querySelectorAll(".healthbanner__line")].map((e) => e.textContent);

beforeEach(() => {
  hoisted.pointers = [];
  hoisted.calls = 0;
  hoisted.storeIssues = [];
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("startup health banner", () => {
  it("shows nothing when there are no issues (environment detection runs once at startup)", async () => {
    await render();
    expect(container.querySelector(".healthbanner")).toBeNull();
    expect(hoisted.calls).toBe(1);
  });

  it("lists both in-store issues and binding pointers as rows", async () => {
    hoisted.storeIssues = [{
      kind: "duplicate_order_key", severity: "warning", target: "project:3", params: { project: "3", order_key: "n" },
    }];
    hoisted.pointers = [
      pointer(),
      pointer({ kind: "legacy_pointer", target: "/w/案件Y/.amenbo", params: { path: "/w/案件Y/.amenbo", project: "4" } }),
    ];
    await render();

    // The surface composes the lines: each one is in the UI language (ja by default) and names what is broken.
    expect(lines()).toEqual([
      doctorText(hoisted.storeIssues[0]).message,
      doctorText(hoisted.pointers[0]).message,
      doctorText(hoisted.pointers[1]).message,
    ]);
    expect(lines()[1]).toContain("/w/案件X");
    expect(lines()[2]).toContain("/w/案件Y/.amenbo");
  });

  it("the banner appears when a binding pointer is broken, even if the store is healthy", async () => {
    hoisted.pointers = [pointer()];
    await render();

    expect(container.querySelector(".healthbanner")).not.toBeNull();
    expect(lines()).toHaveLength(1);

    // ✕ dismisses it for this session only; the next launch evaluates everything again.
    await act(async () => {
      container.querySelector<HTMLButtonElement>(".healthbanner__close")!.click();
    });
    expect(container.querySelector(".healthbanner")).toBeNull();
  });

  it("a broken pointer is fixed by the inline button, and the row disappears once it says it is fixed", async () => {
    hoisted.pointers = [pointer(), pointer({ target: "/w/案件Y", params: { dir: "/w/案件Y", project: "4" } })];
    await render();

    const repair = container.querySelector<HTMLButtonElement>(".healthbanner__action");
    expect(repair, "the repair button appears (does not send to a separate screen)").not.toBeNull();

    await act(async () => repair!.click());

    expect(lines(), "a fixed pointer disappears from detection").toHaveLength(0);
    expect(container.querySelector(".healthbanner__title")!.textContent).toContain("2");
  });

  // A folder whose owner is not uniquely determined is left alone — we never silently pick a project for it. The line stays, and a human decides.
  it("a folder with no uniquely determined owner is left unfixed and remains as a row", async () => {
    hoisted.pointers = [pointer(), pointer({ target: "/w/曖昧", params: { dir: "/w/曖昧" } })];
    await render();

    await act(async () => container.querySelector<HTMLButtonElement>(".healthbanner__action")!.click());

    expect(lines()).toHaveLength(1);
    expect(lines()[0]).toContain("/w/曖昧");
  });

  // With nothing but in-store issues, which the banner cannot fix, no repair button appears — it promises nothing it cannot do.
  it("shows no repair button when there are only in-store issues", async () => {
    hoisted.storeIssues = [{
      kind: "duplicate_order_key", severity: "warning", target: "project:3", params: { project: "3", order_key: "n" },
    }];
    await render();

    expect(container.querySelector(".healthbanner")).not.toBeNull();
    expect(container.querySelector(".healthbanner__action")).toBeNull();
  });
});
