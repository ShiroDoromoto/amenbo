// How far a decision has got, said as a colour. Core owns the values (`DecisionStatus` in
// `snapshot.ts`); what lives here is the one reading of them a surface needs — which fill and ink
// pair the chip wears. Written once, because the two places that draw the chip used to hold the
// same three colours side by side and there was nothing to notice when one of them moved.
import type { DecisionStatus } from "./snapshot";

/**
 * The chip's modifier class for one decision. The unfinished writing is read before the status,
 * because the status has nothing to tell apart while it is up: a decision is `decided` from the
 * moment it is saved (`AMB-D-918`). The badge says how far the decision has got and nothing else
 * (`AMB-D-410`) — that it was overturned is an edge, and the edge list is where that is read.
 */
export function decisionStatusChip(s: DecisionStatus, draft: boolean): string {
  if (draft) return "chip--status-draft";
  switch (s) {
    case "decided": return "chip--status-decided";
    case "rejected": return "chip--status-rejected";
  }
}
