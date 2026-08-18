// @vitest-environment jsdom
// The field that writes one of a task's two days. What is asked here is the three things a native date
// input does not settle on its own: that no day is drawn as no day (an empty picker fills itself in with
// a date of its own, which reads as one somebody set), that the value crossing the boundary is
// `YYYY-MM-DD` and nothing else, and that the day can be taken back off. The clear button exists because
// not every platform draws one on the picker itself, so its absence would be invisible on the machine
// that does.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { DateField } from "./atoms";
import { t } from "../core/i18n";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
let written: (string | null)[];

function render(value: string | null): void {
  act(() => {
    root.render(createElement(DateField, { label: "期日", value, onChange: (d) => written.push(d) }));
  });
}

const input = () => container.querySelector("input");
const button = (label: string) =>
  Array.from(container.querySelectorAll("button")).find((b) => b.textContent === label);
const clearButton = () => button(t("date.clear"));
const addButton = () => button(t("detail.add"));

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  written = [];
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("DateField", () => {
  it("is a date input carrying the day it was given, named for the field it writes", () => {
    render("2099-12-31");
    expect(input()!.type).toBe("date");
    expect(input()!.value).toBe("2099-12-31");
    expect(input()!.getAttribute("aria-label")).toBe("期日");
  });

  it("draws no day as no day — the picker that would fill itself in is not on screen at all", () => {
    render(null);
    expect(input()).toBeNull();
    expect(container.textContent).toContain(t("detail.none"));
    expect(addButton()).toBeDefined();
    expect(clearButton()).toBeUndefined();
  });

  it("opens the picker on request, and writes nothing until a day is actually named", () => {
    render(null);
    act(() => addButton()!.click());
    expect(input()).not.toBeNull();
    expect(input()!.value).toBe("");
    expect(written).toEqual([]);

    // Closing a picker that never got a day is not a clear: there was nothing there to take away.
    act(() => clearButton()!.click());
    expect(input()).toBeNull();
    expect(written).toEqual([]);
  });

  it("says 'no day' as null, whether it is pressed away or emptied in the picker", () => {
    render("2099-12-31");
    act(() => clearButton()!.click());
    expect(written).toEqual([null]);

    // An emptied picker sends the empty string; what leaves here is the absence itself, so the write
    // below it never has to read a blank as a date.
    written = [];
    const setValue = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
    act(() => {
      setValue.call(input()!, "");
      input()!.dispatchEvent(new Event("input", { bubbles: true }));
    });
    expect(written).toEqual([null]);
  });
});
