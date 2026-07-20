// The single export point for doctor's issue kinds on the GUI side.
//
// On the producing side (Rust) the kinds are gathered in the typed registry
// `amenbo_core::doctor::DoctorIssueKind` (`as_str()` / `ALL` / `param_keys()` in
// `crates/amenbo-core/src/doctor.rs`). core holds no sentence — it returns only the kind (the id of a message
// template) and the params (the specifics), and each surface composes the sentence to read (the GUI localises
// through the DOCTOR table in `i18n.ts`; the CLI is English-only).
//
// This is the sole TS definition of the kinds the webview sees. The Rust↔TS parity test in `doctorKinds.test.ts`
// catches drift in both the set of kinds and the params a template may interpolate (`param_keys`).

/** The kinds of issue doctor can raise (same order as `DoctorIssueKind::ALL`). */
export const DOCTOR_ISSUE_KINDS = [
  "self_dependency",
  "duplicate_order_key",
  "stale_managed_block",
  "legacy_pointer",
  "legacy_pointer_ambiguous",
  "missing_pointer",
  "missing_pointer_ambiguous",
  "orphan_binding",
  "dead_ref",
  "start_after_due",
] as const;

/** The type of a contract kind (single source). The DOCTOR table is a `Record` over it with every key required, so a new kind does not typecheck until its message is written. */
export type DoctorIssueKind = (typeof DOCTOR_ISSUE_KINDS)[number];

/** Whether a string is a contract kind (with narrowing). */
export function isDoctorIssueKind(s: string): s is DoctorIssueKind {
  return (DOCTOR_ISSUE_KINDS as readonly string[]).includes(s);
}

/** A repair the doctor surface can drive from the row alone. Core's cleanup entry point ("repair" = orphan comment
 *  rows, blobs, leftover folder rows) does not fix the binding-shaped issues, so those go straight from the issue's
 *  params to bind / resync. */
export type DoctorRepair =
  | { action: "rebind"; dir: string; project: number }
  | { action: "resync"; dir: string };

/**
 * Whether the issue can be fixed in one click — and if so, the repair to run.
 * The target follows from the kind; we never read the params to decide whether it is unambiguous. When the binding
 * target is not determined, core hands it over as a separate `*_ambiguous` kind, so such issues never arrive here
 * and **never become a button** — nothing quietly picks a different project (the design of `binding.rs`). Kinds
 * with nowhere to go (orphan references, duplicate order keys) return null too, leaving just the sentence.
 */
export function doctorRepair(issue: { kind: string; params: Record<string, string> }): DoctorRepair | null {
  const dir = issue.params.dir ?? "";
  if (!dir) return null;
  switch (issue.kind) {
    case "legacy_pointer":
    case "missing_pointer": {
      const project = Number(issue.params.project);
      return Number.isSafeInteger(project) && project > 0 ? { action: "rebind", dir, project } : null;
    }
    case "stale_managed_block":
      return { action: "resync", dir };
    default:
      return null;
  }
}
