// @vitest-environment jsdom
// Every box on the automation screens takes what was typed as it was typed. Without `asTyped`, macOS
// capitalizes a one-word name the moment focus leaves the box — `theme` is shown as `Theme` — and a name
// declared that way no longer reads as the output of the step before it, which is the whole of what joins
// the two. Nothing in the type system says a box was left bare, hence the sweep below rather than a
// convention, with the declaring row — where the name joins two steps — drawn on its own as well.
//
// What the sweep can see: an `<input>` or `<textarea>` spelled in the JSX of a file on the automation
// screens. A box whose `type` takes no characters (a checkbox, a number, a date…) is left out, as `asTyped`
// itself says. A box made through a component of its own is the component's file to answer for.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { DeclareRow } from "./automationPanel";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const screens = import.meta.glob("./*.tsx", { query: "?raw", import: "default", eager: true });
const sources = Object.fromEntries(
  (Object.entries(screens) as [string, string][]).filter(
    ([file]) => /^\.\/automation/i.test(file) && !file.endsWith(".test.tsx"),
  ),
);

/** The `type`s that take no characters, so take none of `asTyped`. */
const UNTYPED = new Set(["checkbox", "radio", "number", "date", "time", "color", "file", "range", "hidden"]);

/** Each `<input …>` and `<textarea …>` opening tag in a source, braces and all, with the line it starts on. */
function boxesIn(source: string): { line: number; tag: string }[] {
  const found: { line: number; tag: string }[] = [];
  const opening = /<(input|textarea)\b/g;
  for (let open = opening.exec(source); open !== null; open = opening.exec(source)) {
    let at = open.index + open[0].length;
    let depth = 0;
    for (; at < source.length; at++) {
      const c = source[at];
      if (c === "{") depth++;
      else if (c === "}") depth--;
      else if (depth === 0 && c === ">") break;
    }
    found.push({ line: source.slice(0, open.index).split("\n").length, tag: source.slice(open.index, at + 1) });
  }
  return found;
}

describe("the boxes on the automation screens", () => {
  it("are found by the sweep", () => {
    expect(Object.keys(sources).length).toBeGreaterThan(0);
    expect(Object.values(sources).flatMap(boxesIn).length).toBeGreaterThan(0);
  });

  it("each spread asTyped unless they take no characters", () => {
    const bare = Object.entries(sources).flatMap(([file, source]) =>
      boxesIn(source)
        .filter(({ tag }) => !UNTYPED.has(/\btype="(\w+)"/.exec(tag)?.[1] ?? ""))
        .filter(({ tag }) => !tag.includes("{...asTyped}"))
        .map(({ line }) => `${file}:${line}`),
    );
    expect(bare).toEqual([]);
  });
});

describe("the row that declares one more", () => {
  let root: Root;
  let host: HTMLDivElement;

  beforeEach(() => {
    host = document.createElement("div");
    document.body.appendChild(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  it("leaves the name as typed — no capitals, no corrections, no spelling marks", () => {
    act(() => {
      root.render(createElement(DeclareRow, { what: "name", kinds: null, onAdd: () => Promise.resolve(true) }));
    });
    const box = host.querySelector("input")!;
    expect(box.getAttribute("autocapitalize")).toBe("off");
    expect(box.getAttribute("autocorrect")).toBe("off");
    expect(box.getAttribute("spellcheck")).toBe("false");
  });
});
