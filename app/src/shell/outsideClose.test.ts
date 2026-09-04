// @vitest-environment jsdom
// The heart of it: the TopBar (back/forward/refresh) must not close the pane. If it did, closeRight
// would push "nothing selected" onto the history, and the goBack of the very same click would only
// undo what was just pushed, leaving no way back past the detail pane.
import { describe, it, expect, beforeEach } from "vitest";
import { isBlankSpaceClose } from "./outsideClose";

let rightpane: HTMLElement;

beforeEach(() => {
  document.body.innerHTML = `
    <div class="topbar">
      <span class="topbar__nav">
        <button class="topbar__navbtn" id="back">‹</button>
        <button class="topbar__navbtn" id="forward">›</button>
      </span>
      <button class="topbar__refresh" id="refresh">↻</button>
    </div>
    <div class="shell__header">
      <div class="board__toolbar">
        <button class="filtertoggle" id="filters">Filters</button>
        <div class="topbar__spacer" id="toolbar-gap"></div>
      </div>
    </div>
    <div class="main">
      <div class="board__toolbar" id="inline-toolbar">
        <input class="board__search" id="screen-search" />
      </div>
      <ul>
        <li data-pane-select id="row"><span id="row-label">task</span></li>
      </ul>
      <div id="blank">blank body space</div>
    </div>
    <div class="modal__overlay"><button id="modal-btn">ok</button></div>
    <div id="pane"><input id="pane-input" /></div>
  `;
  rightpane = document.getElementById("pane") as HTMLElement;
});

const at = (id: string) => document.getElementById(id);

describe("isBlankSpaceClose", () => {
  it("closes on a blank body-space click", () => {
    expect(isBlankSpaceClose(at("blank"), rightpane)).toBe(true);
  });

  it("does NOT close on the TopBar back/forward/refresh buttons", () => {
    expect(isBlankSpaceClose(at("back"), rightpane)).toBe(false);
    expect(isBlankSpaceClose(at("forward"), rightpane)).toBe(false);
    expect(isBlankSpaceClose(at("refresh"), rightpane)).toBe(false);
  });

  it("does NOT close on the TopBar chrome itself (bubbled from a child)", () => {
    expect(isBlankSpaceClose(document.querySelector(".topbar"), rightpane)).toBe(false);
  });

  // The board's controls stand in the shell's header slot, portalled there rather than drawn in the list,
  // which is the one place a press is chrome and reads as blank space by its position alone.
  it("does NOT close on the board toolbar's own controls", () => {
    expect(isBlankSpaceClose(at("filters"), rightpane)).toBe(false);
    expect(isBlankSpaceClose(at("toolbar-gap"), rightpane)).toBe(false);
    expect(isBlankSpaceClose(document.querySelector(".shell__header .board__toolbar"), rightpane)).toBe(false);
  });

  // The same toolbar drawn inline by the screens that do not portal it (search, activity), which show the
  // pane just as the board does.
  it("does NOT close on a toolbar a screen draws inline", () => {
    expect(isBlankSpaceClose(at("screen-search"), rightpane)).toBe(false);
    expect(isBlankSpaceClose(at("inline-toolbar"), rightpane)).toBe(false);
  });

  it("does NOT close on a list row/card — switching is left to onClick", () => {
    expect(isBlankSpaceClose(at("row"), rightpane)).toBe(false);
    expect(isBlankSpaceClose(at("row-label"), rightpane)).toBe(false); // closest catches a descendant of the row too
  });

  it("does NOT close inside the right pane", () => {
    expect(isBlankSpaceClose(at("pane-input"), rightpane)).toBe(false);
  });

  it("does NOT close inside a modal overlay", () => {
    expect(isBlankSpaceClose(at("modal-btn"), rightpane)).toBe(false);
  });

  it("closes when the target is null or a non-Element node (falls through)", () => {
    expect(isBlankSpaceClose(null, rightpane)).toBe(true);
    expect(isBlankSpaceClose(document.createTextNode("x"), rightpane)).toBe(true);
  });
});
