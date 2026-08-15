// @vitest-environment jsdom
// The note that says what the reader pressed went through (`AMB-D-686`).
//
// What this guards: the tick is drawn rather than typed, and the line is a live region. Both were lost
// in the shape this replaced — the mark was a character inside the message string, which is a shape
// nothing can check and a reader whose fonts differ never sees the same way, and nothing announced it.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { DoneNote } from "./DoneNote";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

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

describe("the note that says it went through", () => {
  it("carries the drawn tick and the status role together", async () => {
    await act(async () => { root.render(createElement(DoneNote, { children: "backed up, 12 KB" })); });

    const note = container.querySelector(".donetext");
    expect(note?.getAttribute("role")).toBe("status");
    expect(note?.textContent).toContain("backed up, 12 KB");
    expect(note?.querySelector("svg.icon")?.getAttribute("data-icon")).toBe("check");
  });

  it("keeps the words out of the mark", async () => {
    await act(async () => { root.render(createElement(DoneNote, { children: "restored" })); });

    // The tick carries no text of its own: what the line says is the message, so a reader hearing it
    // read out is not told "check mark" first.
    expect(container.querySelector("svg.icon")?.textContent).toBe("");
  });
});
