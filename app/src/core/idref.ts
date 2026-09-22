// The GUI's side of the ref spelling — the mirror of core's `idref.rs`, which is where the format is
// decided. Every exposed Amenbo ref is `AMB-<kind>-<n>`.
//
// Almost every ref the GUI shows arrives already rendered, as the backend's `ref` field; this module is for
// the few places that hold nothing but an id (the id chip, an optimistic row created before the backend has
// answered) and for reading refs back out of body text.
//
// Why the namespace: a bare `T-123` is another tracker's ref as much as ours — Jira keys are free-form —
// so no amount of checking against the store tells the two apart when the numbers coincide. `AMB-` makes
// the ref self-declaring, which is what lets body-text detection be a pure pattern.

/** The prefix every user-visible Amenbo ref carries. */
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
 * still accepted, matching what core's parser takes, because text a user hands Amenbo directly — typing
 * into the search box — is not the foreign text the namespace guards against.
 *
 * `side` is what a number with no type code is read as. It carries nothing that says which space it is in,
 * and the two number themselves apart (`AMB-D-29`), so `12` is task 12 as much as decision 12 — readable
 * only where the side is already settled, which is a box that searches one of them (`AMB-D-833`). Called
 * with no side, a bare number is not a ref at all and the caller is left to decide for itself.
 */
export function parseRef(raw: string, side?: RefSpace): { num: number; space: RefSpace } | null {
  const s = raw.trim();
  let m: RegExpExecArray | null;
  if ((m = /^(?:AMB-)?[Tt]-(\d+)$/i.exec(s))) return { num: Number(m[1]), space: "task" };
  if ((m = /^(?:AMB-)?[Dd]-(\d+)$/i.exec(s))) return { num: Number(m[1]), space: "decision" };
  if (side && (m = /^#?(\d+)$/.exec(s))) return { num: Number(m[1]), space: side };
  if ((m = /^#(\d+)$/.exec(s))) return { num: Number(m[1]), space: "task" };
  return null;
}

/**
 * The automation an `AMB-AUT-<n>` names, or null where the text is not one.
 *
 * **Read apart from `parseRef`, because an automation is not one of the two spaces.** A space travels
 * as a space and a number through the pane's links and the made-in chip (`RefSpace`), where what is
 * named is a record the board opens on its own. An automation is read inside its build screen
 * (`AMB-D-944`), so what its ref carries is a number and a place that already knows what to do with
 * one — the search row (`../screens/SearchScreen`).
 *
 * The namespaced form only: this reads a ref Amenbo rendered, never text somebody typed.
 */
export function automationRefNum(raw: string): number | null {
  const m = /^AMB-AUT-(\d+)$/i.exec(raw.trim());
  return m ? Number(m[1]) : null;
}
