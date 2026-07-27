// @vitest-environment jsdom
// The part carries `AMB-D-414`'s promise: the user is left with exactly two moves, each of which does
// what its label says and nothing else, and what the copy hands over is a request that can be pasted
// as it is. These tests hold it to that — the seam to core (the terminal) and the clipboard are
// stubbed, so what runs for real is which button calls what.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const hoisted = vi.hoisted(() => ({
  /** The folders `openTerminal` was asked for. */
  opened: [] as string[],
  /** What `openTerminal` rejects with, when the environment has no terminal to open. */
  terminalFails: null as { code: string; message: string; message_en: string } | null,
}));

vi.mock("../core/mutations", () => ({
  openTerminal: (path: string) => {
    if (hoisted.terminalFails) return Promise.reject(hoisted.terminalFails);
    hoisted.opened.push(path);
    return Promise.resolve();
  },
}));

import { FirstLoop } from "./FirstLoop";
import { t } from "../core/i18n";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
let clipboard: string[];

const button = (label: string) =>
  Array.from(container.querySelectorAll("button")).find((b) => (b.textContent ?? "").includes(label));

beforeEach(() => {
  hoisted.opened = [];
  hoisted.terminalFails = null;
  clipboard = [];
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: { writeText: (s: string) => { clipboard.push(s); return Promise.resolve(); } },
  });
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

const render = (dir = "/w/first") => act(() => root.render(createElement(FirstLoop, { dir })));

describe("the two moves the user is left with", () => {
  it("opens the terminal in the linked folder, and copies nothing on the way", async () => {
    render("/w/mine");
    await act(async () => { button(t("firstloop.s1btn"))!.click(); });

    expect(hoisted.opened).toEqual(["/w/mine"]);
    expect(clipboard).toEqual([]);
  });

  it("copies the request as it stands, and opens no terminal on the way", async () => {
    render();
    await act(async () => { button(t("firstloop.s2btn"))!.click(); });

    expect(clipboard).toEqual([t("firstloop.prompt")]);
    expect(hoisted.opened).toEqual([]);
    expect(container.textContent).toContain(t("firstloop.copied"));
  });
});

describe("what the request text says", () => {
  // The request is handed over finished, so it has to be readable before it is copied — and it has to
  // carry the one line that makes the loop close for an AI that does not read AGENTS.md on its own.
  it("shows the very text the copy hands over, and points the AI at the guide", () => {
    render();

    expect(container.textContent).toContain(t("firstloop.prompt"));
    expect(t("firstloop.prompt")).toContain("AGENTS.md");
    expect(t("firstloop.prompt", "en")).toContain("AGENTS.md");
  });
});

describe("when the terminal will not open", () => {
  it("says so instead of failing silently", async () => {
    hoisted.terminalFails = { code: "not_found", message: "開けません", message_en: "cannot open" };
    render();
    await act(async () => { button(t("firstloop.s1btn"))!.click(); });

    expect(container.querySelector('[role="alert"]')?.textContent).toContain("開けません");
  });
});
