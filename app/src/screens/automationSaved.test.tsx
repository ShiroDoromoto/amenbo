// @vitest-environment jsdom
// What a build screen says about saving (`AMB-D-1005`): the head says the definition's saved state,
// or why a write was refused, and "Written" stands beside the field the write came from for two
// seconds. The writes are `told` directly with promises the test settles, so what runs for real is
// the line they report on and the hook that draws it.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { told } from "../core/automationSave";
import { t } from "../core/i18n";
import { fieldOf, MARK_MS, markBeside, useSaved, type SavedState } from "./AutomationSaved";

/** A write the test settles by hand. */
function pending() {
  let settle: { ok: () => void; no: (why: unknown) => void } = { ok: () => {}, no: () => {} };
  const promise = new Promise<void>((ok, no) => (settle = { ok: () => ok(), no }));
  return { promise, settle: () => settle };
}

let write = pending();
// The saved state the screen hands the hook.
let state: SavedState | null;

function Screen() {
  const saved = useSaved(state);
  return createElement(
    "div",
    { ...saved.capture },
    createElement("div", { className: "head" }, saved.head),
    createElement("input", {
      "aria-label": "name",
      onBlur: () => void told(() => write.promise).catch(() => undefined),
    }),
    saved.marks,
  );
}

let root: Root;
let host: HTMLDivElement;

beforeEach(() => {
  vi.useFakeTimers();
  write = pending();
  state = null;
  host = document.createElement("div");
  document.body.appendChild(host);
  root = createRoot(host);
  act(() => root.render(createElement(Screen)));
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
  vi.useRealTimers();
});

const head = () => host.querySelector(".actsaved");
const marks = () => document.body.querySelectorAll(".actsaved__mark");
const field = () => host.querySelector("input") as HTMLInputElement;

/** The caret leaves the field, which writes. */
function leave() {
  act(() => {
    field().focus();
    field().blur();
  });
}

describe("a head handed the definition's saved state", () => {
  function draw(next: SavedState | null) {
    state = next;
    act(() => root.render(createElement(Screen)));
  }

  it("says nothing is saved yet, then that changes are unsaved, then the version saved last", () => {
    draw({ unsaved: true });
    expect(head()?.textContent).toBe(t("auto.saved.never"));
    draw({ saved: { version: 2, savedAt: "2026-10-01T09:00:00Z" }, unsaved: true });
    expect(head()?.textContent).toBe(t("auto.saved.unsaved"));
    draw({ saved: { version: 2, savedAt: "2026-10-01T09:00:00Z" }, unsaved: false });
    expect(head()?.textContent).toContain("2");
    expect(head()?.textContent).not.toBe(t("auto.saved.unsaved"));
  });

  it("says nothing until the state is read", () => {
    draw(null);
    expect(head()?.textContent).toBe("");
  });

  it("stands Written beside the field a write came from for two seconds", async () => {
    draw({ unsaved: false });
    leave();
    await act(async () => {
      write.settle().ok();
      await write.promise;
    });
    expect(head()?.textContent).toBe(t("auto.saved.never"));
    expect(marks()).toHaveLength(1);
    expect(marks()[0].textContent).toBe(t("auto.saved.written"));
    act(() => vi.advanceTimersByTime(MARK_MS));
    expect(marks()).toHaveLength(0);
  });

  it("marks only the head for a write no event of the reader's started", async () => {
    draw({ unsaved: false });
    act(() => vi.runAllTimers());
    await act(async () => {
      const own = told(() => Promise.resolve());
      await own;
    });
    expect(marks()).toHaveLength(0);
  });

  it("turns the head red when a write is refused", async () => {
    draw({ unsaved: false });
    leave();
    await act(async () => {
      write.settle().no(new Error("held by a run"));
      await write.promise.catch(() => undefined);
    });
    expect(head()?.classList.contains("actsaved--failed")).toBe(true);
    expect(head()?.textContent).toContain("held by a run");
    expect(marks()).toHaveLength(0);
  });
});

describe("where Written stands", () => {
  /** An element drawn from `left` to `right`, 20px high at the top of the window. */
  function drawn(el: Element, left: number, right: number): Element {
    el.getBoundingClientRect = () => ({ left, right, top: 0, bottom: 20, width: right - left, height: 20 }) as DOMRect;
    return el;
  }

  it("takes a control of a stepper as the whole stepper", () => {
    const group = document.createElement("span");
    group.setAttribute("data-saved-field", "");
    const plus = document.createElement("button");
    group.appendChild(plus);
    const input = document.createElement("input");
    expect(fieldOf(plus)).toBe(group);
    expect(fieldOf(input)).toBe(input);
  });

  it("stands right of a field with room, and inside one at the window's end", () => {
    const end = window.innerWidth;
    expect(markBeside(drawn(document.createElement("input"), 10, 100), 1)).toMatchObject({ left: 106, inside: false });
    expect(markBeside(drawn(document.createElement("input"), end - 200, end - 10), 1)).toMatchObject({
      left: end - 16,
      inside: true,
    });
  });

  it("stands left of a stepper at the window's end, off its number", () => {
    const end = window.innerWidth;
    const group = drawn(document.createElement("span"), end - 110, end - 10);
    group.setAttribute("data-saved-field", "");
    expect(markBeside(group, 1)).toMatchObject({ left: end - 116, inside: true });
  });
});
