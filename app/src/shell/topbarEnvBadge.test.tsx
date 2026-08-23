// @vitest-environment jsdom
// The header badge that says which build this window is. Only the boundary is stubbed (core's
// build-time channel, which a test cannot vary); what the badge renders — and that production renders
// nothing at all — runs for real.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const hoisted = vi.hoisted(() => ({
  /** What core answers for this build's channel: null is production. */
  badge: null as string | null,
  /** How many times it was asked — the evidence it is a startup question, not a per-render one. */
  calls: 0,
}));

vi.mock("../core/mutations", () => ({
  fetchDevBadge: () => {
    hoisted.calls += 1;
    return Promise.resolve(hoisted.badge);
  },
  // The remaining boundary TopBar imports; unused by this test.
  openExternalUrl: () => Promise.resolve(),
}));

import { TopBar } from "./TopBar";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

const props = {
  onBack: () => {},
  onForward: () => {},
  canBack: false,
  canForward: false,
  sidebarCollapsed: false,
  onToggleSidebar: () => {},
  face: "tasks" as const,
  onSelectFace: () => {},
  terminalBadge: false,
};
const badge = () => container.querySelector(".topbar__envbadge");

async function render() {
  await act(async () => {
    root.render(createElement(TopBar, props));
  });
}

beforeEach(() => {
  hoisted.badge = null;
  hoisted.calls = 0;
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("TopBar environment badge", () => {
  it("a production build wears none", async () => {
    await render();
    expect(badge()).toBeNull();
  });

  it("the shared dev build says DEV", async () => {
    hoisted.badge = "DEV";
    await render();
    expect(badge()?.textContent).toBe("DEV");
  });

  it("a task's throwaway instance names its task, so a screenshot says which one it is", async () => {
    hoisted.badge = "DEV AMB-T-2133";
    await render();
    expect(badge()?.textContent).toBe("DEV AMB-T-2133");
  });

  it("a preview CI baked carries the commit and the minute, so two bakes of one theme differ", async () => {
    hoisted.badge = "DEV AMB-T-3493 · 7901f2b9 · 08-22 07:36";
    await render();
    expect(badge()?.textContent).toBe("DEV AMB-T-3493 · 7901f2b9 · 08-22 07:36");
  });

  it("is asked once, not on every render", async () => {
    hoisted.badge = "DEV";
    await render();
    await render(); // a re-render of the same tree
    expect(hoisted.calls).toBe(1);
  });
});
