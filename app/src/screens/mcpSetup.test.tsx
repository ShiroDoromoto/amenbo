// @vitest-environment jsdom
// The line on a project's own screens that points at where an AI is connected (`AMB-D-681`).
//
// What these guard: **it is folded**, so a reader on the command line walks past a line rather than a
// list; and that it **points** rather than sets anything up — the choosing is one per app, on the
// screen behind this, and a block that quietly did it here would be the per-project entrance
// `AMB-D-681` took away.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../core/snapshot", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/snapshot")>();
  return { ...orig, inTauri: () => true, subscribe: () => () => {} };
});

import { t } from "../core/i18n";
import { McpSetup } from "./McpSetup";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
let opened: number;

function button(label: string): HTMLButtonElement {
  const found = [...container.querySelectorAll("button")].find((b) => b.textContent?.includes(label));
  if (!found) throw new Error(`no button labelled ${label}`);
  return found as HTMLButtonElement;
}

async function render() {
  await act(async () => {
    root.render(createElement(McpSetup, { onOpen: () => { opened += 1; } }));
  });
}

beforeEach(() => {
  opened = 0;
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the line that points at where an AI is connected", () => {
  it("is folded: what is behind it is one sentence and one way on", async () => {
    await render();

    expect(container.textContent).toContain(t("mcp.title"));
    expect(container.textContent).not.toContain(t("mcp.hint"));

    await act(async () => { button(t("mcp.open")).click(); });
    expect(container.textContent).toContain(t("mcp.hint"));
  });

  it("points rather than sets anything up", async () => {
    await render();
    await act(async () => { button(t("mcp.open")).click(); });

    await act(async () => { button(t("nav.mcp")).click(); });
    expect(opened).toBe(1);
  });
});
