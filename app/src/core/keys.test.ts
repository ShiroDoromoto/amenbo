import { describe, it, expect } from "vitest";
import type { KeyboardEvent } from "react";
import { isEnterSubmit } from "./keys";

// A minimal stub of a React KeyboardEvent: only what the check looks at (key / nativeEvent.isComposing / keyCode).
function ev(opts: { key: string; isComposing?: boolean; keyCode?: number }): KeyboardEvent {
  return {
    key: opts.key,
    keyCode: opts.keyCode ?? (opts.key === "Enter" ? 13 : 0),
    nativeEvent: { isComposing: opts.isComposing ?? false },
  } as unknown as KeyboardEvent;
}

describe("isEnterSubmit", () => {
  it("treats plain Enter as submit", () => {
    expect(isEnterSubmit(ev({ key: "Enter" }))).toBe(true);
  });

  it("rejects Enter while an IME composition is being confirmed (isComposing=true)", () => {
    expect(isEnterSubmit(ev({ key: "Enter", isComposing: true }))).toBe(false);
  });

  it("rejects the composition-confirming Enter on older environments (keyCode 229)", () => {
    expect(isEnterSubmit(ev({ key: "Enter", keyCode: 229 }))).toBe(false);
  });

  it("keys other than Enter are not submit", () => {
    expect(isEnterSubmit(ev({ key: "a" }))).toBe(false);
    expect(isEnterSubmit(ev({ key: "Escape" }))).toBe(false);
  });
});
