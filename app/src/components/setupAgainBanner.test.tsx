// @vitest-environment jsdom
// The band that asks for the Viewer to be set up again. Only the boundary is stubbed (the Tauri command core
// answers through); the seam in `core/viewer`, the query layer and the band's own branching run for real.
//
// What is pinned here is when it goes down: core's answer turning `false` arrives from a send in another
// process, which the change feed never names, so the band has to ask again by itself.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { t } from "../core/i18n";
import { invalidateScopes } from "../core/query";

const hoisted = vi.hoisted(() => ({
  /** What core answers to `viewer_asks_for_setup_again`. */
  asks: true,
  /** How many times it was asked. */
  asked: 0,
  /** How many times the band's button asked for the update screen. */
  opened: 0,
}));

vi.mock("../core/snapshot", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/snapshot")>();
  return { ...orig, inTauri: () => true, subscribe: () => () => {} };
});
vi.mock("../core/ipc", () => ({
  invoke: async (cmd: string) => {
    if (cmd !== "viewer_asks_for_setup_again") throw new Error(`unexpected command ${cmd}`);
    hoisted.asked += 1;
    return hoisted.asks;
  },
}));

import { RECHECK_MS, SetupAgainBanner } from "./SetupAgainBanner";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

async function render() {
  await act(async () => {
    root.render(createElement(SetupAgainBanner, { onUpdate: () => { hoisted.opened += 1; } }));
  });
}

const banner = () => container.querySelector(".healthbanner");

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
  hoisted.asks = true;
  hoisted.asked = 0;
  hoisted.opened = 0;
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
  vi.useRealTimers();
});

describe("SetupAgainBanner", () => {
  it("is up, as the strong offer, with the one sentence when core asks for setup again", async () => {
    await render();
    const b = banner();
    expect(b).not.toBeNull();
    expect(b!.classList.contains("healthbanner--offer")).toBe(true);
    expect(b!.getAttribute("role")).toBe("alert");
    expect(b!.querySelector(".healthbanner__title")!.textContent).toBe(t("viewer.setupAgain"));
  });

  it("opens the update screen from its one button", async () => {
    await render();
    const action = banner()!.querySelector<HTMLButtonElement>("button.healthbanner__action");
    expect(action).not.toBeNull();
    expect(action!.textContent).toBe(t("viewer.update.open"));
    expect(banner()!.querySelectorAll("button")).toHaveLength(1);
    await act(async () => {
      action!.click();
    });
    expect(hoisted.opened).toBe(1);
  });

  it("stays down when core does not ask", async () => {
    hoisted.asks = false;
    await render();
    expect(banner()).toBeNull();
  });

  it("goes down by itself once the server's build comes back new enough", async () => {
    await render();
    expect(banner()).not.toBeNull();
    const before = hoisted.asked;

    // A send in another process brought a new enough build back; nothing on the feed says so.
    hoisted.asks = false;
    await act(async () => {
      vi.advanceTimersByTime(RECHECK_MS);
    });
    expect(hoisted.asked).toBeGreaterThan(before);
    expect(banner()).toBeNull();

    // Once it is down it stops asking.
    const after = hoisted.asked;
    await act(async () => {
      vi.advanceTimersByTime(RECHECK_MS * 3);
    });
    expect(hoisted.asked).toBe(after);
  });

  it("asks again when a setup from outside the window moves the viewer scope", async () => {
    await render();
    hoisted.asks = false;
    await act(async () => {
      invalidateScopes(new Set(["viewer"]));
    });
    expect(banner()).toBeNull();
  });
});
