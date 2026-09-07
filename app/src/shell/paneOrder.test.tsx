// @vitest-environment jsdom
// The reorder modal: that it draws the pages the list makes, that a card carried across them says
// where it came from, and above all that nothing leaves here except by the button.
//
// jsdom has no layout — every point in one is over nothing at all and every rectangle is zero — so
// the cards answer for their own rectangles and the document for what is under a point. That is the
// same trade `./rowDrag.test` makes, and it is what lets a case put a card exactly where it needs it.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { EMPTY_LAYOUT, openedFrame, panesOf, setCount, type Count, type Frame, type Layout } from "../talk/layout";
import { PaneOrder } from "./PaneOrder";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** A face with `n` panes opened in one project, drawn at `count` a page. */
function faceOf(n: number, count: Count = 2): Layout {
  let layout: Layout = setCount({ ...EMPTY_LAYOUT, project: 1 }, count);
  for (let i = 0; i < n; i++) layout = openedFrame(layout, 1, `/work/${i + 1}`).layout;
  return layout;
}

let host: HTMLDivElement;
let root: Root;
let taken: Frame[][];
let closed: number;

beforeEach(() => {
  host = document.createElement("div");
  document.body.appendChild(host);
  root = createRoot(host);
  taken = [];
  closed = 0;
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
});

function draw(layout: Layout, names: ReadonlyMap<string, string> = new Map()) {
  act(() => {
    root.render(createElement(PaneOrder, {
      layout,
      panes: panesOf(layout, layout.project),
      names,
      onClose: () => { closed += 1; },
      onOrder: (order: readonly Frame[]) => { taken.push([...order]); },
    }));
  });
}

const cards = () => [...host.querySelectorAll<HTMLElement>(".paneorder__card")];
const ids = () => cards().map((one) => one.dataset.paneCard);
const cardOf = (id: string) => host.querySelector<HTMLElement>(`[data-pane-card="${id}"]`)!;

describe("the pages the modal draws", () => {
  it("cuts the list at the count, the way the face does", () => {
    draw(faceOf(3, 2));
    expect(host.querySelectorAll(".paneorder__page")).toHaveLength(2);
    expect(ids()).toEqual(["1", "2", "3"]);
  });

  it("draws no empty box where a page has room — the cards are the only places", () => {
    draw(faceOf(3, 2));
    const pages = [...host.querySelectorAll(".paneorder__page")];
    expect(pages[1]!.querySelectorAll(".paneorder__card")).toHaveLength(1);
  });

  it("lays a page out at the grid the face is drawn at", () => {
    draw(faceOf(2, 2));
    expect(host.querySelector(".paneorder__grid")!.className)
      .toContain("termface__page-grid--2");
  });

  it("calls a pane what it is called on the face — its name, else the folder it works in", () => {
    draw(faceOf(2, 2), new Map([["1", "agent"]]));
    expect([...host.querySelectorAll(".paneorder__name")].map((one) => one.textContent))
      .toEqual(["agent", "2"]);
  });

  it("says nothing about where a card came from until one has moved", () => {
    draw(faceOf(4, 2));
    expect(host.querySelectorAll(".paneorder__from")).toHaveLength(0);
  });
});

describe("what leaves the modal", () => {
  it("is the order the reader pressed for, and only then", () => {
    draw(faceOf(3, 2));
    act(() => { host.querySelector<HTMLButtonElement>(".btn--primary")!.click(); });
    expect(taken.map((order) => order.map((one) => one.id))).toEqual([["1", "2", "3"]]);
  });

  it("is nothing at all from the button beside it", () => {
    draw(faceOf(3, 2));
    const back = [...host.querySelectorAll<HTMLButtonElement>(".buttonrow .btn")]
      .find((one) => !one.classList.contains("btn--primary"))!;
    act(() => { back.click(); });
    expect(taken).toEqual([]);
    expect(closed).toBe(1);
  });

  it("is nothing at all from Escape", () => {
    draw(faceOf(3, 2));
    act(() => { window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" })); });
    expect(taken).toEqual([]);
    expect(closed).toBe(1);
  });

  it("is nothing at all from the backdrop", () => {
    draw(faceOf(3, 2));
    act(() => { host.querySelector<HTMLElement>(".modal__overlay")!.click(); });
    expect(taken).toEqual([]);
    expect(closed).toBe(1);
  });
});

describe("carrying a card", () => {
  /** Two columns of one row: card `1` on the left of the screen, everything else on the right. */
  function laidOut() {
    for (const card of cards()) {
      const left = card.dataset.paneCard === "1" ? 0 : 100;
      card.getBoundingClientRect = () => ({
        top: 0, height: 50, bottom: 50, left, right: left + 100, width: 100,
        x: left, y: 0, toJSON: () => ({}),
      }) as DOMRect;
    }
  }

  /** Press on a card, move to a point, and let go — with the document answering for what is there. */
  async function carry(from: string, to: HTMLElement | null, at: { x: number; y: number }) {
    document.elementFromPoint = () => to;
    await act(async () => {
      cardOf(from).dispatchEvent(new MouseEvent("pointerdown", {
        bubbles: true, button: 0, clientX: 500, clientY: 25,
      }));
      document.dispatchEvent(new MouseEvent("pointermove", { clientX: at.x, clientY: at.y }));
      // One hit test a frame, so the order moves on the frame the move asked for.
      await new Promise((done) => setTimeout(done, 20));
      document.dispatchEvent(new MouseEvent("pointerup", { clientX: at.x, clientY: at.y }));
    });
  }

  it("puts it where the half of the card it was let go over says", async () => {
    draw(faceOf(3, 2));
    laidOut();
    // The near half of the first card, which is the left half at a page drawn across.
    await carry("3", cardOf("1"), { x: 10, y: 25 });
    expect(ids()).toEqual(["3", "1", "2"]);
  });

  it("says which page a card has come off, on every card that crossed one", async () => {
    // Three panes at two a page, and the last one carried to the front: it has come off page two,
    // and the one it pushed over the edge has come off page one. Both crossed a page, so both say
    // so — the card that was pushed is as far from where the reader left it as the carried one.
    draw(faceOf(3, 2));
    laidOut();
    await carry("3", cardOf("1"), { x: 10, y: 25 });
    expect(cardOf("3").querySelector(".paneorder__from")).not.toBeNull();
    expect(cardOf("2").querySelector(".paneorder__from")).not.toBeNull();
    // The one that stayed on its page says nothing, however far along it it has moved.
    expect(cardOf("1").querySelector(".paneorder__from")).toBeNull();
  });

  it("writes nothing on its own — the order still waits for the button", async () => {
    draw(faceOf(3, 2));
    laidOut();
    await carry("3", cardOf("1"), { x: 10, y: 25 });
    expect(taken).toEqual([]);
    act(() => { host.querySelector<HTMLButtonElement>(".btn--primary")!.click(); });
    expect(taken.map((order) => order.map((one) => one.id))).toEqual([["3", "1", "2"]]);
  });

  it("moves nothing where the press never became a drag", async () => {
    draw(faceOf(3, 2));
    laidOut();
    // Two pixels from where it went down, which is inside the slop every gesture here shares.
    await carry("3", cardOf("1"), { x: 502, y: 25 });
    expect(ids()).toEqual(["1", "2", "3"]);
  });
});
