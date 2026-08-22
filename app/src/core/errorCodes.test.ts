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

/** The text between `open` and the delimiter that closes it, counting nesting. Reading the two lists out
 * of their own bodies is what keeps an `ErrorCode::` written anywhere else in the file — a doc comment,
 * a unit test — from being read as a declaration. Throws rather than returning nothing: a header that no
 * longer matches means the extraction has stopped seeing the registry, which is the one failure this
 * whole test is here to notice. */
function bodyAfter(src: string, header: string, open: "{" | "[", close: "}" | "]"): string {
  const at = src.indexOf(header);
  if (at < 0) throw new Error(`the Rust registry no longer spells \`${header}\` — the extraction below reads nothing`);
  const from = at + header.length;
  let depth = 1;
  for (let i = from; i < src.length; i++) {
    if (src[i] === open) depth++;
    else if (src[i] === close && --depth === 0) return src.slice(from, i);
  }
  throw new Error(`\`${header}\` is never closed`);
}

// The arm's code string, allowing the block body rustfmt gives a long arm (`Variant => { "code" }`) as
// well as the one-line form. A miss here is silent — the code simply drops out of the set the parity is
// held to — which is why the variant names are cross-checked against `ALL` below.
const AS_STR_ARM = /ErrorCode::(\w+)\s*=>\s*\{?\s*"([a-z_]+)"/g;

const AS_STR_HEADER = "pub const fn as_str(self) -> &'static str {";

const asStrArms = () => [...bodyAfter(coreErrorRs, AS_STR_HEADER, "{", "}").matchAll(AS_STR_ARM)];

function coreCodesFromRust(): string[] {
  return [...new Set(asStrArms().map((m) => m[2]))];
}

function coreVariantsFromAsStr(): string[] {
  return [...new Set(asStrArms().map((m) => m[1]))];
}

function coreVariantsFromAll(): string[] {
  const body = bodyAfter(coreErrorRs, "pub const ALL: &'static [ErrorCode] = &[", "[", "]");
  return [...new Set([...body.matchAll(/ErrorCode::(\w+)/g)].map((m) => m[1]))];
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
  // The parity below is only as good as the reading that feeds it: an arm the pattern does not match is
  // a code that never enters the comparison, so both sides agree about a set with a hole in it and the
  // test stays green. `ErrorCode::ALL` is core's own declaration of the whole set, written in a plainer
  // shape, so holding one reading to the other is what makes a missed arm fail instead of disappear.
  it("reads every arm core declares in ErrorCode::ALL", () => {
    expect(sorted(coreVariantsFromAsStr())).toEqual(sorted(coreVariantsFromAll()));
  });

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
