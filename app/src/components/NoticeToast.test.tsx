// @vitest-environment jsdom
//
// What this holds down is `AMB-T-4668`: the toast has to be a thing a window mounts on its own, so
// that the terminal window — which has no `StoreProvider` — draws what is pushed at it. Mounting it
// with nothing else around is the test of exactly that.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { NoticeToast } from "./NoticeToast";
import { pushNotice } from "../core/notice";

// React 18's act() requires this environment flag to be set.
(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  vi.useFakeTimers();
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
  vi.useRealTimers();
});

function toast(): HTMLElement | null {
  return container.querySelector<HTMLElement>(".toast");
}

describe("NoticeToast", () => {
  it("draws nothing until something is pushed", () => {
    act(() => root.render(createElement(NoticeToast)));
    expect(toast()).toBeNull();
  });

  it("says what was pushed, standing on its own with no provider around it", () => {
    act(() => root.render(createElement(NoticeToast)));
    act(() => pushNotice("a folder of that name is already there"));
    expect(toast()?.textContent).toContain("a folder of that name is already there");
    expect(toast()?.getAttribute("role")).toBe("alert");
  });

  it("shows the newest message, not the one before it", () => {
    act(() => root.render(createElement(NoticeToast)));
    act(() => pushNotice("first"));
    act(() => pushNotice("second"));
    expect(toast()?.textContent).toContain("second");
    expect(toast()?.textContent).not.toContain("first");
  });

  it("goes on its own after four seconds", () => {
    act(() => root.render(createElement(NoticeToast)));
    act(() => pushNotice("passing"));
    act(() => { vi.advanceTimersByTime(4000); });
    expect(toast()).toBeNull();
  });

  it("goes when it is clicked, without waiting", () => {
    act(() => root.render(createElement(NoticeToast)));
    act(() => pushNotice("in the way"));
    act(() => { toast()?.click(); });
    expect(toast()).toBeNull();
  });

  it("stops listening once the window it was in has gone", () => {
    act(() => root.render(createElement(NoticeToast)));
    act(() => root.unmount());
    expect(() => pushNotice("nobody home")).not.toThrow();
    expect(toast()).toBeNull();
    // The afterEach unmount is harmless on a root already gone.
  });
});
