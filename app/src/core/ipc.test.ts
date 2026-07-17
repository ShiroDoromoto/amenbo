import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { applyPerfConfig, invoke, perfMode } from "./ipc";
import { isFormatAhead, resetFormatAheadForTest } from "./formatAhead";

const tauriInvoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => tauriInvoke(...a) }));

const DEV_DEFAULT = import.meta.env.DEV ? "budget-only" : "off";

afterEach(() => applyPerfConfig(null)); // back to the default, so tests do not bleed into each other.

describe("applyPerfConfig", () => {
  it("adopts an explicit value as-is (off/budget-only/verbose)", () => {
    applyPerfConfig("off");
    expect(perfMode()).toBe("off");
    applyPerfConfig("budget-only");
    expect(perfMode()).toBe("budget-only");
    applyPerfConfig("verbose");
    expect(perfMode()).toBe("verbose");
  });

  it("unset (null/undefined) falls back to the build default", () => {
    applyPerfConfig(null);
    expect(perfMode()).toBe(DEV_DEFAULT);
    applyPerfConfig(undefined);
    expect(perfMode()).toBe(DEV_DEFAULT);
  });

  it("an unknown value also falls back to the build default (rejecting a bad config)", () => {
    applyPerfConfig("bogus");
    expect(perfMode()).toBe(DEV_DEFAULT);
  });
});

describe("invoke detects format_ahead while re-throwing the rejection as-is", () => {
  beforeEach(() => {
    resetFormatAheadForTest();
    tauriInvoke.mockReset();
  });

  for (const mode of ["off", "budget-only", "verbose"] as const) {
    it(`detects it on the perf=${mode} branch too`, async () => {
      applyPerfConfig(mode);
      const err = { code: "format_ahead", message: "…", message_en: "…" };
      tauriInvoke.mockRejectedValueOnce(err);
      await expect(invoke("snapshot")).rejects.toBe(err); // the error is not swallowed.
      expect(isFormatAhead()).toBe(true);
    });
  }

  it("does not set on other failures", async () => {
    applyPerfConfig("off");
    tauriInvoke.mockRejectedValueOnce({ code: "store_busy", message: "…", message_en: "…" });
    await expect(invoke("snapshot")).rejects.toMatchObject({ code: "store_busy" });
    expect(isFormatAhead()).toBe(false);
  });

  it("passes a success through unchanged", async () => {
    applyPerfConfig("off");
    tauriInvoke.mockResolvedValueOnce(42);
    await expect(invoke<number>("snapshot")).resolves.toBe(42);
    expect(isFormatAhead()).toBe(false);
  });
});
