import { describe, expect, it } from "vitest";
import { edgeCandidates, edgeRows, promotesToAccepted, standingOn } from "./decisionEdges";
import type { Decision } from "./snapshot";

function dec(over: Partial<Decision> & { id: number }): Decision {
  return {
    ref: `D-${over.id}`,
    title: `決定 ${over.id}`,
    body: "",
    status: "accepted",
    current: true,
    project: { id: 1, name: "amenbo" },
    supersedes: [],
    supersededBy: [],
    amends: [],
    amendedBy: [],
    buildsOn: [],
    builtOnBy: [],
    linkedTasks: [],
    createdAt: "2026-07-12T00:00:00Z",
    ...over,
  } as Decision;
}

const ref = (id: number) => ({ id, name: `決定 ${id}`, ref: `D-${id}` });

describe("edgeRows", () => {
  it("forward edges point self-as-newer to other-as-older (the direction unlink can take directly)", () => {
    const d = dec({ id: 10, supersedes: [ref(3)], amends: [ref(4)], buildsOn: [{ ...ref(5) }] });
    expect(edgeRows(d).map((r) => [r.labelKey, r.from, r.to])).toEqual([
      ["dec.supersedes", 10, 3],
      ["dec.amends", 10, 4],
      ["dec.buildsOn", 10, 5],
    ]);
  });

  it("back-reference edges put other-as-newer and self-as-older, so the direction flips (the pair is ordered)", () => {
    const d = dec({ id: 10, supersededBy: [ref(20)], amendedBy: [ref(21)], builtOnBy: [ref(22)] });
    expect(edgeRows(d).map((r) => [r.labelKey, r.from, r.to])).toEqual([
      ["dec.supersededBy", 20, 10],
      ["dec.amendedBy", 21, 10],
      ["dec.builtOnBy", 22, 10],
    ]);
  });

  it("a superseded premise carries staleBy (only on builds_on rows)", () => {
    const d = dec({ id: 10, buildsOn: [{ ...ref(5), supersededBy: "D-40" }] });
    expect(edgeRows(d)[0].staleBy).toBe("D-40");
    expect(edgeRows(dec({ id: 10, supersedes: [ref(3)] }))[0].staleBy).toBeUndefined();
  });
});

describe("edgeCandidates", () => {
  const self = dec({ id: 10, supersedes: [ref(3)], supersededBy: [ref(20)] });
  const all = [self, dec({ id: 3 }), dec({ id: 20 }), dec({ id: 7, title: "台帳を末尾から読む" })];

  it("excludes itself and already-linked decisions (in either direction) from candidates", () => {
    expect(edgeCandidates(all, self, "").map((c) => c.id)).toEqual([7]);
  });

  it("narrows by partial match on ref and title (case-insensitive)", () => {
    expect(edgeCandidates(all, self, "d-7").map((c) => c.id)).toEqual([7]);
    expect(edgeCandidates(all, self, "台帳").map((c) => c.id)).toEqual([7]);
    expect(edgeCandidates(all, self, "存在しない")).toEqual([]);
  });
});

describe("promotesToAccepted", () => {
  it("promotes only when drawing supersedes from a decision under discussion", () => {
    const proposed = dec({ id: 10, status: "proposed" });
    expect(promotesToAccepted(proposed, "supersedes")).toBe(true);
    expect(promotesToAccepted(proposed, "amends")).toBe(false);
    expect(promotesToAccepted(proposed, "buildsOn")).toBe(false);
  });

  it("an already-accepted or rejected decision does not promote even on supersedes (core only raises Proposed)", () => {
    expect(promotesToAccepted(dec({ id: 10, status: "accepted" }), "supersedes")).toBe(false);
    expect(promotesToAccepted(dec({ id: 10, status: "rejected" }), "supersedes")).toBe(false);
  });
});

describe("standingOn", () => {
  it("gathers back-references of all three kinds (the correcting side also stands on that content; same set as the CLI's standing_on)", () => {
    const d = dec({ id: 10, supersededBy: [ref(20)], amendedBy: [ref(21)], builtOnBy: [ref(22)] });
    expect(standingOn(d).map((s) => s.id)).toEqual([20, 21, 22]);
  });

  it("forward edges (the premises self stands on) are not the blast radius: empty when no one stands on it", () => {
    expect(standingOn(dec({ id: 10, supersedes: [ref(3)], buildsOn: [{ ...ref(5) }] }))).toEqual([]);
  });
});
