// @vitest-environment jsdom
// The screen where an AI is connected (`AMB-D-672`, `AMB-D-673`, `AMB-D-681`). Only the boundary is
// stubbed (the read, the request texts, the bundle write and the clipboard); the screen's own
// branching and wording run for real.
//
// What these guard: **the ticks open on what the app already reaches**, so a reader who has set it up
// before does not have to rebuild their own selection before touching anything; **the whole selection
// travels**, the texts following the ticks rather than the button; **the two roads are one per row**,
// the app that cannot run a command getting a file and the rest a request; **an old entry is drawn
// apart** from the row's own state, since it is something to take away rather than this app being set
// up; that a machine with no folder anywhere says so rather than drawing rows nobody can act on; and
// that **reading, unread and empty are three answers** (`AMB-D-690`), a write that failed being said on
// the row whose button wrote rather than once on top of eight identical rows.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { McpAppDto, McpProjectDto, McpSetupDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  /** What the screen's read answers with — reassign it to stand for the world having moved on. */
  setup: null as McpSetupDto | null,
  /** How many times the screen read it, which is what the re-read on return is counted by. */
  reads: 0,
  /** Every request the rows asked for, as `app:ids` — evidence the texts follow the ticks. */
  asked: [] as string[],
  /** Every bundle write, by the projects ticked when the button was pressed. */
  written: [] as number[][],
  /** Where the write says the file landed, or `null` for a picker that was closed. */
  writtenTo: "/w/downloads/amenbo.mcpb" as string | null,
  /** Set it to hold the read open — the moment the screen is reading and has nothing yet. */
  stall: false,
  /** Set it to make the read fail with these words instead of answering. */
  readFails: null as string | null,
  /** Set it to make the bundle write fail with these words instead of landing. */
  writeFails: null as string | null,
}));

vi.mock("../core/snapshot", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/snapshot")>();
  return { ...orig, inTauri: () => true, subscribe: () => () => {} };
});
vi.mock("../core/mutations", () => ({
  fetchMcpSetup: () => {
    hoisted.reads += 1;
    if (hoisted.stall) return new Promise(() => {});
    if (hoisted.readFails !== null) return Promise.reject(new Error(hoisted.readFails));
    return Promise.resolve(hoisted.setup);
  },
  fetchMcpRequest: (app: string, projectIds: number[]) => {
    hoisted.asked.push(`${app}:${projectIds.join(",")}`);
    return Promise.resolve({
      add: `add ${app} for ${projectIds.join(",")}`,
      remove: `remove ${app}`,
    });
  },
  saveMcpBundle: (projectIds: number[]) => {
    hoisted.written.push(projectIds);
    if (hoisted.writeFails !== null) return Promise.reject(new Error(hoisted.writeFails));
    return Promise.resolve(hoisted.writtenTo);
  },
}));

import { t, tf } from "../core/i18n";
import { McpAppsScreen } from "./McpAppsScreen";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
let clipboard: string[];

const SHOP: McpProjectDto = { id: 1, name: "Shop", folder: "/w/shop" };
const GREENHOUSE: McpProjectDto = { id: 2, name: "Greenhouse", folder: "/w/greenhouse" };

function app(over: Partial<McpAppDto> = {}): McpAppDto {
  return {
    app: "cursor",
    label: "Cursor",
    writesFile: false,
    configured: false,
    folders: [],
    stale: [],
    ...over,
  };
}

async function render(setup: McpSetupDto, pick: number | null = null) {
  hoisted.setup = setup;
  await act(async () => {
    root.render(createElement(McpAppsScreen, { pick }));
  });
}

function button(label: string): HTMLButtonElement {
  const found = [...container.querySelectorAll("button")].find((b) => b.textContent?.includes(label));
  if (!found) throw new Error(`no button labelled ${label}`);
  return found as HTMLButtonElement;
}
const buttons = () => [...container.querySelectorAll("button")].map((b) => b.textContent ?? "");
const rows = () => [...container.querySelectorAll(".mcp__app")];
const ticks = () => [...container.querySelectorAll<HTMLInputElement>(".mcp__project input")];
const heads = () => [...container.querySelectorAll<HTMLButtonElement>(".mcp__head")];

/** Open the row at `at`, the rows being folded to start with (`AMB-D-690`). */
async function openRow(at = 0) {
  await act(async () => { heads()[at].click(); });
}

/**
 * A clock the test moves. The screen throttles a burst of returns on `Date.now`, so a test that wants
 * a second read has to be past that window — and only `Date` is stood in for, leaving React's own
 * scheduling alone.
 */
