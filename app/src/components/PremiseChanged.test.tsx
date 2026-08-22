// @vitest-environment jsdom
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { PremiseChangedChip, PremiseChangedField, StatusSelect } from "./atoms";
import { subscribeNotice } from "../core/notice";
import { t } from "../core/i18n";
import type { TaskCard } from "../mock/types";
import type { PremiseChangeDto } from "../bindings/bindings";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

function card(over: Partial<TaskCard>): TaskCard {
  return {
    id: 1, title: "t", ref: "#1", notes: "", projectId: null, status: "todo",
    assignee: null, priority: null, due: null, completedAt: null,
    comments: 0, ready: true, blockedBy: [], placement: null, createdBy: null,
    linkedDecisions: [], blockedByDecisions: [], startOn: null, notStartedUntil: null, draft: false,
    createdAt: "2026-06-01T09:00:00Z", updatedAt: "2026-06-01T09:00:00Z",
    ...over,
  };
}

const change = (over: Partial<PremiseChangeDto> = {}): PremiseChangeDto => ({
  addedBlockers: [], addedDecisions: [], reopenedDecisions: [], ...over,
});

const chips = () => Array.from(container.querySelectorAll(".chip--premise"));
const glyphs = () => Array.from(container.querySelectorAll(".chip--blockglyph"));
// Which mark an element drew. The icons carry no text, so the name is read off the svg itself.
const markOf = (el: Element) => el.querySelector("svg")?.getAttribute("data-icon");

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

  it("shows a bell with a count and names the added blockers and decisions in the tooltip", () => {
    act(() => root.render(createElement(PremiseChangedChip, { task: card({
      status: "in_progress",
      premiseChange: change({
        addedBlockers: [{ id: 2, name: "後付けブロッカー" }],
        addedDecisions: [{ id: 159, name: "未採択の決定", ref: "D-159" }],
      }),
    }) })));
    expect(chips()).toHaveLength(1);
    expect(markOf(chips()[0])).toBe("bell");
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

  it("tells the two axes apart in the tooltip — pinned on, versus settlement come off", () => {
    act(() => root.render(createElement(PremiseChangedChip, { task: card({
      status: "in_progress",
      premiseChange: change({
        addedDecisions: [{ id: 159, name: "後から張られた決定", ref: "D-159" }],
        reopenedDecisions: [{ id: 373, name: "開き直った決定", ref: "D-373" }],
      }),
    }) })));
    const title = chips()[0].getAttribute("title") ?? "";
    // The tag sits on the reopened side only; without it the two read as one list.
    expect(title).toContain(`${t("premise.noLongerSettled")}: D-373 開き直った決定`);
    expect(title.indexOf("D-159")).toBeLessThan(title.indexOf(t("premise.noLongerSettled")));
  });

  it("compact drops the count and shows only the mark, tooltip still names what changed", () => {
    act(() => root.render(createElement(PremiseChangedChip, { task: card({
      status: "in_progress",
      premiseChange: change({ addedBlockers: [{ id: 2, name: "後付け" }] }),
    }), compact: true })));
    expect(chips()).toHaveLength(0);
    expect(glyphs()).toHaveLength(1);
    expect(markOf(glyphs()[0])).toBe("bell");
    expect(glyphs()[0].textContent).toBe("");
    expect(glyphs()[0].getAttribute("title")).toContain("AMB-T-2 後付け");
  });
});

describe("PremiseChangedField (the detail pane's spelled-out surface)", () => {
  const fieldChips = () => Array.from(container.querySelectorAll("button.chip--link"));

  it("draws every axis, the reopened one included, and navigates to what it names", () => {
    const opened: number[] = [];
    act(() => root.render(createElement(PremiseChangedField, {
      pc: change({
        addedBlockers: [{ id: 2, name: "後付けブロッカー" }],
        addedDecisions: [{ id: 159, name: "後から張られた決定", ref: "D-159" }],
        reopenedDecisions: [{ id: 373, name: "開き直った決定", ref: "D-373" }],
      }),
      onSelectDecision: (id: number) => opened.push(id),
    })));
    const labels = fieldChips().map((c) => c.textContent ?? "");
    const marks = fieldChips().map(markOf);
    expect(labels).toHaveLength(3);
    expect(marks[0]).toBe("blocked");
    expect(labels[0]).toContain("後付けブロッカー");
    expect(marks[1]).toBe("warning");
    expect(labels[1]).toContain("D-159 後から張られた決定");
    // The axis the pane used to drop entirely: the link is older, the settlement is what went away.
    expect(marks[2]).toBe("unlock");
    expect(labels[2]).toContain("D-373 開き直った決定");
    act(() => { (fieldChips()[2] as HTMLButtonElement).click(); });
    expect(opened).toEqual([373]);
  });

  it("marks which axis each decision is on, so the two glyphs are readable", () => {
    act(() => root.render(createElement(PremiseChangedField, {
      pc: change({
        addedDecisions: [{ id: 159, name: "後から張られた決定", ref: "D-159" }],
        reopenedDecisions: [{ id: 373, name: "開き直った決定", ref: "D-373" }],
      }),
    })));
    expect(fieldChips()[0].getAttribute("title")).toBe(t("detail.premiseAdded"));
    expect(fieldChips()[1].getAttribute("title")).toBe(t("detail.premiseReopened"));
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

  it("names a ground that stopped being settled, not only the ones pinned on", () => {
    const notices = fireChange({
      id: 7, status: "in_progress", onStatus: () => {},
      premiseChange: change({ reopenedDecisions: [{ id: 373, name: "開き直った決定", ref: "D-373" }] }),
    }, "done");
    // The toast once fired with an empty detail here: the axis was in the chip and missing from the warn.
    expect(notices).toHaveLength(1);
    expect(notices[0]).toContain("開き直った決定");
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
