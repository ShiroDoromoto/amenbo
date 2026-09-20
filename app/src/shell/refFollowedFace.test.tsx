// @vitest-environment jsdom
// Where a ref followed out of a pane lands, in the window that holds both the pane and the ledger.
//
// The host settles the click and brings the window forward (`crate::windows::show_ref`); the move
// left is this shell's. Split out, the pane is in the other window and this one is already on the
// ledger — so the selection is the whole of it. In one window the two are faces of the same window,
// and a selection made behind the workspace is a press that opened nothing: the record is drawn
// under a face nobody is looking at (`AMB-D-897`).
//
// It is not visible in code that looks right either way: the selection is made in both shapes, and
// what parts them is whether anything came of it on the screen.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { RefTargetDto } from "../bindings/bindings";
import type { Face } from "../core/windowShape";

const hoisted = vi.hoisted(() => ({
  /** Which records the shell selected, in the order it selected them. */
  selected: [] as string[],
}));

// The host's own side of the bridge, which jsdom has none of. It is stood up rather than the event
// module mocked, because what this test plays is the host saying something — and the shell reaches
// that through the real `listen`, whose whole job is to register a callback here.
const heard = new Map<string, (e: { event: string; id: number; payload: unknown }) => void>();
const callbacks = new Map<number, (e: { event: string; id: number; payload: unknown }) => void>();
let nextCallback = 0;

// Inside Tauri, because a ref arriving from the host is a thing only a host sends.
vi.mock("../core/snapshot", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../core/snapshot")>()),
  inTauri: () => true,
}));
vi.mock("../core/ipc", () => ({ invoke: () => Promise.resolve(null) }));
// Answered yes, so nothing here turns on the discard question a selection can be refused by.
vi.mock("../core/dialog", () => ({ confirmDialog: () => Promise.resolve(true) }));

// The bar, as the two things this test wants of it: it says which face is up, and pressing it goes
// to the terminal — which is the reader's own way onto the face the ref is followed from.
vi.mock("./TopBar", () => ({
  TopBar: ({ face, onSelectFace }: { face: Face; onSelectFace: (f: Face) => void }) =>
    createElement("button", {
      className: "topbar-face",
      "data-face": face,
      onClick: () => onSelectFace("workspace"),
    }),
}));
vi.mock("./WorkspaceFace", () => ({ WorkspaceFace: () => createElement("div", { className: "workspace" }) }));
vi.mock("./Sidebar", () => ({ Sidebar: () => null }));
// The bands the shell stacks above the faces. Each asks the host something of its own, and none of
// them is what this test is about.
vi.mock("../components/UpdateBanner", () => ({ UpdateBanner: () => null, UpdateCheckFeedback: () => null }));
vi.mock("../components/HealthBanner", () => ({ HealthBanner: () => null }));
vi.mock("../components/ManagedBlockBanner", () => ({ ManagedBlockBanner: () => null }));
vi.mock("../components/OrphanBindingBanner", () => ({ OrphanBindingBanner: () => null }));
vi.mock("../components/HandoverBanner", () => ({ HandoverBanner: () => null }));
vi.mock("../components/HookSetupBanner", () => ({ HookSetupBanner: () => null }));
vi.mock("../components/TickBanner", () => ({ TickBanner: () => null }));
vi.mock("../screens/NudgeHost", () => ({ NudgeHost: () => null }));
vi.mock("../screens/HookConsentModal", () => ({ HookConsentModal: () => null }));
vi.mock("../screens/BoardScreen", () => ({ BoardScreen: () => null }));
vi.mock("../screens/TaskDetailPane", () => ({
  TaskDetailPane: ({ taskId }: { taskId: number }) => {
    hoisted.selected.push(`task-${taskId}`);
    return createElement("div", { className: "taskpane" });
  },
}));
vi.mock("../screens/DecisionDetailPane", () => ({
  DecisionDetailPane: ({ decisionId }: { decisionId: number }) => {
    hoisted.selected.push(`decision-${decisionId}`);
    return createElement("div", { className: "decisionpane" });
  },
}));
vi.mock("../mock/adapter", () => ({
  dataAdapter: { listProjects: () => [{ id: 1, name: "one" }] },
}));

import { AppShell } from "./AppShell";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

/** Which face the window is showing, as the bar reads it. */
const facing = () => container.querySelector(".topbar-face")?.getAttribute("data-face") ?? null;

/** Put the shell up and press through to the workspace, the way a reader reaches it. */
async function onTheWorkspace() {
  await act(async () => {
    root.render(createElement(AppShell));
    await Promise.resolve();
  });
  await act(async () => {
    container.querySelector<HTMLButtonElement>(".topbar-face")!.click();
  });
  expect(facing()).toBe("workspace");
}

/** A ref followed out of a pane, as the host hands it over (`crate::windows::show_ref`). */
async function follow(payload: RefTargetDto) {
  const on = heard.get("ref-activated");
  expect(on, "the shell is not listening for a ref followed out of a pane").toBeDefined();
  await act(async () => {
    on!({ event: "ref-activated", id: 1, payload });
    await Promise.resolve();
    await Promise.resolve();
  });
}

beforeEach(() => {
  localStorage.clear();
  hoisted.selected = [];
  heard.clear();
  callbacks.clear();
  nextCallback = 0;
  (window as unknown as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = {
    transformCallback: (cb: (e: { event: string; id: number; payload: unknown }) => void) => {
      nextCallback += 1;
      callbacks.set(nextCallback, cb);
      return nextCallback;
    },
    invoke: (cmd: string, args: { event?: string; handler?: number } = {}) => {
      if (cmd === "plugin:event|listen" && args.event !== undefined && args.handler !== undefined) {
        heard.set(args.event, callbacks.get(args.handler)!);
      }
      return Promise.resolve(nextCallback);
    },
  };
  // The event plugin's own half of the bridge, which is where an unlisten goes.
  (window as unknown as { __TAURI_EVENT_PLUGIN_INTERNALS__: unknown }).__TAURI_EVENT_PLUGIN_INTERNALS__ = {
    unregisterListener: (event: string) => { heard.delete(event); },
  };
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(async () => {
  // Let the unlistens land: each one is a promise the host answers, and a teardown that walked away
  // from them would leave the answers arriving after this file is done with its window.
  await act(async () => {
    root.unmount();
    await Promise.resolve();
  });
  container.remove();
});

describe("a ref followed out of a pane, with the terminal a face of this window", () => {
  it("puts the ledger up, so the task it opened is on the screen", async () => {
    await onTheWorkspace();
    await follow({ kind: "task", id: 7 });

    expect(hoisted.selected).toContain("task-7");
    expect(facing()).toBe("tasks");
  });

  it("does the same for a decision", async () => {
    await onTheWorkspace();
    await follow({ kind: "decision", id: 3 });

    expect(hoisted.selected).toContain("decision-3");
    expect(facing()).toBe("tasks");
  });
});
