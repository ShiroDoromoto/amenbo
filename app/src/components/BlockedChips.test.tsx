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
    assignee: null, priority: null, due: null, dueLabel: null, completedAt: null,
    comments: 0, ready: true, blockedBy: [], placement: null, createdBy: null,
    linkedDecisions: [], blockedByDecisions: [], notStartedUntil: null,
    ...over,
  };
}

const chips = () => Array.from(container.querySelectorAll(".chip--block"));
const glyphs = () => Array.from(container.querySelectorAll(".chip--blockglyph"));
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

  it("shows incomplete blockers as ⛔ with a count and names them in the tooltip", () => {
    render(card({
      ready: false,
      blockedBy: [{ id: 2, name: "先行A" }, { id: 3, name: "先行B" }],
    }));
    expect(chips()).toHaveLength(1);
    expect(chips()[0].textContent).toContain("⛔");
    expect(chips()[0].textContent).toContain("2");
    expect(chips()[0].getAttribute("title")).toContain("AMB-T-2 先行A");
    expect(chips()[0].getAttribute("title")).toContain("AMB-T-3 先行B");
    expect(chips()[0].getAttribute("aria-label")).toContain("AMB-T-3 先行B");
  });

  it("shows unsettled basis decisions separately as ⚠ and names the ref", () => {
    render(card({
      ready: false,
      blockedByDecisions: [{ id: 159, name: "ready を書き込み層で強制", ref: "D-159" }],
    }));
    expect(chips()).toHaveLength(1);
    expect(chips()[0].textContent).toContain("⚠");
    expect(chips()[0].getAttribute("title")).toContain("D-159");
  });

  it("shows a start day that has not come as ⏳, carrying the day itself rather than a count", () => {
    render(card({ ready: false, notStartedUntil: "2026-08-01" }));
    expect(chips()).toHaveLength(1);
    expect(chips()[0].textContent).toContain("⏳");
    expect(chips()[0].textContent).toContain("2026-08-01");
    expect(chips()[0].getAttribute("title")).toContain("2026-08-01");
  });

  it("shows all three when all three hold it back — no reason hides behind another", () => {
    render(card({
      ready: false,
      blockedBy: [{ id: 2, name: "先行" }],
      blockedByDecisions: [{ id: 159, name: "根拠", ref: "D-159" }],
      notStartedUntil: "2026-08-01",
    }));
    expect(chips()).toHaveLength(3);
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
  it("shows only the glyph, with no count or chip background", () => {
    renderCompact(card({
      ready: false,
      blockedBy: [{ id: 2, name: "先行A" }, { id: 3, name: "先行B" }],
    }));
    expect(chips()).toHaveLength(0);
    expect(glyphs()).toHaveLength(1);
    expect(glyphs()[0].textContent).toBe("⛔");
  });

  it("even glyph-only, the tooltip names what is blocking", () => {
    renderCompact(card({
      ready: false,
      blockedByDecisions: [{ id: 159, name: "根拠", ref: "D-159" }],
    }));
    expect(glyphs()[0].textContent).toBe("⚠");
    expect(glyphs()[0].getAttribute("title")).toContain("D-159");
    expect(glyphs()[0].getAttribute("aria-label")).toContain("D-159");
  });

  it("drops the date and keeps the glyph, with the day still in the tooltip", () => {
    renderCompact(card({ ready: false, notStartedUntil: "2026-08-01" }));
    expect(chips()).toHaveLength(0);
    expect(glyphs()[0].textContent).toBe("⏳");
    expect(glyphs()[0].getAttribute("title")).toContain("2026-08-01");
  });

  it("shows nothing even on a dense surface when ready", () => {
    renderCompact(card({ ready: true, blockedBy: [{ id: 2, name: "先行" }] }));
    expect(glyphs()).toHaveLength(0);
  });
});
