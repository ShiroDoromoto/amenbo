// @vitest-environment jsdom
// The pull-down is the GUI's only door to `rejected`, and the door is what is under test: the reason is
// asked for before anything is written, an unanswered question writes nothing, and every other value
// still lands on the first pick. Only the snapshot boundary is stubbed (the language i18n reads, and the
// facet roster); the control's own branching runs for real.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Status } from "../mock/types";

vi.mock("../core/snapshot", () => ({
  inTauri: () => false,
  getSnapshot: () => ({ language: "ja", roster: [] }),
}));

import { StatusSelect } from "./atoms";

let host: HTMLDivElement;
let root: Root;
let calls: Array<[number, Status, string | undefined]>;

async function render(status: Status = "todo") {
  await act(async () => {
    root.render(createElement(StatusSelect, {
      id: 7,
      status,
      onStatus: (id: number, next: Status, reason?: string) => { calls.push([id, next, reason]); },
    }));
  });
}

/** Pick a value the way a user does — the select is controlled, so the change event is the whole gesture. */
async function pick(value: Status) {
  const select = host.querySelector("select")!;
  await act(async () => {
    select.value = value;
    select.dispatchEvent(new Event("change", { bubbles: true }));
  });
}

/**
 * Type into a controlled textarea. The value has to go through the prototype's setter: React tracks the
 * last value it wrote on the node, and assigning `.value` directly updates it behind React's back, so the
 * change event that follows is discarded as "no change".
 */
async function type(el: HTMLTextAreaElement, text: string) {
  const setValue = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")!.set!;
  await act(async () => {
    setValue.call(el, text);
    el.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

const dialog = () => document.querySelector("[role=dialog]");
const button = (label: string) =>
  [...document.querySelectorAll<HTMLButtonElement>("[role=dialog] button")].find((b) => b.textContent === label)!;

beforeEach(() => {
  calls = [];
  host = document.createElement("div");
  document.body.appendChild(host);
  root = createRoot(host);
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
});

describe("StatusSelect", () => {
  it("offers every status, the two terminals included", async () => {
    await render();
    const values = [...host.querySelectorAll("option")].map((o) => o.value);
    expect(values).toEqual(["todo", "in_progress", "blocked", "done", "rejected"]);
  });

  it("writes the ordinary values on the pick itself, with no reason attached", async () => {
    await render();
    await pick("done");
    expect(calls).toEqual([[7, "done", undefined]]);
    expect(dialog()).toBeNull();
  });

  it("asks for the reason before rejecting, and hands it on with the status", async () => {
    await render();
    await pick("rejected");
    expect(calls).toEqual([]); // Nothing is written while the question is still up.
    expect(dialog()).not.toBeNull();

    const input = document.querySelector<HTMLTextAreaElement>("[role=dialog] textarea")!;
    // The confirm is dead until something is typed — this is what makes the reason required.
    expect(button("却下する").disabled).toBe(true);
    await type(input, "  測っても何も変わらなかった  ");
    expect(button("却下する").disabled).toBe(false);
    await act(async () => button("却下する").click());

    expect(calls).toEqual([[7, "rejected", "測っても何も変わらなかった"]]);
    expect(dialog()).toBeNull();
  });

  it("writes nothing when the question is dropped", async () => {
    await render();
    await pick("rejected");
    await act(async () => button("やめる").click());

    expect(calls).toEqual([]);
    expect(dialog()).toBeNull();
    // The control is set from the status it was given, so dropping the question leaves it where it was.
    expect(host.querySelector("select")!.value).toBe("todo");
  });
});
