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
  /** What the project's folders come back as. */
  folders: [] as Array<{ path: string; exists: boolean }>,
  /** The CLI name this build installs — what the request has to name. */
  cli: "amenbo" as string,
}));

vi.mock("../core/mutations", () => ({
  openTerminal: (path: string) => {
    if (hoisted.terminalFails) return Promise.reject(hoisted.terminalFails);
    hoisted.opened.push(path);
    return Promise.resolve();
  },
  fetchBoundFolders: () => Promise.resolve(hoisted.folders),
  fetchCliCommandName: () => Promise.resolve(hoisted.cli),
}));

import { FirstLoop, ProjectFirstLoop } from "./FirstLoop";
import { t, tf } from "../core/i18n";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
let clipboard: string[];
let linkFolder: number;

const button = (label: string) =>
  Array.from(container.querySelectorAll("button")).find((b) => (b.textContent ?? "").includes(label));

beforeEach(() => {
  hoisted.opened = [];
  hoisted.terminalFails = null;
  hoisted.folders = [];
  hoisted.cli = "amenbo";
  linkFolder = 0;
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

/** The request as this build hands it over, with the command name filled in. */
const prompt = (lang?: "en") => tf("firstloop.prompt", { cmd: hoisted.cli }, lang);

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
});

describe("the same loop, asked for by project", () => {
  const renderForProject = () =>
    act(async () => {
      root.render(createElement(ProjectFirstLoop, { projectId: 7, onLinkFolder: () => { linkFolder++; } }));
    });

  it("finds the project's folder itself and hands it to the loop", async () => {
    hoisted.folders = [{ path: "/w/bound", exists: true }];
    await renderForProject();
    await act(async () => { button(t("firstloop.s1btn"))!.click(); });

    expect(hoisted.opened).toEqual(["/w/bound"]);
  });

  // A folder that has moved away is no folder: there is nothing to open, and nowhere for an AI to write.
  it("invites the reader to link one when every folder it finds has gone", async () => {
    hoisted.folders = [{ path: "/w/gone", exists: false }];
    await renderForProject();

    expect(container.textContent).toContain(t("firstloop.noFolderTitle"));
    expect(button(t("firstloop.s1btn"))).toBeUndefined();
    await act(async () => { button(t("firstloop.noFolderBtn"))!.click(); });
    expect(linkFolder).toBe(1);
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
