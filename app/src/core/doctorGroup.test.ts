// A real store measured 411 issues of one kind. What the settings panel draws has to survive that
// without becoming a wall nobody reads — and without hiding the one row somebody could have acted on.
import { describe, it, expect } from "vitest";
import { DOCTOR_LIST_CAP, groupDoctorIssues } from "./doctorKinds";

const dead = (n: number) =>
  Array.from({ length: n }, (_, i) => ({ kind: "dead_ref", params: { ref: `AMB-T-${i}` } }));

/** A stale managed block carries a one-click resync, so it is a repairable row. */
const stale = (dir: string) => ({ kind: "stale_managed_block", params: { dir } });

describe("groupDoctorIssues", () => {
  it("keeps a kind whole while it is under the cap", () => {
    const [group] = groupDoctorIssues(dead(3));
    expect(group.shown).toHaveLength(3);
    expect(group.withheld).toBe(0);
  });

  it("names the cap's worth and counts the rest", () => {
    const [group] = groupDoctorIssues(dead(411));
    expect(group.shown).toHaveLength(DOCTOR_LIST_CAP);
    expect(group.withheld).toBe(411 - DOCTOR_LIST_CAP);
  });

  // The point of capping per kind rather than over the whole report: a lone actionable issue must not
  // be pushed out by hundreds of a noisier one.
  it("caps each kind on its own, so a rare kind is never crowded out", () => {
    const groups = groupDoctorIssues([...dead(411), stale("/tmp/a")]);
    expect(groups.map((g) => g.kind)).toEqual(["stale_managed_block", "dead_ref"]);
    expect(groups.find((g) => g.kind === "stale_managed_block")!.shown).toHaveLength(1);
  });

  // Withholding a message spares the reader; withholding a button takes away the only way to act.
  it("draws every repairable row, however many of its kind there are", () => {
    const dirs = Array.from({ length: 25 }, (_, i) => stale(`/tmp/${i}`));
    const [group] = groupDoctorIssues(dirs);
    expect(group.shown).toHaveLength(25);
    expect(group.withheld).toBe(0);
  });

  it("returns nothing for a clean report", () => {
    expect(groupDoctorIssues([])).toEqual([]);
  });

  // The order is the contract's (`DoctorIssueKind::ALL`), not the order core happened to emit them in.
  it("orders the groups by the contract, not by arrival", () => {
    const groups = groupDoctorIssues([
      { kind: "dead_ref", params: {} },
      { kind: "self_dependency", params: {} },
    ]);
    expect(groups.map((g) => g.kind)).toEqual(["self_dependency", "dead_ref"]);
  });
});
