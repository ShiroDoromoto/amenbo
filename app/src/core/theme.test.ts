// @vitest-environment jsdom
// What the appearance does across the windows of one app: the preference is answered in one of them
// and every window is wearing it a moment later.
//
// The road under test is the one the reader cannot repair themselves. A reader following the OS is
// repaired by the OS — each window hears `prefers-color-scheme` for itself — so what is read here is
// a named dark or light, which nothing but the window it was named in would otherwise apply.
import { beforeEach, describe, expect, it, vi } from "vitest";

const hoisted = vi.hoisted(() => ({
  /** What crossed to the host, in the order it crossed. */
  said: [] as Array<{ event: string; payload: unknown }>,
  /** Who is listening for what, so a test can be the other window and say something. */
  heard: new Map<string, (said: { payload: unknown }) => void>(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  emit: vi.fn(async (event: string, payload: unknown) => {
    hoisted.said.push({ event, payload });
  }),
  listen: vi.fn(async (event: string, on: (said: { payload: unknown }) => void) => {
    hoisted.heard.set(event, on);
    return () => hoisted.heard.delete(event);
  }),
}));

/** The appearance the page is wearing. */
const worn = () => document.documentElement.dataset.theme;

/** Let the dynamic import of the host's events settle. */
const wired = () => new Promise((over) => setTimeout(over, 0));

describe("an appearance answered in one window", () => {
  beforeEach(() => {
    hoisted.said = [];
    hoisted.heard.clear();
    localStorage.clear();
    delete document.documentElement.dataset.theme;
    // jsdom answers no media query, and one test stands a screen up: taken down here so the order
    // the tests run in cannot decide what a window reads its own screen as.
    delete (window as { matchMedia?: unknown }).matchMedia;
  });

  it("is worn by the window it was answered in, and told to the others", async () => {
    const { setThemePref } = await import("./theme");

    setThemePref("dark");
    await wired();

    expect(worn(), "the window it was answered in").toBe("dark");
    expect(hoisted.said, "and the others were told, with the preference itself").toEqual([
      { event: "theme-changed", payload: "dark" },
    ]);
  });

  // The other half, read as the window that was not answered in: it comes up on what it was left on
  // and then hears. A window that only applied the preference as it started is the fault here, and
  // it is invisible until somebody looks at two windows at once.
  it("is worn by a window that was already drawn when it was answered", async () => {
    const { initTheme } = await import("./theme");

    initTheme();
    await wired();
    expect(worn(), "what it came up on, nobody having said anything").toBe("light");

    hoisted.heard.get("theme-changed")?.({ payload: "dark" });

    expect(worn(), "and what the other window said").toBe("dark");
  });

  // Said as the preference rather than as the appearance it comes to, so a window told to follow the
  // OS resolves that against its own screen.
  it("leaves a window told to follow the OS reading its own screen", async () => {
    const { initTheme } = await import("./theme");
    // A screen of the road's own, jsdom having none to read.
    Object.defineProperty(window, "matchMedia", {
      configurable: true,
      value: () => ({ matches: true, addEventListener: () => {} }) as unknown as MediaQueryList,
    });

    initTheme();
    await wired();
    hoisted.heard.get("theme-changed")?.({ payload: "os" });

    expect(worn(), "this screen is a dark one, whatever the other window's is").toBe("dark");
  });
});
