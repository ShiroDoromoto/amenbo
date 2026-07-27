// Rust↔TS parity for doctor issues. Extracts core's kind registry from the real source and holds it against the
// TS single source (doctorKinds.ts) and the GUI's wording table (the `doctor` section in i18n/locales/). Adding or renaming a kind on
// one side only, or a template referring via `{...}` to params core never sends, would surface in production as
// nothing more than a hole in a sentence — invisible. So it breaks here instead.
import { describe, expect, it } from "vitest";
import { DOCTOR_ISSUE_KINDS, type DoctorIssueKind } from "./doctorKinds";
import { doctorText } from "./i18n";

// The kind strings and their params are defined nowhere but the match arms of `DoctorIssueKind`'s `as_str()` / `param_keys()`.
import doctorRs from "../../../crates/amenbo-core/src/doctor.rs?raw";

/** Cut out the body of exactly one method, so that `severity()`'s `Self::A | Self::B => "error"` is never mistaken for a kind. */
function methodBody(name: string): string {
  const start = doctorRs.indexOf(`pub const fn ${name}(self)`);
  if (start < 0) throw new Error(`DoctorIssueKind::${name}() is gone from doctor.rs`);
  const end = doctorRs.indexOf("\n    }\n", start);
  return doctorRs.slice(start, end);
}

/** Variant → kind, read from Rust's `Self::Variant => "kind"` (as_str). */
function kindByVariant(): Map<string, string> {
  const m = new Map<string, string>();
  for (const hit of methodBody("as_str").matchAll(/Self::(\w+) => "([a-z_]+)",/g)) m.set(hit[1], hit[2]);
  return m;
}

/** kind → its set of param keys, read from Rust's `Self::Variant => &["a", "b"],` (param_keys). */
function paramKeysByKind(): Map<string, string[]> {
  const variants = kindByVariant();
  const m = new Map<string, string[]>();
  for (const hit of methodBody("param_keys").matchAll(/Self::(\w+) => &\[([^\]]*)\],/g)) {
    const kind = variants.get(hit[1]);
    if (!kind) continue;
    m.set(kind, [...hit[2].matchAll(/"([a-z_]+)"/g)].map((k) => k[1]));
  }
  return m;
}

const sorted = (xs: readonly string[]) => [...xs].sort();

describe("doctor issue kind Rust↔TS parity", () => {
  it("the TS single source is exactly the Rust registry", () => {
    expect(sorted([...kindByVariant().values()])).toEqual(sorted(DOCTOR_ISSUE_KINDS));
  });

  it("has no duplicate kinds", () => {
    expect(new Set(DOCTOR_ISSUE_KINDS).size).toBe(DOCTOR_ISSUE_KINDS.length);
  });

  it("every kind reads in both languages, with its params filled in", () => {
    const byKind = paramKeysByKind();
    for (const kind of DOCTOR_ISSUE_KINDS) {
      const keys = byKind.get(kind);
      expect(keys, `${kind}: core declares no param_keys`).toBeDefined();
      // What core actually sends, with recognisable marker values. These are the only keys a template may reference with `{...}`.
      const params = Object.fromEntries(keys!.map((k) => [k, `<${k}>`]));
      for (const lang of ["ja", "en"] as const) {
        const { message, fixHint } = doctorText({ kind, params }, lang);
        for (const text of [message, fixHint]) {
          expect(text, `${kind}/${lang}: the face has no wording`).not.toBe("");
          expect(text, `${kind}/${lang}: an unfilled placeholder is left in ${text}`).not.toMatch(/\{\w+\}/);
        }
        expect(message, `${kind}/${lang}: the message never says what is broken`).toContain("<");
      }
    }
  });

  it("falls back to the raw kind when a newer core sends one this build does not know", () => {
    const unknown = "some_future_issue" as DoctorIssueKind;
    expect(doctorText({ kind: unknown, params: {} }, "en")).toEqual({ message: unknown, fixHint: "" });
  });
});
