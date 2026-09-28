// @vitest-environment jsdom
// What a build screen says about saving (`AMB-D-1005`): the head says when the last write landed, or
// why it was refused, and "Saved" stands beside the field the write came from for two seconds. The
// writes are `told` directly with promises the test settles, so what runs for real is the line they
// report on and the hook that draws it.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { told } from "../core/automationSave";
import { t } from "../core/i18n";
import { MARK_MS, useSaved } from "./AutomationSaved";

/** A write the test settles by hand. */
function pending() {
  let settle: { ok: () => void; no: (why: unknown) => void } = { ok: () => {}, no: () => {} };
  const promise = new Promise<void>((ok, no) => (settle = { ok: () => ok(), no }));
  return { promise, settle: () => settle };
}

let write = pending();

function Screen() {
  const saved = useSaved();
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

describe("what a build screen says about saving (AMB-D-1005)", () => {
  it("says editing saves on the spot before anything is written", () => {
    expect(head()?.textContent).toBe(t("auto.saved.onTheSpot"));
    expect(marks()).toHaveLength(0);
  });

  it("says when the last write landed, and stands Saved beside its field for two seconds", async () => {
    leave();
    await act(async () => {
      write.settle().ok();
      await write.promise;
    });
    expect(head()?.textContent).not.toBe(t("auto.saved.onTheSpot"));
    expect(head()?.classList.contains("actsaved--failed")).toBe(false);
    expect(marks()).toHaveLength(1);
    expect(marks()[0].textContent).toBe(t("auto.saved.mark"));
    act(() => vi.advanceTimersByTime(MARK_MS));
    expect(marks()).toHaveLength(0);
  });

  it("turns the head red with the reason when a write is refused", async () => {
    leave();
    await act(async () => {
      write.settle().no(new Error("held by a run"));
      await write.promise.catch(() => undefined);
    });
    expect(head()?.classList.contains("actsaved--failed")).toBe(true);
    expect(head()?.textContent).toContain("held by a run");
    expect(marks()).toHaveLength(0);
  });

  it("marks only the head for a write no event of the reader's started", async () => {
    act(() => vi.runAllTimers());
    await act(async () => {
      const own = told(() => Promise.resolve());
      await own;
    });
    expect(head()?.textContent).not.toBe(t("auto.saved.onTheSpot"));
    expect(marks()).toHaveLength(0);
  });
});
