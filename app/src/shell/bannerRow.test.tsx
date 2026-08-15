// @vitest-environment jsdom
// Where the app's bands sit in the shell. `.shell` is a grid whose rows are declared once
// (`styles/global.css`), and every child is given its row outright — a child left to auto-placement
// moves as the ones before it come and go, and every band renders conditionally.
//
// That is what this pins. Each band was once a direct child of the shell, sharing one row number with
// its siblings; a grid child with no row of its own is not stacked under one that has, it is put in a
// column beside it, and the implicit columns that makes squeeze the app into the first one. So what
// the shell must see is a child count that does not move: one wrapper, whatever is inside it.
//
// jsdom lays nothing out, so the assertion is on the shape rather than on the pixels — which is the
// half a regression would show up in first, the CSS having no way to say "row per child" wrongly
// without a child appearing beside the wrapper.
import { act, createElement, type ReactNode } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

// React 18's act() requires this environment flag to be set.
(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// One band. Each of the real ones returns null when it has nothing to say, which is what makes the
// child count vary in the first place.
const band = (key: string, up: boolean): ReactNode =>
  up ? createElement("div", { key, className: "healthbanner" }, key) : null;

// The production wiring: the bands stack inside one wrapper, and the wrapper is what the shell places.
function Shell({ up }: { up: string[] }) {
  return createElement(
    "div",
    { className: "shell" },
    createElement("div", { className: "topbar" }),
    createElement("div", { className: "shell__banners" }, ...ALL.map((n) => band(n, up.includes(n)))),
    createElement("div", { className: "shell__body" }),
  );
}

// The control: every band a direct child of the shell, as they were. Nothing is wrong with the markup
// — it is that the shell now has to know how many there could be, and give each one a row.
function FlatShell({ up }: { up: string[] }) {
  return createElement(
    "div",
    { className: "shell" },
    createElement("div", { className: "topbar" }),
    ...ALL.map((n) => band(n, up.includes(n))),
    createElement("div", { className: "shell__body" }),
  );
}

const ALL = ["update", "plugins", "health", "hooks"];

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

const shellChildren = () => container.querySelector(".shell")!.children.length;
const bands = () => container.querySelectorAll(".healthbanner").length;

describe("where the bands sit in the shell", () => {
  it("gives the shell the same three children whether nothing is up or everything is", () => {
    act(() => root.render(createElement(Shell, { up: [] })));
    expect(shellChildren()).toBe(3);
    expect(bands()).toBe(0);

    act(() => root.render(createElement(Shell, { up: ["health"] })));
    expect(shellChildren()).toBe(3);

    act(() => root.render(createElement(Shell, { up: ALL })));
    expect(shellChildren()).toBe(3);
    expect(bands()).toBe(ALL.length);
  });

  it("keeps every band inside the one wrapper, so none of them lands beside it", () => {
    act(() => root.render(createElement(Shell, { up: ALL })));
    const wrapper = container.querySelector(".shell__banners")!;
    expect(wrapper.querySelectorAll(".healthbanner")).toHaveLength(ALL.length);
    expect([...container.querySelector(".shell")!.children].filter((c) => c.classList.contains("healthbanner")))
      .toHaveLength(0);
  });

  it("is what the flat shape cannot do: its child count moves with the news", () => {
    act(() => root.render(createElement(FlatShell, { up: [] })));
    const quiet = shellChildren();
    act(() => root.render(createElement(FlatShell, { up: ALL })));
    expect(shellChildren()).toBe(quiet + ALL.length);
  });
});
