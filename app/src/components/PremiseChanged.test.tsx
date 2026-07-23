// @vitest-environment jsdom
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { PremiseChangedChip, StatusSelect } from "./atoms";
import { subscribeNotice } from "../core/notice";
import type { TaskCard } from "../mock/types";
import type { PremiseChangeDto } from "../bindings/bindings";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

function card(over: Partial<TaskCard>): TaskCard {
  return {
    id: 1, title: "t", ref: "#1", notes: "", projectId: null, status: "todo",
    assignee: null, priority: null, due: null, dueLabel: null, completedAt: null,
    comments: 0, ready: true, blockedBy: [], placement: null, createdBy: null,
    linkedDecisions: [], blockedByDecisions: [], notStartedUntil: null,
    ...over,
  };
}

const change = (over: Partial<PremiseChangeDto> = {}): PremiseChangeDto => ({
  addedBlockers: [], addedDecisions: [], reopenedDecisions: [], ...over,
});

const chips = () => Array.from(container.querySelectorAll(".chip--premise"));
const glyphs = () => Array.from(container.querySelectorAll(".chip--blockglyph"));

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("PremiseChangedChip", () => {
  it("shows nothing when no premise changed after reservation", () => {
    act(() => root.render(createElement(PremiseChangedChip, { task: card({}) })));
    expect(chips()).toHaveLength(0);
  });

  it("shows 🔔 with a count and names the added blockers and decisions in the tooltip", () => {
    act(() => root.render(createElement(PremiseChangedChip, { task: card({
      status: "in_progress",
      premiseChange: change({
        addedBlockers: [{ id: 2, name: "後付けブロッカー" }],
        addedDecisions: [{ id: 159, name: "未採択の決定", ref: "D-159" }],
      }),
    }) })));
    expect(chips()).toHaveLength(1);
    expect(chips()[0].textContent).toContain("🔔");
    expect(chips()[0].textContent).toContain("2"); // one blocker + one decision
    expect(chips()[0].getAttribute("title")).toContain("AMB-T-2 後付けブロッカー");
    expect(chips()[0].getAttribute("title")).toContain("D-159");
    expect(chips()[0].getAttribute("aria-label")).toContain("後付けブロッカー");
  });

  it("counts a ground that stopped being settled under the holder, and names it", () => {
    act(() => root.render(createElement(PremiseChangedChip, { task: card({
      status: "in_progress",
      premiseChange: change({
        reopenedDecisions: [{ id: 373, name: "開き直った決定", ref: "D-373" }],
      }),
    }) })));
    expect(chips()).toHaveLength(1);
    expect(chips()[0].textContent).toContain("1");
    expect(chips()[0].getAttribute("title")).toContain("D-373 開き直った決定");
  });

  it("compact drops the count and shows only the glyph, tooltip still names what changed", () => {
    act(() => root.render(createElement(PremiseChangedChip, { task: card({
      status: "in_progress",
      premiseChange: change({ addedBlockers: [{ id: 2, name: "後付け" }] }),
    }), compact: true })));
    expect(chips()).toHaveLength(0);
    expect(glyphs()).toHaveLength(1);
    expect(glyphs()[0].textContent).toBe("🔔");
    expect(glyphs()[0].getAttribute("title")).toContain("AMB-T-2 後付け");
  });
});

describe("StatusSelect premise-change safety net (AMB-D-366)", () => {
  function fireChange(props: Parameters<typeof StatusSelect>[0], next: string): string[] {
    const notices: string[] = [];
    const unsub = subscribeNotice((m) => notices.push(m));
    act(() => root.render(createElement(StatusSelect, props)));
    const select = container.querySelector("select") as HTMLSelectElement;
    act(() => {
      select.value = next;
      select.dispatchEvent(new Event("change", { bubbles: true }));
    });
    unsub();
    return notices;
  }

  it("warns firmly when a holder leaves in_progress and a premise was added after reservation", () => {
    let got: [number, string] | null = null;
    const notices = fireChange({
      id: 7, status: "in_progress", onStatus: (id, s) => { got = [id, s]; },
      premiseChange: change({ addedBlockers: [{ id: 2, name: "後付け" }] }),
    }, "done");
    expect(notices).toHaveLength(1);
    expect(notices[0]).toContain("後付け");
    // Surface, not veto: the transition is still handed on.
    expect(got).toEqual([7, "done"]);
  });

  it("stays silent when there is no premise change", () => {
    const notices = fireChange({
      id: 7, status: "in_progress", onStatus: () => {},
    }, "done");
    expect(notices).toHaveLength(0);
  });

  it("stays silent for a transition that is not leaving in_progress", () => {
    const notices = fireChange({
      id: 7, status: "todo", onStatus: () => {},
      premiseChange: change({ addedBlockers: [{ id: 2, name: "後付け" }] }),
    }, "in_progress");
    expect(notices).toHaveLength(0);
  });
});
