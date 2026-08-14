// @vitest-environment jsdom
// The folded way in for an AI whose host cannot open a folder (`AMB-D-671`, `AMB-D-672`, `AMB-D-673`).
// Only the boundary is stubbed (the per-project read, the bundle write and the clipboard); the block's
// own branching and wording run for real.
//
// What these guard: **it is folded**, so a reader on the command line walks past a line rather than a
// list; **the two roads are one per row**, the app that cannot run a command getting a file and the rest
// getting a request; **the folder is drawn beside "set up"**, since set up for which folder is the half a
// reader cannot work out; **the removal is offered only where there is something to remove**; and that a
// project with nothing to offer draws **nothing at all** rather than a heading over an empty list.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { McpAppDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  /** What the per-project read answers with. */
  apps: [] as McpAppDto[],
  /** Which projects it was asked about, in order — evidence it is read per project. */
  asked: [] as number[],
  /** Every bundle write, by project — the one move on this block that writes anything. */
  written: [] as number[],
  /** Where the write says the file landed, or `null` for a picker that was closed. */
  writtenTo: "/w/downloads/amenbo-shop.mcpb" as string | null,
}));

vi.mock("../core/snapshot", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/snapshot")>();
  return { ...orig, inTauri: () => true, subscribe: () => () => {} };
});
vi.mock("../core/mutations", () => ({
  fetchMcpApps: (projectId: number) => {
    hoisted.asked.push(projectId);
    return Promise.resolve(hoisted.apps);
  },
  saveMcpBundle: (projectId: number) => {
    hoisted.written.push(projectId);
    return Promise.resolve(hoisted.writtenTo);
  },
}));

import { t, tf } from "../core/i18n";
import { McpSetup } from "./McpSetup";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
let clipboard: string[];

function app(over: Partial<McpAppDto> = {}): McpAppDto {
  return {
    app: "cursor",
    label: "Cursor",
    writesFile: false,
    configured: false,
    folder: null,
    addRequest: "Please add the Amenbo project to Cursor",
    removeRequest: "Please remove the Amenbo project from Cursor",
    ...over,
  };
}

async function render(projectId = 7) {
  await act(async () => {
    root.render(createElement(McpSetup, { projectId }));
  });
}

/** Open the disclosure, which is where everything but the one line lives. */
async function unfold() {
  await act(async () => {
    button(t("mcp.open")).click();
  });
}

function button(label: string): HTMLButtonElement {
  const found = [...container.querySelectorAll("button")].find((b) => b.textContent?.includes(label));
  if (!found) throw new Error(`no button labelled ${label}`);
  return found as HTMLButtonElement;
}
const buttons = () => [...container.querySelectorAll("button")].map((b) => b.textContent ?? "");
const rows = () => [...container.querySelectorAll(".mcp__app")];

beforeEach(() => {
  hoisted.apps = [];
  hoisted.asked = [];
  hoisted.written = [];
  hoisted.writtenTo = "/w/downloads/amenbo-shop.mcpb";
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

describe("the way in for an AI that cannot open a folder", () => {
  it("draws nothing where there is nothing to offer", async () => {
    await render();
    expect(container.textContent).toBe("");
    expect(hoisted.asked).toEqual([7]);
  });

  it("is folded: the apps are behind a line, not on the screen", async () => {
    hoisted.apps = [app()];
    await render();

    expect(rows()).toHaveLength(0);
    expect(container.textContent).toContain(t("mcp.title"));

    await unfold();
    expect(rows()).toHaveLength(1);
    expect(container.textContent).toContain(t("mcp.hint"));
  });

  it("gives the app that cannot run a command a file, and the rest a request", async () => {
    hoisted.apps = [app({ app: "claude-desktop", label: "Claude Desktop", writesFile: true }), app()];
    await render();
    await unfold();

    expect(buttons()).toContain(t("mcp.write"));
    expect(buttons()).toContain(t("mcp.copyAdd"));

    await act(async () => { button(t("mcp.copyAdd")).click(); });
    expect(clipboard).toEqual(["Please add the Amenbo project to Cursor"]);

    await act(async () => { button(t("mcp.write")).click(); });
    expect(hoisted.written).toEqual([7]);
    expect(container.textContent).toContain(tf("mcp.written", { path: "/w/downloads/amenbo-shop.mcpb" }));
  });

  it("says which folder an app is set up for, and offers to take it back out", async () => {
    hoisted.apps = [app({ configured: true, folder: "/w/elsewhere" })];
    await render();
    await unfold();

    expect(container.textContent).toContain(t("mcp.configured"));
    // The folder as well as the answer: which one it is set up for is the half a reader cannot work out.
    expect(container.textContent).toContain("/w/elsewhere");

    await act(async () => { button(t("mcp.copyRemove")).click(); });
    expect(clipboard).toEqual(["Please remove the Amenbo project from Cursor"]);
  });

  it("offers no removal for an app that holds nothing", async () => {
    hoisted.apps = [app()];
    await render();
    await unfold();

    expect(container.textContent).toContain(t("mcp.unconfigured"));
    expect(buttons()).not.toContain(t("mcp.copyRemove"));
  });

  // The picker closed without a folder is not a write: nothing landed, so nothing is said about where.
  it("says nothing about a file the reader never chose a place for", async () => {
    hoisted.apps = [app({ writesFile: true })];
    hoisted.writtenTo = null;
    await render();
    await unfold();

    await act(async () => { button(t("mcp.write")).click(); });
    expect(container.querySelector(".mcp__saved")).toBeNull();
  });
});
