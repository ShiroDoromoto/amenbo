// The single export point for doctor's issue kinds on the GUI side.
//
// On the producing side (Rust) the kinds are gathered in the typed registry
// `amenbo_core::doctor::DoctorIssueKind` (`as_str()` / `ALL` / `param_keys()` in
// `crates/amenbo-core/src/doctor.rs`). core holds no sentence — it returns only the kind (the id of a message
// template) and the params (the specifics), and each surface composes the sentence to read (the GUI localises
// through the `doctor` section of each dictionary in `i18n/locales/`; the CLI is English-only).
//
// This is the sole TS definition of the kinds the webview sees. The Rust↔TS parity test in `doctorKinds.test.ts`
// catches drift in both the set of kinds and the params a template may interpolate (`param_keys`).

/** The kinds of issue doctor can raise (same order as `DoctorIssueKind::ALL`). */
export const DOCTOR_ISSUE_KINDS = [
  "self_dependency",
  "duplicate_order_key",
  "orphan_attachment",
  "stale_managed_block",
  "legacy_pointer",
  "legacy_pointer_ambiguous",
  "missing_pointer",
  "missing_pointer_ambiguous",
  "orphan_binding",
  "dead_ref",
  "start_after_due",
  "project_without_folder",
  "unwired_folder",
  "unwired_folder_ambiguous",
] as const;

/** The type of a contract kind (single source). The DOCTOR table is a `Record` over it with every key required, so a new kind does not typecheck until its message is written. */
export type DoctorIssueKind = (typeof DOCTOR_ISSUE_KINDS)[number];

/** Whether a string is a contract kind (with narrowing). */
export function isDoctorIssueKind(s: string): s is DoctorIssueKind {
  return (DOCTOR_ISSUE_KINDS as readonly string[]).includes(s);
}

/** A repair the doctor surface can drive from the row alone. Core's cleanup entry point ("repair" = unreferenced
 *  blobs and leftover folder rows) does not fix the binding-shaped issues, so those go straight from the issue's
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

/**
 * How many issues of one kind the screen names one by one before it stops.
 *
 * The same cap, and the same reason, as the terminal's (`HUMAN_LIST_CAP` in `doctor_text.rs`): a real
 * store measured 411 dead refs, and a panel that long is not read — it just makes the settings screen
 * look like a disaster. Per kind, so a rare issue is never crowded out by a common one.
 */
export const DOCTOR_LIST_CAP = 10;

/** One kind's issues, as the screen draws them: the ones it names, and how many it did not. */
export type DoctorGroup<I> = {
  kind: string;
  /** The issues to draw, in arrival order — every repairable one, then messages up to the cap. */
  shown: I[];
  /** How many of this kind were left unnamed. Zero means the group is complete. */
  withheld: number;
};

/**
 * Fold the report into what the screen draws: **grouped by kind, capped, in the contract's order**.
 *
 * **The cap withholds messages, never actions.** An issue carrying a one-click repair is always drawn,
 * however many of its kind there are — hiding a button behind "… and 400 more" would take away the
 * only way to act on it, which is a different thing from sparing the reader a wall of text.
 *
 * A kind with no issues is not in the result, so the caller draws groups and nothing else.
 */
export function groupDoctorIssues<I extends { kind: string; params: Record<string, string> }>(
  issues: I[],
  cap: number = DOCTOR_LIST_CAP,
): DoctorGroup<I>[] {
  const groups: DoctorGroup<I>[] = [];
  for (const kind of DOCTOR_ISSUE_KINDS) {
    const ofKind = issues.filter((i) => i.kind === kind);
    if (ofKind.length === 0) continue;
    let budget = cap;
    const shown = ofKind.filter((i) => {
      if (doctorRepair(i)) return true;
      if (budget <= 0) return false;
      budget -= 1;
      return true;
    });
    groups.push({ kind, shown, withheld: ofKind.length - shown.length });
  }
  return groups;
}
