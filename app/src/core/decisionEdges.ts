// Pure logic for the edges between decisions (supersedes / amends / builds_on) as the GUI handles them. It is
// kept apart from rendering because getting a row's direction wrong — which end is the newer one — makes unlink
// remove a different edge, and that is worth pinning down in tests.

import type { Decision, DecisionRef } from "./snapshot";

/** The edge kinds. These three are what the GUI can draw (decision → decision, always newer → older). */
export type EdgeKind = "supersedes" | "amends" | "buildsOn";

export const EDGE_KINDS: readonly EdgeKind[] = ["supersedes", "amends", "buildsOn"];

/** One edge row in the decision detail. `from`/`to` keep the newer → older direction the edge was drawn in, so they can go straight to unlink. */
export interface EdgeRow {
  /** i18n key of the display label (`dec.supersedes` and friends). */
  labelKey: string;
  /** The decision at the far end. `name` is `null` when a forward edge dangles (target no longer live). */
  target: { id: number; name: string | null; ref?: string };
  /** The side that drew the edge (the newer one). */
  from: number;
  /** The side it was drawn to (the older one). */
  to: number;
  /** If the premise has been overturned, the ref of the decision that overturned it (builds_on rows only). */
  staleBy?: string;
}

/**
 * Lay out the edges attached to one decision in display order: forward first, then the reverse ones. A reverse
 * edge (`supersededBy` and friends) was drawn by the other decision, so its `from`/`to` are swapped — get that
 * backwards and unlinking "from the superseded side" quietly does nothing (the pair is ordered).
 */
export function edgeRows(d: Decision): EdgeRow[] {
  const rows: EdgeRow[] = [];
  const forward = ([
    ["dec.supersedes", d.supersedes],
    ["dec.amends", d.amends],
  ] as const);
  for (const [labelKey, edges] of forward) {
    for (const e of edges) rows.push({ labelKey, target: e, from: d.id, to: e.id });
  }
  for (const p of d.buildsOn) {
    rows.push({ labelKey: "dec.buildsOn", target: p, from: d.id, to: p.id, staleBy: p.supersededBy });
  }
  const backward = ([
    ["dec.supersededBy", d.supersededBy],
    ["dec.amendedBy", d.amendedBy],
    ["dec.builtOnBy", d.builtOnBy],
  ] as const);
  for (const [labelKey, edges] of backward) {
    for (const e of edges) rows.push({ labelKey, target: e, from: e.id, to: d.id });
  }
  return rows;
}

/**
 * Blast radius: the decisions standing on this one (the reverse direction of all three kinds, **one hop only**).
 * They are offered as the decisions that "need another look" when superseding, rejecting or deleting — this never
 * blocks the operation (currency does not cascade automatically). All three kinds count because `supersedes` and
 * `amends` imply `builds_on`: whatever corrects a decision plainly stands on it. Same set as the CLI's
 * `standing_on`, so both surfaces show the same behaviour.
 */
export function standingOn(d: Decision): DecisionRef[] {
  return [...d.supersededBy, ...d.amendedBy, ...d.builtOnBy];
}

/**
 * Whether drawing this edge would silently promote the drawing side (this decision) to accepted. core's
 * `supersede` takes the view that "if it replaces something, it is settled" and lifts a `Proposed` new side to
 * `Accepted` — an ambush for a user who never pressed accept, so we say so before the edge is drawn (`amends`
 * and `builds_on` do not promote).
 */
export function promotesToAccepted(d: Decision, kind: EdgeKind): boolean {
  return kind === "supersedes" && d.status === "proposed";
}

/**
 * Candidates for the "draw an edge" picker. Excludes this decision itself and any decision already connected by
 * an edge in either direction: one pair holds one edge (`decision_edge_pair` is UNIQUE), so offering a connected
 * decision again would merely rewrite the kind, which reads misleadingly as a way to redraw the edge. `query` is
 * a substring match over ref and title.
 */
export function edgeCandidates(all: Decision[], d: Decision, query: string): Decision[] {
  const linked = new Set(edgeRows(d).map((r) => r.target.id));
  const q = query.trim().toLowerCase();
  return all.filter((c) => {
    if (c.id === d.id || linked.has(c.id)) return false;
    if (!q) return true;
    return c.ref.toLowerCase().includes(q) || c.title.toLowerCase().includes(q);
  });
}
