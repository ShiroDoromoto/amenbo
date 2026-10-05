// @vitest-environment jsdom
// What the workspace was asked to open, once the face that answered it has come down.
//
// Splitting the workspace out takes the face here down, and folding back stands a new one. The new
// face does what it is handed on its first draw (`./WorkspaceFace`), so an asking left over from the
// face before would be answered twice: the pane closed since stood again, or a terminal started in
// the folder again — with nobody having pressed anything.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

type OpenIn = { project: number; dir?: string; pane?: string; run?: number; nth: number } | null;

const hoisted = vi.hoisted(() => ({
  /** What each face was handed when it was stood, in the order the faces were stood. */
  stood: [] as OpenIn[],
}));

// The host's own side of the bridge, which jsdom has none of — stood up so the shell's real `listen`
// can register the callback this test plays the host through.
const heard = new Map<string, (e: { event: string; id: number; payload: unknown }) => void>();
const callbacks = new Map<number, (e: { event: string; id: number; payload: unknown }) => void>();
let nextCallback = 0;

// Inside Tauri, because the second window going away is a thing only a host says.
vi.mock("../core/snapshot", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../core/snapshot")>()),
  inTauri: () => true,
}));
vi.mock("../core/ipc", () => ({ invoke: () => Promise.resolve(null) }));
vi.mock("../core/dialog", () => ({ confirmDialog: () => Promise.resolve(true) }));

vi.mock("./TopBar", () => ({ TopBar: () => null }));
// The face, as what it is handed on its first draw and the button that splits it out.
vi.mock("./WorkspaceFace", async () => {
  const { useState } = await import("react");
  return {
    WorkspaceFace: ({ openIn, onWindow }: { openIn: OpenIn; onWindow: () => void }) => {
      useState(() => { hoisted.stood.push(openIn); });
      return createElement("button", { className: "split-out", onClick: onWindow });
    },
  };
});
vi.mock("./Sidebar", () => ({ Sidebar: () => null }));
vi.mock("../components/UpdateBanner", () => ({ UpdateBanner: () => null, UpdateCheckFeedback: () => null }));
vi.mock("../components/HealthBanner", () => ({ HealthBanner: () => null }));
vi.mock("../components/ManagedBlockBanner", () => ({ ManagedBlockBanner: () => null }));
vi.mock("../components/OrphanBindingBanner", () => ({ OrphanBindingBanner: () => null }));
vi.mock("../components/HandoverBanner", () => ({ HandoverBanner: () => null }));
vi.mock("../components/HookSetupBanner", () => ({ HookSetupBanner: () => null }));
vi.mock("../components/TickBanner", () => ({ TickBanner: () => null }));
vi.mock("../screens/NudgeHost", () => ({ NudgeHost: () => null }));
vi.mock("../screens/HookConsentModal", () => ({ HookConsentModal: () => null }));
// The board, as its "start in the workspace" press.
vi.mock("../screens/BoardScreen", () => ({
  BoardScreen: ({ onStartTerminal }: { onStartTerminal: (project: number, dir: string) => void }) =>
    createElement("button", { className: "start-in", onClick: () => onStartTerminal(1, "/repo") }),
}));
vi.mock("../screens/TaskDetailPane", () => ({ TaskDetailPane: () => null }));
vi.mock("../screens/DecisionDetailPane", () => ({ DecisionDetailPane: () => null }));
vi.mock("../mock/adapter", () => ({
  dataAdapter: { listProjects: () => [{ id: 1, name: "one" }] },
}));

import { AppShell } from "./AppShell";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

async function press(selector: string) {
  await act(async () => {
    container.querySelector<HTMLButtonElement>(selector)!.click();
    await Promise.resolve();
  });
}

beforeEach(() => {
  localStorage.clear();
  hoisted.stood = [];
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
  (window as unknown as { __TAURI_EVENT_PLUGIN_INTERNALS__: unknown }).__TAURI_EVENT_PLUGIN_INTERNALS__ = {
    unregisterListener: (event: string) => { heard.delete(event); },
  };
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(async () => {
  await act(async () => {
    root.unmount();
    await Promise.resolve();
  });
  container.remove();
});

describe("folding back into one window", () => {
  it("stands the face without the asking the face before it already answered", async () => {
    await act(async () => {
      root.render(createElement(AppShell));
      await Promise.resolve();
    });
    await press(".start-in");
    expect(hoisted.stood).toEqual([{ project: 1, dir: "/repo", nth: 1 }]);

    await press(".split-out");
    expect(container.querySelector(".split-out")).toBeNull();

    const closed = heard.get("talk://closed");
    expect(closed, "the shell is not listening for the second window going away").toBeDefined();
    await act(async () => {
      closed!({ event: "talk://closed", id: 1, payload: null });
      await Promise.resolve();
    });

    expect(hoisted.stood).toEqual([{ project: 1, dir: "/repo", nth: 1 }, null]);
  });
});
