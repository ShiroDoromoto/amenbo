// @vitest-environment jsdom
// A build screen's panel, drawn into the shell's right-pane column (`AMB-T-5418`).
//
// What these guard: **the panel lands in the column, not beside the picture** — nested in `.main` it
// scrolled inside a column that scrolled too; **the column stands for as long as a panel is mounted**,
// counted, so one panel giving way to another in the same commit keeps it; and **outside the shell the
// panel is drawn where it stands**, which is what the screens' own tests render.
import { act, createElement, useCallback, useMemo, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { Panel } from "../screens/AutomationActionBuildScreen";
import { PaneSlotProvider } from "./paneSlot";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  host = document.createElement("div");
  document.body.appendChild(host);
  root = createRoot(host);
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
});

/** The shell's side of the seam, reduced to it: the column stands while anything claims it. */
function Shell({ which }: { which: "a" | "b" | null }) {
  const [claims, setClaims] = useState(0);
  const [slot, setSlot] = useState<HTMLElement | null>(null);
  const claim = useCallback(() => {
    setClaims((n) => n + 1);
    return () => setClaims((n) => n - 1);
  }, []);
  const value = useMemo(() => ({ slot, claim }), [slot, claim]);
  const main = createElement("div", { key: "main", className: "main" }, which && createElement(Panel, {
    key: which,
    place: "Step",
    title: which,
    onClose: () => {},
    children: createElement("p", null, `body ${which}`),
  }));
  const column = claims > 0 && createElement("div", { key: "column", className: "rightpane", ref: setSlot });
  return createElement(PaneSlotProvider, { value, children: [main, column] });
}

describe("a build screen's panel", () => {
  it("is drawn in the right-pane column, not in main", () => {
    act(() => root.render(createElement(Shell, { which: "a" })));
    expect(host.querySelector(".main .actpanel")).toBeNull();
    expect(host.querySelector(".rightpane .actpanel__title")?.textContent).toBe("a");
  });

  it("keeps the column when one panel gives way to another", () => {
    act(() => root.render(createElement(Shell, { which: "a" })));
    act(() => root.render(createElement(Shell, { which: "b" })));
    expect(host.querySelector(".rightpane .actpanel__title")?.textContent).toBe("b");
  });

  it("gives the column back when it closes", () => {
    act(() => root.render(createElement(Shell, { which: "a" })));
    act(() => root.render(createElement(Shell, { which: null })));
    expect(host.querySelector(".rightpane")).toBeNull();
  });

  it("is drawn in place where there is no shell", () => {
    act(() => root.render(createElement(Panel, { place: "Step", title: "alone", onClose: () => {}, children: null })));
    expect(host.querySelector(".actpanel__title")?.textContent).toBe("alone");
  });
});
