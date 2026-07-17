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
    <div class="main">
      <ul>
        <li data-pane-select id="row"><span id="row-label">task</span></li>
      </ul>
      <div id="blank">blank body space</div>
    </div>
    <div class="setup__overlay"><button id="modal-btn">ok</button></div>
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
