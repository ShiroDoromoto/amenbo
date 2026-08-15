// @vitest-environment jsdom
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { BlockedChips } from "./atoms";
import type { TaskCard } from "../mock/types";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

function card(over: Partial<TaskCard>): TaskCard {
  return {
    id: 1, title: "t", ref: "#1", notes: "", projectId: null, status: "todo",
    assignee: null, priority: null, due: null, completedAt: null,
    comments: 0, ready: true, blockedBy: [], placement: null, createdBy: null,
    linkedDecisions: [], blockedByDecisions: [], notStartedUntil: null, draft: false,
    ...over,
  };
}

const chips = () => Array.from(container.querySelectorAll(".chip--block"));
const glyphs = () => Array.from(container.querySelectorAll(".chip--blockglyph"));
// Which mark a chip drew. The icons carry no text, so the name is read off the svg itself.
const markOf = (el: Element) => el.querySelector("svg")?.getAttribute("data-icon");
const render = (task: TaskCard) => act(() => root.render(createElement(BlockedChips, { task })));
const renderCompact = (task: TaskCard) =>
  act(() => root.render(createElement(BlockedChips, { task, compact: true })));

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("BlockedChips", () => {
  it("shows nothing when ready", () => {
    render(card({ ready: true, blockedBy: [{ id: 2, name: "先行" }] }));
    expect(chips()).toHaveLength(0);
  });

  it("shows incomplete blockers as a barred circle with a count and names them in the tooltip", () => {
    render(card({
      ready: false,
      blockedBy: [{ id: 2, name: "先行A" }, { id: 3, name: "先行B" }],
    }));
    expect(chips()).toHaveLength(1);
    expect(markOf(chips()[0])).toBe("blocked");
    expect(chips()[0].textContent).toContain("2");
    expect(chips()[0].getAttribute("title")).toContain("AMB-T-2 先行A");
    expect(chips()[0].getAttribute("title")).toContain("AMB-T-3 先行B");
    expect(chips()[0].getAttribute("aria-label")).toContain("AMB-T-3 先行B");
  });

  it("shows unsettled basis decisions separately as a warning triangle and names the ref", () => {
    render(card({
      ready: false,
      blockedByDecisions: [{ id: 159, name: "ready を書き込み層で強制", ref: "D-159" }],
    }));
    expect(chips()).toHaveLength(1);
    expect(markOf(chips()[0])).toBe("warning");
    expect(chips()[0].getAttribute("title")).toContain("D-159");
  });

  it("shows a start day that has not come as an hourglass, carrying the day itself rather than a count", () => {
    render(card({ ready: false, notStartedUntil: "2026-08-01" }));
    expect(chips()).toHaveLength(1);
    expect(markOf(chips()[0])).toBe("hourglass");
    expect(chips()[0].textContent).toContain("2026-08-01");
    expect(chips()[0].getAttribute("title")).toContain("2026-08-01");
  });

  it("shows an unfinished creation as a pencil, naming the state rather than a count", () => {
    render(card({ ready: false, draft: true }));
    expect(chips()).toHaveLength(1);
    expect(markOf(chips()[0])).toBe("pencil");
    expect(chips()[0].textContent).toContain("Being created");
    expect(chips()[0].getAttribute("title")).toContain("finish creating it first");
  });

  it("shows all four when all four hold it back — no reason hides behind another", () => {
    render(card({
      ready: false,
      blockedBy: [{ id: 2, name: "先行" }],
      blockedByDecisions: [{ id: 159, name: "根拠", ref: "D-159" }],
      notStartedUntil: "2026-08-01",
      draft: true,
    }));
    expect(chips()).toHaveLength(4);
  });

  it("shows both when both are present", () => {
    render(card({
      ready: false,
      blockedBy: [{ id: 2, name: "先行" }],
      blockedByDecisions: [{ id: 159, name: "根拠", ref: "D-159" }],
    }));
    expect(chips()).toHaveLength(2);
  });

  it("shows even for in_progress as long as it is not ready (the display condition does not depend on status)", () => {
    render(card({ status: "in_progress", ready: false, blockedBy: [{ id: 2, name: "先行" }] }));
    expect(chips()).toHaveLength(1);
  });
});

describe("BlockedChips compact", () => {
  it("shows only the mark, with no count or chip background", () => {
    renderCompact(card({
      ready: false,
      blockedBy: [{ id: 2, name: "先行A" }, { id: 3, name: "先行B" }],
    }));
    expect(chips()).toHaveLength(0);
    expect(glyphs()).toHaveLength(1);
    expect(markOf(glyphs()[0])).toBe("blocked");
    expect(glyphs()[0].textContent).toBe("");
  });

  it("even mark-only, the tooltip names what is blocking", () => {
    renderCompact(card({
      ready: false,
      blockedByDecisions: [{ id: 159, name: "根拠", ref: "D-159" }],
    }));
    expect(markOf(glyphs()[0])).toBe("warning");
    expect(glyphs()[0].getAttribute("title")).toContain("D-159");
    expect(glyphs()[0].getAttribute("aria-label")).toContain("D-159");
  });

  it("drops the date and keeps the mark, with the day still in the tooltip", () => {
    renderCompact(card({ ready: false, notStartedUntil: "2026-08-01" }));
    expect(chips()).toHaveLength(0);
    expect(markOf(glyphs()[0])).toBe("hourglass");
    expect(glyphs()[0].textContent).toBe("");
    expect(glyphs()[0].getAttribute("title")).toContain("2026-08-01");
  });

  it("drops the word and keeps the mark for an unfinished creation", () => {
    renderCompact(card({ ready: false, draft: true }));
    expect(chips()).toHaveLength(0);
    expect(markOf(glyphs()[0])).toBe("pencil");
    expect(glyphs()[0].textContent).toBe("");
    expect(glyphs()[0].getAttribute("title")).toContain("finish creating it first");
  });

  it("shows nothing even on a dense surface when ready", () => {
    renderCompact(card({ ready: true, blockedBy: [{ id: 2, name: "先行" }] }));
    expect(glyphs()).toHaveLength(0);
  });
});
