// @vitest-environment jsdom
// The restart offered by the overtaking gate, and the terminals still running behind it.
//
// The gate goes up whenever the store is found to be ahead of this build, and that can be noticed
// long after startup — another process migrated it while this one was working (`../core/formatAhead`).
// So the panes of that session are still open behind the screen, and starting again ends every one of
// them for good.
//
// It asks, and it does not name: what a way out of the app says is that a terminal is going, and
// nothing about what any of them was doing (`AMB-D-858`). So the question is raised only when there
// is a terminal to lose.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const hoisted = vi.hoisted(() => ({
  /** The sessions the host says are open (`crate::pty::pty_sessions`). */
  running: [] as { session: string; startedAt: string; folder: string | null }[],
  /** How many times the process was asked to start again. */
  restarted: 0,
  /** Whether the person said yes. */
  agrees: true,
  /** How many times they were asked. */
  asked: 0,
}));

vi.mock("../core/snapshot", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/snapshot")>();
  return { ...orig, inTauri: () => true };
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
    if (cmd === "restart_app") { hoisted.restarted++; return null; }
    // Everything else this screen asks for — the language, the CLI's name — is refused the way the
    // overtaken store refuses it, and the screen is drawn from what it already has.
    throw new Error(`format_ahead: ${cmd}`);
  }),
}));

import { RestartGate } from "./RestartGate";
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

const press = async (label: string) => {
  const b = Array.from(container.querySelectorAll("button")).find(
    (one) => (one.textContent ?? "").trim() === label,
  );
  expect(b, label).toBeTruthy();
  await act(async () => { b!.click(); });
};

beforeEach(() => {
  hoisted.running = [];
  hoisted.restarted = 0;
  hoisted.agrees = true;
  hoisted.asked = 0;
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

const render = async () => {
  await act(async () => { root.render(createElement(RestartGate)); });
};

describe("restarting out of the overtaking gate", () => {
  it("goes without a word when no terminal is open", async () => {
    await render();
    await press(t("restart.button"));

    expect(hoisted.asked).toBe(0);
    expect(hoisted.restarted).toBe(1);
  });

  it("asks first when terminals are still running behind the screen", async () => {
    open("a", "b");
    await render();
    await press(t("restart.button"));

    expect(hoisted.asked).toBe(1);
    expect(hoisted.restarted).toBe(1);
  });

  // A refusal is not a failed restart: the screen stays as it was, with nothing said about a restart
  // that was never attempted.
  it("does not restart, and reports no failure, when the question is refused", async () => {
    open("a");
    hoisted.agrees = false;
    await render();
    await press(t("restart.button"));

    expect(hoisted.restarted).toBe(0);
    expect(container.textContent).not.toContain(t("restart.failed"));
  });
});
