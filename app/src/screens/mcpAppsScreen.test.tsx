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
// up; and that a machine with no folder anywhere says so rather than drawing rows nobody can act on.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { McpAppDto, McpProjectDto, McpSetupDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  /** What the screen's one read answers with. */
  setup: null as McpSetupDto | null,
  /** Every request the rows asked for, as `app:ids` — evidence the texts follow the ticks. */
  asked: [] as string[],
  /** Every bundle write, by the projects ticked when the button was pressed. */
  written: [] as number[][],
  /** Where the write says the file landed, or `null` for a picker that was closed. */
  writtenTo: "/w/downloads/amenbo.mcpb" as string | null,
}));

vi.mock("../core/snapshot", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/snapshot")>();
  return { ...orig, inTauri: () => true, subscribe: () => () => {} };
});
vi.mock("../core/mutations", () => ({
  fetchMcpSetup: () => Promise.resolve(hoisted.setup),
  fetchMcpRequest: (app: string, projectIds: number[]) => {
    hoisted.asked.push(`${app}:${projectIds.join(",")}`);
    return Promise.resolve({
      add: `add ${app} for ${projectIds.join(",")}`,
      remove: `remove ${app}`,
    });
  },
  saveMcpBundle: (projectIds: number[]) => {
    hoisted.written.push(projectIds);
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

async function render(setup: McpSetupDto) {
  hoisted.setup = setup;
  await act(async () => {
    root.render(createElement(McpAppsScreen));
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

beforeEach(() => {
  hoisted.setup = null;
  hoisted.asked = [];
  hoisted.written = [];
  hoisted.writtenTo = "/w/downloads/amenbo.mcpb";
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

  it("opens each row's ticks on the projects that app already reaches", async () => {
    await render({
      projects: [SHOP, GREENHOUSE],
      apps: [app({ configured: true, folders: ["/w/greenhouse"] })],
    });

    expect(ticks().map((box) => box.checked)).toEqual([false, true]);
    // And the folder is drawn beside the answer: set up for *which* folder is the half a reader
    // cannot work out for themselves.
    expect(container.textContent).toContain(t("mcp.configured"));
    expect(container.textContent).toContain("/w/greenhouse");
    // The text is asked for the selection the row opened on, not for an empty one.
    expect(hoisted.asked).toEqual(["cursor:2"]);
  });

  it("carries the whole selection: the text follows the ticks, and copying hands over that text", async () => {
    await render({ projects: [SHOP, GREENHOUSE], apps: [app()] });
    expect(hoisted.asked).toEqual(["cursor:"]); // nothing ticked is still an answer

    await act(async () => { ticks()[0].click(); });
    await act(async () => { ticks()[1].click(); });
    expect(hoisted.asked).toEqual(["cursor:", "cursor:1", "cursor:1,2"]);

    await act(async () => { button(t("mcp.copyAdd")).click(); });
    expect(clipboard).toEqual(["add cursor for 1,2"]);
  });

  it("gives the app that cannot run a command a file, written for the projects ticked", async () => {
    await render({
      projects: [SHOP, GREENHOUSE],
      apps: [app({ app: "claude-desktop", label: "Claude Desktop", writesFile: true }), app()],
    });

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
    expect(container.textContent).toContain(t("mcp.stale"));
    expect(container.textContent).toContain("amenbo-shop");

    await act(async () => { button(t("mcp.copyRemove")).click(); });
    expect(clipboard).toEqual(["clear amenbo-shop"]);
  });

  it("offers no removal for an app that holds nothing", async () => {
    await render({ projects: [SHOP], apps: [app()] });

    expect(container.textContent).toContain(t("mcp.unconfigured"));
    expect(buttons()).not.toContain(t("mcp.copyRemove"));
  });

  // The picker closed without a folder is not a write: nothing landed, so nothing is said about where.
  it("says nothing about a file the reader never chose a place for", async () => {
    hoisted.writtenTo = null;
    await render({ projects: [SHOP], apps: [app({ writesFile: true })] });

    await act(async () => { ticks()[0].click(); });
    await act(async () => { button(t("mcp.write")).click(); });
    expect(container.querySelector(".mcp__saved")).toBeNull();
  });
});