function fakeClock() {
  let at = Date.now();
  const spy = vi.spyOn(Date, "now").mockImplementation(() => at);
  return {
    advance: (ms: number) => { at += ms; },
    restore: () => spy.mockRestore(),
  };
}

beforeEach(() => {
  hoisted.setup = null;
  hoisted.reads = 0;
  hoisted.asked = [];
  hoisted.written = [];
  hoisted.writtenTo = "/w/downloads/amenbo.mcpb";
  hoisted.stall = false;
  hoisted.readFails = null;
  hoisted.writeFails = null;
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

describe("the screen where an AI is connected", () => {
  it("says there is nowhere to point a server where no project has a folder", async () => {
    await render({ projects: [], apps: [app()] });

    expect(container.textContent).toContain(t("mcp.noProjects"));
    expect(rows()).toHaveLength(0);
  });

  // The same shell as every other screen (`AMB-D-690`): the heading in the band on top, the rows in a
  // section card. The heading is held to the sidebar entrance's own words, because a reader who
  // pressed "connect via MCP" arriving at a different name reads it as a different place.
  it("wears the shell the other screens wear, under the name the reader pressed", async () => {
    await render({ projects: [SHOP], apps: [app()] });

    expect(t("mcp.title")).toBe(t("nav.mcp"));
    expect(container.querySelector(".board__toolbar")?.textContent).toContain(t("mcp.title"));
    expect(container.querySelector(".settings__section .mcp__apps")).not.toBeNull();
  });

  // Folded, one at a time (`AMB-D-690`). Eight apps each holding the whole column of projects is the
  // page this replaces, so what a second row opening has to do is close the first — otherwise the
  // reader is back to reading the same column twice over.
  it("keeps the rows folded, and opens one at a time", async () => {
    await render({
      projects: [SHOP, GREENHOUSE],
      apps: [app(), app({ app: "vscode", label: "VS Code" })],
    });

    expect(rows()).toHaveLength(2);
    expect(ticks()).toHaveLength(0);
    expect(heads().map((h) => h.getAttribute("aria-expanded"))).toEqual(["false", "false"]);

    await openRow(0);
    expect(heads().map((h) => h.getAttribute("aria-expanded"))).toEqual(["true", "false"]);
    expect(ticks()).toHaveLength(2); // one row's worth, not both rows'

    await openRow(1);
    expect(heads().map((h) => h.getAttribute("aria-expanded"))).toEqual(["false", "true"]);
    expect(ticks()).toHaveLength(2);

    // And pressing the open one shuts it: the reader who opened a row by mistake folds it away again.
    await openRow(1);
    expect(heads().map((h) => h.getAttribute("aria-expanded"))).toEqual(["false", "false"]);
    expect(ticks()).toHaveLength(0);
  });

  // A row keeps the ticks it was given while it is shut. Rebuilding them off the settings on the way
  // back would quietly drop what the reader chose and never wrote.
  it("keeps a row's ticks while it is folded away", async () => {
    await render({ projects: [SHOP, GREENHOUSE], apps: [app()] });

    await openRow();
    await act(async () => { ticks()[1].click(); });
    expect(ticks().map((box) => box.checked)).toEqual([false, true]);

    await openRow(); // shut
    await openRow(); // and back
    expect(ticks().map((box) => box.checked)).toEqual([false, true]);
  });

  // Arriving from a project that was just made ticks that project, but opens nothing: which app to set
  // up is the reader's question, and there are eight of them to pick from.
  it("opens no row where the screen was walked in holding a project", async () => {
    await render({ projects: [SHOP, GREENHOUSE], apps: [app()] }, SHOP.id);

    expect(heads().map((h) => h.getAttribute("aria-expanded"))).toEqual(["false"]);
    expect(ticks()).toHaveLength(0);
  });

  it("opens each row's ticks on the projects that app already reaches", async () => {
    await render({
      projects: [SHOP, GREENHOUSE],
      apps: [app({ configured: true, folders: ["/w/greenhouse"] })],
    });

    // The folder is drawn beside the answer while the row is still folded: set up for *which* folder
    // is the half a reader cannot work out for themselves, and it is what the list is scanned by.
    expect(container.textContent).toContain(t("mcp.configured"));
    expect(container.textContent).toContain("/w/greenhouse");
    expect(ticks()).toHaveLength(0);

    await openRow();
    expect(ticks().map((box) => box.checked)).toEqual([false, true]);
    // The text is asked for the selection the row opened on, not for an empty one.
    expect(hoisted.asked).toEqual(["cursor:2"]);
  });

  // Walking in from a project that was just made (`AMB-D-684`): the row opens on that project *and* on
  // what the app already reaches, since arriving this way is not a reason to drop a folder it was set
  // up for.
  it("ticks the project the reader walked in holding, on top of what the app already reaches", async () => {
    await render(
      { projects: [SHOP, GREENHOUSE], apps: [app({ configured: true, folders: ["/w/greenhouse"] })] },
      SHOP.id,
    );

    await openRow();
    expect(ticks().map((box) => box.checked)).toEqual([true, true]);
    expect(hoisted.asked).toEqual(["cursor:2,1"]);
  });

  // A project with no folder is not on this screen at all, so a way in naming one has nothing to tick
  // — and the row opens exactly as it would have without it.
  it("ignores a project it was walked in holding that has no folder here", async () => {
    await render({ projects: [SHOP], apps: [app()] }, 99);

    await openRow();
    expect(ticks().map((box) => box.checked)).toEqual([false]);
    expect(hoisted.asked).toEqual(["cursor:"]);
  });

  it("carries the whole selection: the text follows the ticks, and copying hands over that text", async () => {
    await render({ projects: [SHOP, GREENHOUSE], apps: [app()] });
    expect(hoisted.asked).toEqual(["cursor:"]); // nothing ticked is still an answer

    await openRow();
    await act(async () => { ticks()[0].click(); });
    await act(async () => { ticks()[1].click(); });
    expect(hoisted.asked).toEqual(["cursor:", "cursor:1", "cursor:1,2"]);

    await act(async () => { button(t("mcp.copyAdd")).click(); });
    expect(clipboard).toEqual(["add cursor for 1,2"]);
  });

  // Nothing ticked is nothing to hand over, and the two roads used to say so differently: the file was
  // already shut on it while the request went out carrying a `--dir` with no value, which the AI
  // reading it writes into the settings as an entry that cannot run.
  it("shuts the request on an empty selection, the way the file already was", async () => {
    await render({ projects: [SHOP], apps: [app()] });
    await openRow();

    expect(button(t("mcp.copyAdd")).disabled).toBe(true);

    await act(async () => { ticks()[0].click(); });
    expect(button(t("mcp.copyAdd")).disabled).toBe(false);
  });

  // What the ticks amount to, said where the press happens: they are the contents of what goes over,
  // and what goes over replaces rather than adds. Neither is readable off the button's own word.
  it("says beside the button what the ticks are, and that handing them over replaces", async () => {
    await render({ projects: [SHOP], apps: [app()] });

    expect(container.textContent).not.toContain(t("mcp.handover"));
    await openRow();
    expect(container.textContent).toContain(t("mcp.handover"));
  });

  // The removal is not a selection: it asks for the whole entry gone, so an empty one is no reason to
  // shut it — a reader taking amenbo back out has nothing to tick first.
  it("leaves the removal live with nothing ticked", async () => {
    await render({ projects: [SHOP], apps: [app({ configured: true, folders: ["/w/elsewhere"] })] });
    await openRow();

    expect(ticks().map((box) => box.checked)).toEqual([false]);
    expect(button(t("mcp.copyRemove")).disabled).toBe(false);
  });

  it("gives the app that cannot run a command a file, written for the projects ticked", async () => {
    await render({
      projects: [SHOP, GREENHOUSE],
      apps: [app({ app: "claude-desktop", label: "Claude Desktop", writesFile: true }), app()],
    });

    await openRow();
    expect(buttons()).toContain(t("mcp.write"));
    // Nothing ticked is nothing to write: a bundle naming no folder reaches no project.
    expect(button(t("mcp.write")).disabled).toBe(true);

    await act(async () => { ticks()[0].click(); });
    await act(async () => { button(t("mcp.write")).click(); });
    expect(hoisted.written).toEqual([[1]]);
    expect(container.textContent).toContain(tf("mcp.written", { path: "/w/downloads/amenbo.mcpb" }));
  });

  it("draws what an older amenbo left apart from the row's own state, with its own removal", async () => {
    await render({
      projects: [SHOP],
      apps: [app({ stale: [{ name: "amenbo-shop", folder: "/w/shop", removeRequest: "clear amenbo-shop" }] })],
    });

    // Not set up — an old entry is not this app holding the server amenbo writes today.
    expect(container.textContent).toContain(t("mcp.unconfigured"));

    await openRow();
    expect(container.textContent).toContain(t("mcp.stale"));
    expect(container.textContent).toContain("amenbo-shop");

    await act(async () => { button(t("mcp.copyRemove")).click(); });
    expect(clipboard).toEqual(["clear amenbo-shop"]);
  });

  it("offers no removal for an app that holds nothing", async () => {
    await render({ projects: [SHOP], apps: [app()] });

    expect(container.textContent).toContain(t("mcp.unconfigured"));

    await openRow();
    expect(buttons()).not.toContain(t("mcp.copyRemove"));
  });

  // On the request road the writer is the other app's AI: `onWritten` never fires, and `mcp_probe`
  // reads that app's settings file rather than the store, so no change feed carries it. Coming back to
  // the window is the only moment the screen has to notice.
  it("reads the screen again when the reader comes back to the window", async () => {
    const clock = fakeClock();
    try {
      await render({ projects: [SHOP], apps: [app()] });
      expect(hoisted.reads).toBe(1);
      expect(container.textContent).toContain(t("mcp.unconfigured"));

      // While the reader was away in the other app, its AI put the entry in place.
      hoisted.setup = { projects: [SHOP], apps: [app({ configured: true, folders: ["/w/shop"] })] };

      clock.advance(2000);
      await act(async () => { window.dispatchEvent(new Event("focus")); });

      expect(hoisted.reads).toBe(2);
      expect(container.textContent).toContain(t("mcp.configured"));
      expect(container.textContent).toContain("/w/shop");
    } finally {
      clock.restore();
    }
  });

  // A window can come back by either event, and often by both at once — which is one return, not three.
  it("folds a burst of returns into one re-read", async () => {
    const clock = fakeClock();
    try {
      await render({ projects: [SHOP], apps: [app()] });
      expect(hoisted.reads).toBe(1);

      clock.advance(2000);
      await act(async () => {
        window.dispatchEvent(new Event("focus"));
        document.dispatchEvent(new Event("visibilitychange"));
        window.dispatchEvent(new Event("focus"));
      });

      expect(hoisted.reads).toBe(2);
    } finally {
      clock.restore();
    }
  });

  // The picker closed without a folder is not a write: nothing landed, so nothing is said about where.
  it("says nothing about a file the reader never chose a place for", async () => {
    hoisted.writtenTo = null;
    await render({ projects: [SHOP], apps: [app({ writesFile: true })] });

    await openRow();
    await act(async () => { ticks()[0].click(); });
    await act(async () => { button(t("mcp.write")).click(); });
    expect(container.querySelector(".mcp__saved")).toBeNull();
  });

  // The wait has a line of its own, because a card standing empty is the answer a machine with nothing
  // set up gives — a reader who arrives mid-read and reads that presses nothing and leaves.
  it("says it is reading, and stops saying it once the answer is in", async () => {
    const clock = fakeClock();
    try {
      hoisted.stall = true;
      await render({ projects: [SHOP], apps: [app()] });

      expect(container.textContent).toContain(t("app.loading"));
      expect(rows()).toHaveLength(0);

      hoisted.stall = false;
      clock.advance(2000);
      await act(async () => { window.dispatchEvent(new Event("focus")); });

      expect(container.textContent).not.toContain(t("app.loading"));
      expect(rows()).toHaveLength(1);
    } finally {
      clock.restore();
    }
  });

  // A read that fails is a third answer again: the reader is told the list is unread, in the door's own
  // words, rather than left in front of a card that reads as a machine with nothing on it.
  it("says the list could not be read, rather than standing empty", async () => {
    hoisted.readFails = "the store is locked";
    await render({ projects: [SHOP], apps: [app()] });

    const said = container.querySelector(".settings__body > .errortext");
    expect(said?.textContent).toContain(t("app.loadError"));
    expect(said?.textContent).toContain("the store is locked"); // the door's own words, under amenbo's
    expect(container.textContent).not.toContain(t("app.loading"));
    expect(rows()).toHaveLength(0);
  });

  it("puts a write that failed on the row that wrote it", async () => {
    await render({
      projects: [SHOP],
      apps: [app({ app: "claude-desktop", label: "Claude Desktop", writesFile: true }), app()],
    });

    await openRow();
    await act(async () => { ticks()[0].click(); });
    await act(async () => { button(t("mcp.write")).click(); });
    expect(container.textContent).toContain(tf("mcp.written", { path: "/w/downloads/amenbo.mcpb" }));

    hoisted.writeFails = "no room on the disk";
    await act(async () => { button(t("mcp.write")).click(); });

    // On that row, and nowhere else: every row draws the same button, so one message on top of the
    // screen names none of them.
    expect(rows()[0].textContent).toContain("no room on the disk");
    expect(rows()[1].textContent).not.toContain("no room on the disk");
    expect(container.querySelector(".settings__body > .errortext")).toBeNull();
    // And where the last write landed is not left standing beside the one that did not.
    expect(container.querySelector(".mcp__saved")).toBeNull();
  });
});
