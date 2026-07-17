// Shared keyboard helpers. They keep in one place the test that stops an Enter pressed mid-IME-composition (Japanese
// and the like) from being read as "submit". Every input that acts on a bare Enter decides through this helper.
import type { KeyboardEvent } from "react";

// The IME-safe test for a "submit" Enter. In CJK input the Enter that accepts a conversion also fires keydown, so
// `nativeEvent.isComposing` (plus keyCode 229 for older environments) rejects it, and only an Enter that really means
// submit or next returns true. Modified Enter (⌘/Ctrl+Enter and friends) is each caller's own call.
export function isEnterSubmit(e: KeyboardEvent): boolean {
  return e.key === "Enter" && !e.nativeEvent.isComposing && e.keyCode !== 229;
}
