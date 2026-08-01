// Shared helpers for what happens as a reader types into a field: the test that stops an Enter pressed
// mid-IME-composition (Japanese and the like) from being read as "submit", and the attributes that keep the operating
// system from rewriting the characters themselves.
import type { KeyboardEvent } from "react";

// The IME-safe test for a "submit" Enter. In CJK input the Enter that accepts a conversion also fires keydown, so
// `nativeEvent.isComposing` (plus keyCode 229 for older environments) rejects it, and only an Enter that really means
// submit or next returns true. Modified Enter (⌘/Ctrl+Enter and friends) is each caller's own call.
export function isEnterSubmit(e: KeyboardEvent): boolean {
  return e.key === "Enter" && !e.nativeEvent.isComposing && e.keyCode !== 229;
}

/**
 * Spread onto every field that takes text, so the operating system leaves the characters alone. macOS applies
 * "Capitalize words automatically" inside a webview's editable fields: a lowercase word typed into a box is redrawn
 * capitalized as soon as focus leaves it, and the composition opened to make that change stays open, leaving the mark
 * under the text with nothing to close it. What the app holds never changes — React's state and the store both keep
 * what was typed — but the reader is looking at something they did not type, which is a bug in the only place it
 * matters here.
 *
 * Nothing this app takes is prose. Refs, SHAs, filter expressions, project and plugin names, folder paths and command
 * words are all case-carrying and none of them is a sentence, so a capitalizer, an autocorrector and a spell checker
 * have nothing to offer any of these fields — which is why this is one bag spread everywhere rather than a judgment
 * made field by field. A date, a colour, a checkbox and a file picker take no characters, so they take none of this.
 */
export const asTyped = {
  spellCheck: false,
  autoCapitalize: "off",
  autoCorrect: "off",
} as const;
