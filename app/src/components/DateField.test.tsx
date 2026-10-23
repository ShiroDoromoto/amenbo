// @vitest-environment jsdom
// The field that writes a task's start or due. What is asked here is what a native date input does not
// settle on its own: that no day is drawn as no day (an empty picker fills itself in with a date of its
// own, which reads as one somebody set), that the value crossing the boundary is `YYYY-MM-DD`, or
// `YYYY-MM-DD HH:MM` once the optional time is filled, and that the day can be taken back off. The clear
// button exists because not every platform draws one on the picker itself, so its absence would be
// invisible on the machine that does.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { DateField } from "./atoms";
import { t, tf } from "../core/i18n";
import type { Schedule } from "../mock/types";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
let written: (string | null)[];

const day = (d: string): Schedule => ({ kind: "day", day: d });
// An instant at a local time, so the day and time the field shows are the same whatever timezone the test runs in.
const at = (y: number, mo: number, d: number, h: number, mi = 0): Schedule =>
  ({ kind: "at", at: new Date(y, mo - 1, d, h, mi).toISOString() });

function render(value: Schedule | null, timed = true): void {
  act(() => {
    root.render(createElement(DateField, { label: "期日", value, onChange: (d) => written.push(d), timed }));
  });
}

const input = () => container.querySelector<HTMLInputElement>('input[type="date"]');
const timeInput = () => container.querySelector<HTMLInputElement>('input[type="time"]');
const setValue = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
function type(el: HTMLInputElement, value: string): void {
  act(() => {
    setValue.call(el, value);
    el.dispatchEvent(new Event("input", { bubbles: true }));
  });
}
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
    render(day("2099-12-31"));
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
    render(day("2099-12-31"));
    act(() => clearButton()!.click());
    expect(written).toEqual([null]);

    // An emptied picker sends the empty string; what leaves here is the absence itself, so the write
    // below it never has to read a blank as a date.
    written = [];
    type(input()!, "");
    expect(written).toEqual([null]);
  });

  it("puts an empty time beside a day, and a time typed into it rides on that day", () => {
    render(day("2099-12-31"));
    expect(timeInput()!.value).toBe("");
    expect(timeInput()!.getAttribute("aria-label")).toBe(tf("date.timeOf", { field: "期日" }));
    type(timeInput()!, "17:30");
    expect(written).toEqual(["2099-12-31 17:30"]);
  });

  it("shows an instant as its local day and time, and keeps the time when the day moves", () => {
    render(at(2099, 12, 31, 9, 5));
    expect(input()!.value).toBe("2099-12-31");
    expect(timeInput()!.value).toBe("09:05");
    type(input()!, "2099-12-30");
    expect(written).toEqual(["2099-12-30 09:05"]);
  });

  it("goes back to the day alone when the time is emptied, and takes the time with the day", () => {
    render(at(2099, 12, 31, 9, 5));
    type(timeInput()!, "");
    expect(written).toEqual(["2099-12-31"]);

    written = [];
    type(input()!, "");
    expect(written).toEqual([null]);
  });

  it("draws no time field where it is not asked for — a task being created takes a day alone", () => {
    render(day("2099-12-31"), false);
    expect(input()!.value).toBe("2099-12-31");
    expect(timeInput()).toBeNull();
  });

  it("offers no time before there is a day to put it on", () => {
    render(null);
    act(() => addButton()!.click());
    expect(input()).not.toBeNull();
    expect(timeInput()).toBeNull();
  });
});
