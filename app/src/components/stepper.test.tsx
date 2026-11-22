// @vitest-environment jsdom
// The stepper on its own: "−" and "+" step the number, and the number itself takes a typed one. What
// is asked here is what the screens that use it do not see on their own: that a typed number reaches
// the caller only within the bounds, that anything else is refused in the box rather than handed on,
// that a number the caller hands back does not overwrite what the reader is still typing, and that the
// caller is told while a refused number stands in the box.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { Stepper, typedNumber } from "./Stepper";
import { tf } from "../core/i18n";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
let stepped: number[];
let told: boolean[];

function render(value: number, min = 1, max = 10, typable = true): void {
  act(() => {
    root.render(
      createElement(Stepper, {
        value,
        min,
        max,
        label: "How many",
        valueTemplate: "{n} 名",
        lessLabel: "Fewer",
        moreLabel: "More",
        onStep: (n) => stepped.push(n),
        onRefused: (refused) => told.push(refused),
        typable,
      }),
    );
  });
}

const input = () => container.querySelector<HTMLInputElement>(".stepper input")!;
const button = (label: string) => container.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`)!;
const range = () => container.querySelector(".stepfield__range");
const setValue = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
function type(value: string): void {
  act(() => {
    setValue.call(input(), value);
    input().dispatchEvent(new Event("input", { bubbles: true }));
  });
}
const focus = () => act(() => input().focus());
const blur = () => act(() => input().blur());

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  stepped = [];
  told = [];
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("Stepper", () => {
  it("draws the number in a box to type in, with its unit beside it, as one field", () => {
    render(3);
    const group = container.querySelector(".stepper")!;
    expect(group.getAttribute("role")).toBe("group");
    expect(group.getAttribute("aria-label")).toBe("How many");
    expect(group.hasAttribute("data-saved-field")).toBe(true);
    expect(input().value).toBe("3");
    expect(input().getAttribute("aria-label")).toBe("How many");
    expect(container.querySelector(".stepper__value")!.textContent).toBe(" 名");
    expect(input().getAttribute("aria-invalid")).toBeNull();
    expect(range()).toBeNull();
  });

  it("steps one at a time, and disables the press at each bound", () => {
    render(1);
    expect(button("Fewer").disabled).toBe(true);
    act(() => button("More").click());
    expect(stepped).toEqual([2]);
    render(10);
    expect(button("More").disabled).toBe(true);
    act(() => button("Fewer").click());
    expect(stepped).toEqual([2, 9]);
  });

  it("hands a typed number within the bounds to the caller", () => {
    render(3);
    focus();
    type("7");
    type("１０");
    expect(stepped).toEqual([7, 10]);
  });

  it("refuses a number out of the bounds, a word and an empty box, and hands none of them on", () => {
    render(3);
    focus();
    for (const text of ["0", "11", "abc", "2.5", ""]) {
      type(text);
      expect(input().value, text).toBe(text);
      expect(input().getAttribute("aria-invalid"), text).toBe("true");
      expect(range()?.textContent, text).toBe(tf("auto.side.stepperRange", { min: 1, max: 10 }));
      expect(input().getAttribute("aria-describedby"), text).toBe(range()!.id);
    }
    expect(stepped).toEqual([]);
    type("4");
    expect(range()).toBeNull();
    expect(stepped).toEqual([4]);
  });

  it("keeps what is typed while the caret is in the box, and follows the caller once it leaves", () => {
    render(3);
    focus();
    type("1");
    render(1);
    type("10");
    render(10);
    expect(input().value).toBe("10");
    blur();
    render(4);
    expect(input().value).toBe("4");
  });

  it("does not overwrite the typed number with the caller's echo of an earlier one", () => {
    render(3);
    focus();
    type("1");
    type("10");
    render(1);
    expect(input().value).toBe("10");
  });

  it("tells the caller when a refused number comes into the box and when it leaves", () => {
    render(3);
    focus();
    type("11");
    type("12");
    expect(told).toEqual([true]);
    type("4");
    expect(told).toEqual([true, false]);
    type("");
    act(() => root.unmount());
    root = createRoot(container);
    expect(told).toEqual([true, false, true, false]);
  });

  it("drops a refused number for the caller's once a press steps", () => {
    render(3);
    focus();
    type("50");
    act(() => button("More").click());
    expect(stepped).toEqual([4]);
    expect(input().value).toBe("3");
    render(4);
    expect(input().value).toBe("4");
  });

  it("draws the number with no box where it is not to be typed, and still steps", () => {
    render(3, 1, 10, false);
    expect(container.querySelector(".stepper input")).toBeNull();
    expect(container.querySelector(".stepper output")!.textContent).toBe("3 名");
    act(() => button("More").click());
    expect(stepped).toEqual([4]);
    expect(range()).toBeNull();
  });
});

describe("typedNumber", () => {
  it("reads whole numbers, full-width digits too, and nothing else", () => {
    expect(typedNumber(" 12 ")).toBe(12);
    expect(typedNumber("３")).toBe(3);
    expect(typedNumber("")).toBeNull();
    expect(typedNumber("-1")).toBeNull();
    expect(typedNumber("1e2")).toBeNull();
  });
});
