// Detecting that the store has moved ahead of us. Without this flag, a long-running GUI keeps showing stale data and
// quietly stops taking updates.
import { beforeEach, describe, expect, it, vi } from "vitest";
import { formatAheadDetail, isFormatAhead, noteInvokeFailure, resetFormatAheadForTest, subscribeFormatAhead } from "./formatAhead";

/** Stands in for the reason a Tauri command rejects (the structured `CmdError`). */
const cmdError = (code: string) => ({ code, message_en: "…" });

describe("formatAhead", () => {
  beforeEach(() => resetFormatAheadForTest());

  it("is not set by default", () => {
    expect(isFormatAhead()).toBe(false);
  });

  it("is set on a format_ahead rejection and reaches subscribers", () => {
    const fn = vi.fn();
    subscribeFormatAhead(fn);
    noteInvokeFailure(cmdError("format_ahead"));
    expect(isFormatAhead()).toBe(true);
    expect(fn).toHaveBeenCalledTimes(1);
  });

  it("stays unset for other codes, bare strings, and null", () => {
    for (const e of [cmdError("invalid_value"), cmdError("store_busy"), "boom", null, undefined, new Error("x")]) {
      noteInvokeFailure(e);
    }
    expect(isFormatAhead()).toBe(false);
  });

  it("once set it never clears and never wakes subscribers twice (a fact that only a restart can change)", () => {
    const fn = vi.fn();
    subscribeFormatAhead(fn);
    noteInvokeFailure(cmdError("format_ahead"));
    noteInvokeFailure(cmdError("format_ahead"));
    expect(fn).toHaveBeenCalledTimes(1);
    expect(isFormatAhead()).toBe(true);
  });

  it("subscribing while already set fires once immediately (a failure that arrives before the subscription is not lost)", () => {
    noteInvokeFailure(cmdError("format_ahead"));
    const fn = vi.fn();
    subscribeFormatAhead(fn);
    expect(fn).toHaveBeenCalledTimes(1);
  });

  it("keeps the refusal's own words — the only place the version that wrote the store is named", () => {
    noteInvokeFailure({ code: "format_ahead", message_en: "written by a newer Amenbo" });
    expect(formatAheadDetail()).toBe("written by a newer Amenbo");
  });

  it("has no words to show when the rejection carried none (the screen still stands)", () => {
    noteInvokeFailure({ code: "format_ahead" });
    expect(isFormatAhead()).toBe(true);
    expect(formatAheadDetail()).toBeNull();
  });

  it("does not wake an unsubscribed subscriber", () => {
    const fn = vi.fn();
    subscribeFormatAhead(fn)();
    noteInvokeFailure(cmdError("format_ahead"));
    expect(fn).not.toHaveBeenCalled();
  });
});
