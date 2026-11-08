// @vitest-environment jsdom
// The navigation for a face beside the ledger (`./refNav`'s `useLedgerNav`).
//
// **A task or a decision pressed on the workspace opens on the ledger**, which is brought in front of
// the workspace first: selected behind it, the press would look like a link that did nothing. The moves
// that go to the ledger by themselves pass through as they are.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { RefNavProvider, useLedgerNav, type RefNav } from "./refNav";

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

/** The navigation a component under `outer` reads, with `onOpenLedger` handed to `useLedgerNav`. */
function ledgerNavUnder(outer: RefNav, onOpenLedger?: () => void): RefNav {
  let read: RefNav = {};
  function Reader() {
    read = useLedgerNav(onOpenLedger);
    return null;
  }
  act(() => root.render(createElement(RefNavProvider, { value: outer, children: createElement(Reader) })));
  return read;
}

describe("useLedgerNav", () => {
  it("brings the ledger forward, then selects the task or the decision on it", () => {
    const order: string[] = [];
    const outer: RefNav = {
      selectTask: vi.fn(() => void order.push("task")),
      selectDecision: vi.fn(() => void order.push("decision")),
    };
    const nav = ledgerNavUnder(outer, () => order.push("ledger"));

    nav.selectTask?.(214);
    nav.selectDecision?.(7);
    expect(order).toEqual(["ledger", "task", "ledger", "decision"]);
    expect(outer.selectTask).toHaveBeenCalledWith(214);
    expect(outer.selectDecision).toHaveBeenCalledWith(7);
  });

  it("passes the moves that go to the ledger by themselves through as they are", () => {
    const openRuns = vi.fn();
    const openAutomation = vi.fn();
    const nav = ledgerNavUnder({ openRuns, openAutomation }, vi.fn());
    expect(nav.openRuns).toBe(openRuns);
    expect(nav.openAutomation).toBe(openAutomation);
  });

  it("selects through the outer navigation alone where there is no ledger beside it", () => {
    const selectTask = vi.fn();
    ledgerNavUnder({ selectTask }).selectTask?.(214);
    expect(selectTask).toHaveBeenCalledWith(214);
  });
});
