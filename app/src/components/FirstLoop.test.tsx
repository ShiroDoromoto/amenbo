// @vitest-environment jsdom
// The part carries `AMB-D-414`'s promise: the user is left with exactly two moves, each of which does
// what its label says and nothing else, and what the copy hands over is a request that can be pasted
// as it is. These tests hold it to that — the seams to core (the terminal, and the name of the CLI
// this build installs) and the clipboard are stubbed, so what runs for real is which button calls what.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const hoisted = vi.hoisted(() => ({
  /** The folders `openTerminal` was asked for. */
  opened: [] as string[],
  /** What `openTerminal` rejects with, when the environment has no terminal to open. */
  terminalFails: null as { code: string; message_en: string } | null,
  /** The CLI this build installs — what the request has to name; null where it installs none. */
  cli: "amenbo" as string | null,
}));

vi.mock("../core/mutations", () => ({
  openTerminal: (path: string) => {
    if (hoisted.terminalFails) return Promise.reject(hoisted.terminalFails);
    hoisted.opened.push(path);
    return Promise.resolve();
  },
  fetchCliCommandName: () => Promise.resolve(hoisted.cli),
}));

import { FirstLoop } from "./FirstLoop";
import { t, tf } from "../core/i18n";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
let clipboard: string[];

const button = (label: string) =>
  Array.from(container.querySelectorAll("button")).find((b) => (b.textContent ?? "").includes(label));

beforeEach(() => {
  hoisted.opened = [];
  hoisted.terminalFails = null;
  hoisted.cli = "amenbo";
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

// Awaited, so the build's CLI name — asked for at mount — has landed before anything is read off the
// screen. Until it does the request stands at the production name, which is the wrong thing to pin.
const render = (dir = "/w/first") =>
  act(async () => { root.render(createElement(FirstLoop, { dir })); });

/** The request as this build hands it over, with the command name filled in. Only the builds that
 *  have one hand a request over at all, so this is asked for nowhere else. */
const prompt = (lang?: "en") => tf("firstloop.prompt", { cmd: hoisted.cli ?? "" }, lang);

describe("the two moves the user is left with", () => {
  it("opens the terminal in the linked folder, and copies nothing on the way", async () => {
    await render("/w/mine");
    await act(async () => { button(t("firstloop.s1btn"))!.click(); });

    expect(hoisted.opened).toEqual(["/w/mine"]);
    expect(clipboard).toEqual([]);
  });

  it("copies the request as it stands, and opens no terminal on the way", async () => {
    await render();
    await act(async () => { button(t("firstloop.s2btn"))!.click(); });

    expect(clipboard).toEqual([prompt()]);
    expect(hoisted.opened).toEqual([]);
    expect(container.textContent).toContain(t("firstloop.copied"));
  });
});

describe("what the request text says", () => {
  // The request is handed over finished, so it has to be readable before it is copied — and what it
  // sends the AI to is the one document that says how to work here, named as a command to run
  // (`AMB-D-515`).
  it("shows the very text the copy hands over, and points the AI at the spec", async () => {
    await render();

    expect(container.textContent).toContain(prompt());
    expect(prompt()).toContain("agent --json");
    expect(prompt("en")).toContain("agent --json");
  });

  // A window on a dev build names a CLI that is not `amenbo`, and the request tells the AI to *run*
  // this — so a fixed name would send the reader's AI to a binary that need not be there.
  it("names the command this build installs, not the production one", async () => {
    hoisted.cli = "amenbo-dev-2627";
    await render();

    expect(container.textContent).toContain("amenbo-dev-2627 agent --json");
    await act(async () => { button(t("firstloop.s2btn"))!.click(); });
    expect(clipboard).toEqual([prompt()]);
  });

  // A Linux preview has a CLI and no way to reach it, so there is no request to hand over. What must
  // not happen is a request naming a command anyway: the reader would paste it and their AI would
  // meet `not found`, having done exactly what this screen told them to.
  it("hands over no request at all where the build ships no command a reader can run", async () => {
    hoisted.cli = null;
    await render();

    expect(container.textContent).toContain(t("cli.none"));
    expect(container.textContent).not.toContain("agent --json");
    expect(button(t("firstloop.s2btn"))).toBeUndefined();
  });
});

describe("when the terminal will not open", () => {
  it("says so instead of failing silently", async () => {
    hoisted.terminalFails = { code: "not_found", message_en: "cannot open" };
    await render();
    await act(async () => { button(t("firstloop.s1btn"))!.click(); });

    // English by default, and the fixture carries both faces, so this pins which one is shown.
    expect(container.querySelector('[role="alert"]')?.textContent).toContain("cannot open");
  });

  // The walk has to keep going: the user opens their own terminal, so what they need is the folder
  // to cd into — handed over the same way the request is, ready to paste.
  it("hands over the folder's path to copy, so the loop still closes", async () => {
    hoisted.terminalFails = { code: "not_found", message_en: "cannot open" };
    await render("/w/mine");
    await act(async () => { button(t("firstloop.s1btn"))!.click(); });

    expect(container.textContent).toContain(t("firstloop.s1fallback"));
    expect(container.textContent).toContain("/w/mine");

    await act(async () => { button(t("firstloop.s1fallbackbtn"))!.click(); });
    expect(clipboard).toEqual(["/w/mine"]);
  });

  it("offers nothing to copy while the terminal still opens", async () => {
    await render();

    expect(button(t("firstloop.s1fallbackbtn"))).toBeUndefined();
  });
});
