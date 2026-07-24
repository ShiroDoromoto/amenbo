// @vitest-environment jsdom
//
// The anchor half of Markdown, which only exists in a DOM: the ids rehype-slug puts on the headings,
// and the click that scrolls to one. The rest of the renderer is pinned by SSR in `Markdown.test.ts`.
import { act, createElement, Fragment, type ReactNode } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { findAnchorTarget, Markdown } from "./Markdown";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
let scrolled: Element[];

/** One rendered body, wrapped the way every render site wraps this component. */
const body = (md: string): ReactNode =>
  createElement("div", { className: "markdown" }, createElement(Markdown, { children: md }));

const render = (...bodies: string[]) =>
  act(() => root.render(createElement(Fragment, null, ...bodies.map((md, i) => createElement(Fragment, { key: i }, body(md))))));

const click = (a: Element | null) => act(() => (a as HTMLAnchorElement).click());

beforeEach(() => {
  scrolled = [];
  // jsdom lays nothing out, so it implements no scrolling; the stub is also what records the target.
  Element.prototype.scrollIntoView = function scrollIntoView(this: Element) {
    scrolled.push(this);
  };
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("heading ids", () => {
  it("gives every heading an id, whatever language it is written in", () => {
    render("# Layout\n\n## Two Words\n\n### やること");
    expect(container.querySelector("h1")?.id).toBe("layout");
    expect(container.querySelector("h2")?.id).toBe("two-words");
    expect(container.querySelector("h3")?.id).toBe("やること");
  });

  // remarkRefs turns the reference into a link node before the tree is slugged, and the id is read off
  // the heading's text either way — so the words the reader sees are the words in the id.
  it("slugs a heading that carries a reference from the text it reads", () => {
    render("## Decided in AMB-D-347");
    expect(container.querySelector("h2")?.id).toBe("decided-in-amb-d-347");
    expect(container.querySelector("h2 a.reflink")).not.toBeNull();
  });
});

describe("following an anchor", () => {
  it("scrolls to the heading the link names, instead of navigating the window", () => {
    render("[go](#layout)\n\n## Layout");
    click(container.querySelector('a[href="#layout"]'));
    expect(scrolled).toEqual([container.querySelector("h2")]);
  });

  it("works for a heading whose id no `#…` selector would take unescaped", () => {
    render("[行く](#やること)\n\n## やること");
    click(container.querySelector("a"));
    expect(scrolled).toEqual([container.querySelector("h2")]);
  });

  // A screen shows several bodies at once — a task's notes and each of its comments — and two of them
  // may well head a section the same way. An anchor must stay inside the body it was written in.
  it("stays within its own body when two bodies head a section the same way", () => {
    render("## やること\n\nfirst", "[行く](#やること)\n\n## やること");
    const second = container.querySelectorAll(".markdown")[1];
    click(second.querySelector("a"));
    expect(scrolled).toEqual([second.querySelector("h2")]);
  });

  it("does nothing when the anchor names no heading here", () => {
    render("[nowhere](#missing)\n\n## Layout");
    click(container.querySelector("a"));
    expect(scrolled).toEqual([]);
  });
});

describe("findAnchorTarget", () => {
  it("decodes a percent-encoded href, since the id on the heading is not encoded", () => {
    render("## やること");
    expect(findAnchorTarget(container, "#%E3%82%84%E3%82%8B%E3%81%93%E3%81%A8")).toBe(container.querySelector("h2"));
  });

  // A `%` a reader typed is not the start of an escape, and decoding it throws — which must come out
  // as "no such heading", not as a broken click.
  it("survives an href that is not valid percent-encoding", () => {
    render("## Layout");
    expect(findAnchorTarget(container, "#50%-off")).toBeNull();
  });

  it("is null for a bare `#`", () => {
    render("## Layout");
    expect(findAnchorTarget(container, "#")).toBeNull();
  });
});
