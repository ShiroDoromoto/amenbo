// @vitest-environment jsdom
// A path pressed in a pane opens the file it names — once, for that press.
//
// The ask a press leaves is state, and state stands until it is put down. What it is read against is
// the folders the project is bound to, and those change under it every time a reader goes to another
// project. An ask left standing is therefore answered again on the way back, which a reader sees as
// a file they closed coming back and the reading column leaving their notes (`AMB-T-4812`).
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const hoisted = vi.hoisted(() => ({
  bound: { 1: [{ path: "/repo", exists: true }], 2: [{ path: "/other", exists: true }] } as
    Record<number, { path: string; exists: boolean }[]>,
}));

// The pane, stood in for by the one thing this test is about: the path drawn in it, as a press.
// What a terminal does with the characters it drew is `./TerminalPane`'s own, and putting one up is
// a host round trip.
vi.mock("./TerminalPane", () => ({
  TerminalPane: ({ frame, onPath }: { frame: string; onPath: (frame: string, target: string) => void }) =>
    createElement(
      "button",
      { className: "stub-path", onClick: () => onPath(frame, "a.md") },
      "a.md",
    ),
}));
vi.mock("../mock/adapter", () => ({
  dataAdapter: { listProjects: () => [{ id: 1, name: "amenbo" }, { id: 2, name: "other" }] },
}));
// Each project is bound to a folder of its own, which is what makes going to the other one and back
// move the ground the ask is read against.
vi.mock("../core/boundFolders", () => ({
  useBoundFolders: (project: number | null) => {
    const live = project === null ? [] : hoisted.bound[project] ?? [];
    return { all: live, live, answered: true };
  },
}));
vi.mock("../core/mutations", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../core/mutations")>()),
  fetchBoundFolders: async (project: number) => hoisted.bound[project] ?? [],
}));

import { TerminalFace } from "./TerminalFace";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

const q = (sel: string) => container.querySelector<HTMLElement>(sel);
const tabs = () =>
  [...container.querySelectorAll<HTMLElement>(".files__tabname")].map((one) => one.textContent);
const click = (el: HTMLElement | null) => act(async () => {
  el?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
});

/** The face with a pane already in it: the ledger's one button, which is the way a pane is made
 *  without a press on the empty frame having to choose what runs in it. */
const mount = async () => {
  await act(async () => {
    root.render(createElement(TerminalFace, {
      onWindow: () => {},
      note: null,
      openIn: { project: 1, dir: "/repo", nth: 1 },
    }));
  });
  await act(async () => {});
};

/** Go to the project named, by its tab. */
const goTo = async (name: string) => {
  const tab = [...container.querySelectorAll<HTMLElement>(".ptabs__tab")]
    .find((one) => one.textContent?.includes(name)) ?? null;
  await click(tab);
};

beforeEach(() => {
  localStorage.clear();
  window.innerWidth = 1600;
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("a path pressed in a pane", () => {
  it("opens the file it names in the reading column", async () => {
    await mount();
    await click(q(".stub-path"));
    expect(tabs()).toContain("a.md");
  });

  it("is not answered a second time by a project left and come back to", async () => {
    await mount();
    await click(q(".stub-path"));
    expect(tabs()).toContain("a.md");
    await click(q(".files__tabclose"));
    expect(tabs()).not.toContain("a.md");

    await goTo("other");
    await goTo("amenbo");
    expect(tabs()).not.toContain("a.md");
  });

  it("opens it again when the reader presses the same path again", async () => {
    await mount();
    await click(q(".stub-path"));
    await click(q(".files__tabclose"));
    expect(tabs()).not.toContain("a.md");
    await click(q(".stub-path"));
    expect(tabs()).toContain("a.md");
  });
});
