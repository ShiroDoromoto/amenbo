// @vitest-environment jsdom
// The lane count on the band over the panes, and the press that goes to the setting behind it.
//
// Two things are pinned, and neither shows in code that looks right either way.
//
// **The two halves come from different places.** How many lanes there are is a setting and rides in
// the snapshot; how many are held moves when a run somebody else started takes one, and so is read
// off the change feed (`../core/automations`). Drawn from one place they would come apart the moment
// a run started in the other window, and the band would go on saying what it said an hour ago.
//
// **The press is the host's answer, not the band's.** The settings are read on the ledger, which is
// this window in one shape and the other window in the other — so the face is handed the move rather
// than making it, and a face handed nothing draws the number without a way in
// (`crate::windows::show_settings`).
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { tf } from "../core/i18n";
import type { PaneStart } from "../talk/terminal";

const hoisted = vi.hoisted(() => ({
  /** How many lanes the runs are holding, as the feed-backed read answers. */
  held: 0,
}));

vi.mock("../talk/agent", () => ({
  mountAgentFrame: (_host: HTMLElement, _lang: string, on: { opened: (s: string) => void }, start: PaneStart = {}) => {
    on.opened(start.session ?? "s1");
    return Promise.resolve(() => {});
  },
}));

vi.mock("../talk/frames", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../talk/frames")>()),
  frameNames: async () => new Map(),
  nameFrame: async () => new Map(),
  savedLayout: async () => null,
  keepLayout: async () => {},
}));

// The half that hangs off the change feed. What it watches is the feed's business
// (`../core/changes`); what is under test is that the band draws this number and not the other one.
vi.mock("../core/automations", () => ({ useLanesHeld: () => hoisted.held }));

vi.mock("../files/FilesPanel", () => ({ FilesPanel: () => null }));
vi.mock("../files/FolderTree", () => ({ FolderTree: () => null }));
vi.mock("../mock/adapter", () => ({
  dataAdapter: { listProjects: () => [{ id: 1, name: "amenbo" }] },
}));
vi.mock("../core/boundFolders", () => ({
  useBoundFolders: () => ({
    all: [{ path: "/work/a", exists: true }],
    live: [{ path: "/work/a", exists: true }],
    answered: true,
  }),
}));

import { WorkspaceFace } from "./WorkspaceFace";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

/** The band's own control, which is both the number and the way to the setting. */
const band = () => container.querySelector<HTMLButtonElement>(".workspace__lanes");

async function mount(onSettings?: () => void) {
  await act(async () => {
    root.render(createElement(WorkspaceFace, { onWindow: () => {}, note: null, onSettings }));
    await new Promise((r) => setTimeout(r, 0));
  });
}

beforeEach(() => {
  localStorage.clear();
  window.innerWidth = 1600;
  hoisted.held = 0;
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the lane count on the band", () => {
  it("draws what is held over how many there are, and reads both out", async () => {
    hoisted.held = 2;
    await mount(() => {});
    // Three is what a store nobody has changed answers (`amenbo_core::model::DEFAULT_LANES`), and the
    // browser fallback carries that same number (`../core/snapshot`).
    expect(band()?.textContent).toContain("2/3");
    expect(band()?.getAttribute("aria-label")).toBe(tf("face.lanesHeld", { held: 2, lanes: 3 }));
  });

  it("goes to the setting when there is somewhere to go, and is not pressed when there is not", async () => {
    const went = vi.fn();
    await mount(went);
    await act(async () => { band()!.dispatchEvent(new MouseEvent("click", { bubbles: true })); });
    expect(went).toHaveBeenCalledTimes(1);

    await act(async () => root.unmount());
    root = createRoot(container);
    await mount(undefined);
    expect(band()?.disabled).toBe(true);
  });
});
