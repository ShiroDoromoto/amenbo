// @vitest-environment jsdom
// The part carries `AMB-D-414`'s promise, now that the walk is one press: what the reader is left
// with is starting in the terminal, and the road out to their own terminal folded beside it. These
// tests hold it to that — the seam to core (the name of the CLI this build installs) and the
// clipboard are stubbed, so what runs for real is which control calls what.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const hoisted = vi.hoisted(() => ({
  /** The CLI this build installs — what the request has to name; null where it installs none. */
  cli: "amenbo" as string | null,
}));

vi.mock("../core/mutations", () => ({
  fetchCliCommandName: () => Promise.resolve(hoisted.cli),
}));

import { FirstLoop } from "./FirstLoop";
import { t, tf } from "../core/i18n";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
let clipboard: string[];
/** The folders "start in the terminal" was pressed for. */
let started: string[];

const button = (label: string) =>
  Array.from(container.querySelectorAll("button")).find((b) => (b.textContent ?? "").includes(label));

beforeEach(() => {
  hoisted.cli = "amenbo";
  clipboard = [];
  started = [];
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
  act(async () => { root.render(createElement(FirstLoop, { dir, onStart: (at: string) => started.push(at) })); });

/** Open the way out to the reader's own terminal, which is where the request text lives. */
const openOutside = () => act(async () => { button(t("firstloop.outside"))!.click(); });

/** The request as this build hands it over, with the command name filled in. Only the builds that
 *  have one hand a request over at all, so this is asked for nowhere else. */
const prompt = (lang?: "en") => tf("firstloop.prompt", { cmd: hoisted.cli ?? "" }, lang);

describe("the one move the user is left with", () => {
  it("starts in the linked folder, and copies nothing on the way", async () => {
    await render("/w/mine");
    await act(async () => { button(t("firstloop.start"))!.click(); });

    expect(started).toEqual(["/w/mine"]);
    expect(clipboard).toEqual([]);
  });

  // The request is the second road, not the first one: a reader who presses the one button never
  // meets it, because their agent is handed the same instruction before it starts.
  it("keeps the request out of sight until the way out is opened", async () => {
    await render();

    expect(container.textContent).not.toContain(prompt());

    await openOutside();
    expect(container.textContent).toContain(prompt());
  });
});

describe("the way out to the reader's own terminal", () => {
  // It is folded, not conditional. A machine with no agent this app can start leaves the outside as
  // the only road there is, and a control that appears only sometimes is one nobody can be told about.
  it("is one press away, whatever this build can start", async () => {
    await render();

    expect(button(t("firstloop.outside"))!.getAttribute("aria-expanded")).toBe("false");
    await openOutside();
    expect(button(t("firstloop.outside"))!.getAttribute("aria-expanded")).toBe("true");
  });

  it("copies the request as it stands, and starts nothing on the way", async () => {
    await render();
    await openOutside();
    await act(async () => { button(t("firstloop.copy"))!.click(); });

    expect(clipboard).toEqual([prompt()]);
    expect(started).toEqual([]);
    expect(container.textContent).toContain(t("firstloop.copied"));
  });
});

describe("what the request text says", () => {
  // The request is handed over finished, so it has to be readable before it is copied — and what it
  // sends the AI to is the one document that says how to work here, named as a command to run
  // (`AMB-D-515`).
  it("shows the very text the copy hands over, and points the AI at the spec", async () => {
    await render();
    await openOutside();

    expect(container.textContent).toContain(prompt());
    expect(prompt()).toContain("agent --json");
    expect(prompt("en")).toContain("agent --json");
  });

  // A window on a dev build names a CLI that is not `amenbo`, and the request tells the AI to *run*
  // this — so a fixed name would send the reader's AI to a binary that need not be there.
  it("names the command this build installs, not the production one", async () => {
    hoisted.cli = "amenbo-dev-2627";
    await render();
    await openOutside();

    expect(container.textContent).toContain("amenbo-dev-2627 agent --json");
    await act(async () => { button(t("firstloop.copy"))!.click(); });
    expect(clipboard).toEqual([prompt()]);
  });

  // A Linux preview has a CLI and no way to reach it, so there is no request to hand over. What must
  // not happen is a request naming a command anyway: the reader would paste it and their AI would
  // meet `not found`, having done exactly what this screen told them to.
  it("hands over no request at all where the build ships no command a reader can run", async () => {
    hoisted.cli = null;
    await render();
    await openOutside();

    expect(container.textContent).toContain(t("cli.none"));
    expect(container.textContent).not.toContain("agent --json");
    expect(button(t("firstloop.copy"))).toBeUndefined();
  });
});
