// @vitest-environment jsdom
// Which half of the file face is up, and the one way the half that is not up can call.
//
// The two questions are one design. The panel opens on the memo because a person opens the memo
// themselves and loses nothing by its being closed; the files are pointed at by an agent, and an
// agent cannot send anybody there (`../talk/columns`). So moving the default costs the `point` verb
// its only way of being noticed — unless the files half can knock, which is the badge here.
//
// **What the badge must not become is furniture.** It goes up for what an agent said and for nothing
// else, and once the person has been on the half it stays down until something is said again — the
// same rule the terminal segment's badge is held to, for the same reason (`./terminalBadge`).
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SessionSaidDto } from "../bindings/bindings";
import type { PaneEvents, PaneStart } from "../talk/terminal";

const hoisted = vi.hoisted(() => ({
  /** Each pane's way of saying something, in the order the panes were opened. */
  says: [] as ((statement: SessionSaidDto) => void)[],
}));

vi.mock("../mock/adapter", () => ({
  dataAdapter: { listProjects: () => [{ id: 1, name: "amenbo" }] },
}));
vi.mock("../core/boundFolders", () => ({
  useBoundFolders: () => ({
    all: [{ path: "/repo", exists: true }],
    live: [{ path: "/repo", exists: true }],
    answered: true,
  }),
}));

// The frame a slot puts up, stood in for: what is under test is the row above the panes and the
// panel beside them. It opens straight away, because a pane with no session has pointed at nothing.
vi.mock("../talk/agent", () => ({
  mountAgentFrame: (
    _host: HTMLElement,
    _lang: string,
    on: PaneEvents,
    start: PaneStart = {},
  ) => {
    hoisted.says.push((statement) => on.said(statement));
    on.opened(start.session ?? `s${hoisted.says.length}`, "2026-08-24T00:00:00Z", "/repo");
    return Promise.resolve(() => {});
  },
}));

import { TerminalFace } from "./TerminalFace";
import { t } from "../core/i18n";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

const q = (sel: string) => container.querySelector<HTMLElement>(sel);
const click = async (el: HTMLElement | null) => {
  await act(async () => { el?.dispatchEvent(new MouseEvent("click", { bubbles: true })); });
};
/** A button on the top row, by what it says. */
const bar = (name: string) =>
  [...container.querySelectorAll<HTMLElement>(".termface__bar button")]
    .find((one) => (one.getAttribute("aria-label") ?? one.textContent) === name) ?? null;
/** The order the file face's two halves are offered in. */
const halves = () =>
  [...container.querySelectorAll<HTMLElement>(".termface__sides button")]
    .map((one) => one.textContent);
/** Whether the files half is wearing its badge. */
const badged = () => q(".termface__pointed") !== null;

const mount = async () => {
  await act(async () => {
    root.render(createElement(TerminalFace, { onWindow: () => {}, note: null, onWaiting: () => {} }));
  });
};
/** Open a pane, which is what gives the face a session to hear from. */
const openPane = async () => {
  await click(container.querySelector<HTMLElement>(".slot--empty .slot__open"));
};
/** That pane's agent points at something. */
const point = async (pane: number, at: string) => {
  await act(async () => {
    hoisted.says[pane]!({
      session: `s${pane + 1}`, at, verb: "point", target: "src/main.rs", why: "the failing line",
    });
  });
};

beforeEach(() => {
  // What half is up is kept on the device, so a test that inherited the last one's answer would be
  // testing the run before it (`../talk/columns`).
  localStorage.clear();
  hoisted.says = [];
  // Wide enough for the panes to have columns beside them rather than drawers.
  window.innerWidth = 1600;
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the half the file face opens on", () => {
  it("is the memo, and the memo is offered first", async () => {
    await mount();
    expect(q(".memo__field")).not.toBeNull();
    expect(halves()).toEqual([t("files.memo"), t("files.tab")]);
  });

  it("is whichever half was up last, so the default is only the first run's", async () => {
    await mount();
    await click(bar(t("files.tab")));
    await act(async () => root.unmount());
    root = createRoot(container);
    await mount();
    expect(q(".memo__field")).toBeNull();
  });
});

describe("the badge on the files half", () => {
  it("is not up while nothing has been pointed at", async () => {
    await mount();
    await openPane();
    expect(badged()).toBe(false);
  });

  it("goes up the moment an agent points at something", async () => {
    await mount();
    await openPane();
    await point(0, "1");
    expect(badged()).toBe(true);
  });

  it("is never up while the files half is the one being looked at", async () => {
    await mount();
    await openPane();
    await click(bar(t("files.tab")));
    await point(0, "1");
    expect(badged()).toBe(false);
  });

  it("goes down on being looked at, and stays down when the panel is closed again", async () => {
    await mount();
    await openPane();
    await point(0, "1");
    await click(bar(t("files.tab")));
    expect(badged()).toBe(false);
    // Closing the panel is not something coming up: the person has been shown what was there.
    await click(q(".files__close"));
    expect(badged()).toBe(false);
    // Nor is going back to the memo.
    await click(bar(t("files.memo")));
    expect(badged()).toBe(false);
  });

  it("goes up again for the next thing pointed at", async () => {
    await mount();
    await openPane();
    await point(0, "1");
    await click(bar(t("files.tab")));
    await click(bar(t("files.memo")));
    await point(0, "2");
    expect(badged()).toBe(true);
  });
});
