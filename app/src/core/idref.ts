// The GUI's side of the ref spelling — the mirror of core's `idref.rs`, which is where the format is
// decided. Every exposed amenbo ref is `AMB-<kind>-<n>`.
//
// Almost every ref the GUI shows arrives already rendered, as the backend's `ref` field; this module is for
// the few places that hold nothing but an id (the id chip, an optimistic row created before the backend has
// answered) and for reading refs back out of body text.
//
// Why the namespace: a bare `T-123` is another tracker's ref as much as ours — Jira keys are free-form —
// so no amount of checking against the store tells the two apart when the numbers coincide. `AMB-` makes
// the ref self-declaring, which is what lets body-text detection be a pure pattern.

/** The prefix every user-visible amenbo ref carries. */
export const NAMESPACE = "AMB";

export type RefSpace = "task" | "decision";

const CODE: Record<RefSpace, string> = { task: "T", decision: "D" };

/** A task's ref: `AMB-T-<n>`. */
export function taskRef(id: number): string {
  return `${NAMESPACE}-${CODE.task}-${id}`;
}

/** A decision's ref: `AMB-D-<n>`. */
export function decisionRef(id: number): string {
  return `${NAMESPACE}-${CODE.decision}-${id}`;
}

/**
 * The reference tokens picked out of body text: `AMB-T-<n>` / `AMB-D-<n>`, the kind code case-folded.
 *
 * Only the namespaced form is detected. A bare `#<n>` in a body is a GitHub/GitLab issue, not one of ours,
 * and a bare `T-<n>` is exactly the foreign-tracker collision the namespace exists to settle — linking
 * either would hijack a reference that was never about amenbo. The leading boundary (a negative lookbehind)
 * keeps `XAMB-T-<n>` from matching. Case folds, as core's parser does, so a lowercase ref still resolves.
 */
export const REF_RE = /(?<![A-Za-z0-9])AMB-[TD]-\d+/gi;

/**
 * Read a single ref, whole-string. Reading is the loose side: the bare `#<n>` / `T-<n>` / `D-<n>` forms are
 * still accepted, matching what core's parser takes, because text a user hands amenbo directly — typing
 * into the search box — is not the foreign text the namespace guards against.
 */
export function parseRef(raw: string): { num: number; space: RefSpace } | null {
  const s = raw.trim();
  let m: RegExpExecArray | null;
  if ((m = /^(?:AMB-)?[Tt]-(\d+)$/i.exec(s))) return { num: Number(m[1]), space: "task" };
  if ((m = /^(?:AMB-)?[Dd]-(\d+)$/i.exec(s))) return { num: Number(m[1]), space: "decision" };
  if ((m = /^#(\d+)$/.exec(s))) return { num: Number(m[1]), space: "task" };
  return null;
}
