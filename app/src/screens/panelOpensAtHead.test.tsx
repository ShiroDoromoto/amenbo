// @vitest-environment jsdom
// A panel opens at its head (`AMB-T-5676`).
//
// The panels of the build screens are drawn into the shell's right-pane column (`../shell/paneSlot`),
// and that column is one scroll that outlives the panel in it. A panel opened after another — the
// library after a spot scrolled to its foot, say — would otherwise stand wherever the last one was
// scrolled to, with the line it is placing on and its search above the fold.
import { act, createElement, type ReactNode } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { PaneSlotProvider } from "../shell/paneSlot";
import { Panel } from "./AutomationActionBuildScreen";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let column: HTMLDivElement;
let root: Root;

beforeEach(() => {
  container = document.createElement("div");
  column = document.createElement("div");
  document.body.append(container, column);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
  column.remove();
});

/** One panel in the column, told apart by its key as the build screens tell theirs apart. */
async function show(which: string): Promise<void> {
  const panel: ReactNode = createElement(Panel, { key: which, place: which, title: which, onClose: () => {}, children: which });
  await act(async () => {
    root.render(createElement(PaneSlotProvider, { value: { slot: column, claim: () => () => {} }, children: panel }));
  });
}

describe("a panel in the right-pane column", () => {
  it("opens at its head, however far the one before it was scrolled", async () => {
    await show("spot");
    column.scrollTop = 400;
    await show("library");
    expect(column.scrollTop, "the new panel opened where the last one was scrolled to").toBe(0);
  });

  it("stays where the reader scrolled it while it is the same panel", async () => {
    await show("spot");
    column.scrollTop = 400;
    await show("spot");
    expect(column.scrollTop, "a panel redrawn for the same thing jumped back to its head").toBe(400);
  });
});
