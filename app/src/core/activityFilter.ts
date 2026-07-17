// Activity filtering, on two orthogonal axes:
//   kind:  system (events)  / comment (what someone said)
//   facet: human            / ai
// The screen drives this predicate from two groups of chips (single selection per group,
// defaulting to all). The CLI's `activity` has no equivalent filter — it is a plain chronological
// window over everything — so this predicate is the client's source of truth.
import type { ActivityItem } from "../mock/types";

export type ActivityKindFilter = "all" | "system" | "comment";
export type ActivityFacetFilter = "all" | "human" | "ai";

/** ANDs the two axes. "all" on an axis means unspecified — that axis does not narrow anything. */
export function matchesActivityFilter(
  it: ActivityItem,
  kind: ActivityKindFilter,
  facet: ActivityFacetFilter,
): boolean {
  return (kind === "all" || it.kind === kind) && (facet === "all" || it.author.kind === facet);
}
