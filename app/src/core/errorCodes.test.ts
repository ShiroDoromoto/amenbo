// Rust↔TS error-code parity. Extracts, from the Rust sources themselves, every code the webview can
// receive, and holds it to the single TS source of truth (errorCodes.ts): add or rename a code on one
// side only and this breaks. No codegen — the Rust sources are pulled in with Vite's `?raw` and read
// directly, which keeps a node dependency out of the browser-targeted tsconfig.
import { describe, expect, it } from "vitest";
import { CORE_ERROR_CODES, ERROR_CODES, TAURI_ERROR_CODES } from "./errorCodes";

// Every core code: the match arms of `ErrorCode::as_str()` (`ErrorCode::Variant => "code"`) are the only place the strings are spelled.
import coreErrorRs from "../../../crates/amenbo-core/src/error.rs?raw";
// GUI-only codes raised by the Tauri command layer as `CmdError::coded("code", ...)` — sweep all of src-tauri for them.
const tauriSources = import.meta.glob("../../src-tauri/src/**/*.rs", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

function coreCodesFromRust(): string[] {
  const codes = new Set<string>();
  for (const m of coreErrorRs.matchAll(/ErrorCode::\w+\s*=>\s*"([a-z_]+)"/g)) codes.add(m[1]);
  return [...codes];
}

function tauriCodedCodesFromRust(): string[] {
  const codes = new Set<string>();
  for (const src of Object.values(tauriSources)) {
    for (const m of src.matchAll(/CmdError::coded\(\s*"([a-z_]+)"/g)) codes.add(m[1]);
  }
  return [...codes];
}

const sorted = (xs: readonly string[]) => [...xs].sort();

describe("error code Rust↔TS parity", () => {
  it("core codes mirror amenbo_core::ErrorCode::as_str()", () => {
    expect(sorted(coreCodesFromRust())).toEqual(sorted(CORE_ERROR_CODES));
  });

  it("GUI-only codes mirror the Tauri CmdError::coded(...) literals", () => {
    expect(sorted(tauriCodedCodesFromRust())).toEqual(sorted(TAURI_ERROR_CODES));
  });

  it("the TS single source is exactly the Rust union the webview can receive", () => {
    const rust = new Set([...coreCodesFromRust(), ...tauriCodedCodesFromRust()]);
    expect(sorted(ERROR_CODES)).toEqual(sorted([...rust]));
  });

  it("has no duplicate codes", () => {
    expect(new Set(ERROR_CODES).size).toBe(ERROR_CODES.length);
  });
});
