// @vitest-environment jsdom
// The restart that applies an update, and what it is about to take with it.
//
// Pressing it ends the process — every terminal running in it goes, and no session comes back on the
// next run (`../talk/terminal`, `crate::pty`). That is the same loss the way out of the app asks
// about, so the banner asks it the same way (`../shell/openPanes`): panes open is the confirmation,
// and nothing open says nothing at all.
//
// What is pinned here is that no answer but a yes ever reaches `restartApp`.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const hoisted = vi.hoisted(() => ({
  /** The sessions the host says are open (`crate::pty::pty_sessions`). */
  running: [] as { session: string; startedAt: string; folder: string | null }[],
  /** How many times the process was asked to start again. */
  restarted: 0,
  /** Whether the person said yes to the plain confirmation. */
  agrees: true,
  /** How many times they were asked it. */
  asked: 0,
}));

vi.mock("../core/snapshot", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/snapshot")>();
  // One object, handed back unchanged: `useSyncExternalStore` compares what it is given with what it
  // had, so a fresh one per read is an endless re-render rather than a store standing still.
  const snap = { versionStatus: { appVersion: "1.0.0", newerVersion: "1.1.0", updateAvailable: true } };
  return {
    ...orig,
    inTauri: () => true,
    subscribe: () => () => {},
    getSnapshot: () => snap,
  };
});
vi.mock("../core/mutations", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/mutations")>();
  return {
    ...orig,
    openLatestInstaller: vi.fn(async () => {}),
    installUpdate: vi.fn(async () => true),
    restartApp: vi.fn(async () => { hoisted.restarted++; }),
  };
});
vi.mock("../core/dialog", () => ({
  confirmDialog: vi.fn(async () => {
    hoisted.asked++;
    return hoisted.agrees;
  }),
}));
vi.mock("../core/ipc", () => ({
  invoke: vi.fn(async (cmd: string) => {
    if (cmd === "pty_sessions") return hoisted.running;
    throw new Error(`unexpected command ${cmd}`);
  }),
}));

import { UpdateBanner } from "./UpdateBanner";
import { t } from "../core/i18n";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

function open(...sessions: string[]) {
  hoisted.running = sessions.map((session) => ({
    session,
    startedAt: "2026-09-06T00:00:00Z",
    folder: null,
  }));
}

/** Matched exactly, so one road's button is never pressed by another road's test. */
const button = (label: string) =>
  Array.from(document.body.querySelectorAll("button")).find(
    (b) => (b.textContent ?? "").trim() === label,
  );

const press = async (label: string) => {
  const b = button(label);
  expect(b, label).toBeTruthy();
  await act(async () => { b!.click(); });
};

beforeEach(() => {
  hoisted.running = [];
  hoisted.restarted = 0;
  hoisted.agrees = true;
  hoisted.asked = 0;
  localStorage.clear();
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

/** The banner, taken as far as the applied update that offers the restart. */
async function offered() {
  await act(async () => { root.render(createElement(UpdateBanner, { recheck: 0 })); });
  await press(t("update.open"));
}

describe("the restart that applies the update", () => {
  it("goes without a word when no terminal is open", async () => {
    await offered();
    await press(t("update.restart"));

    expect(hoisted.asked).toBe(0);
    expect(hoisted.restarted).toBe(1);
  });

  it("asks once when a terminal is open", async () => {
    open("a", "b");
    await offered();
    await press(t("update.restart"));

    expect(hoisted.asked).toBe(1);
    expect(hoisted.restarted).toBe(1);
  });

  it("stays where it is when that question is refused", async () => {
    open("a");
    hoisted.agrees = false;
    await offered();
    await press(t("update.restart"));

    expect(hoisted.restarted).toBe(0);
    // The banner is still standing, so the reader can press it again once they are ready.
    expect(container.textContent).toContain(t("update.ready"));
  });
});
