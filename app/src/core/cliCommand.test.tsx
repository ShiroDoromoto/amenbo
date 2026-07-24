// @vitest-environment jsdom
// The name every screen that hands over a command asks for. Only the boundary is stubbed (core's
// build-time channel, which a test cannot vary); the hook itself runs for real.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const hoisted = vi.hoisted(() => ({
  /** What core answers for this build's CLI name; null is the browser, with no build to ask. */
  cmd: "amenbo" as string | null,
  /** How many times it was asked — the evidence it is a mount-time question, not a per-render one. */
  calls: 0,
  /** Set to reject instead, for the unanswerable case. */
  fails: false,
}));

vi.mock("./mutations", () => ({
  fetchCliCommandName: () => {
    hoisted.calls += 1;
    return hoisted.fails ? Promise.reject(new Error("no")) : Promise.resolve(hoisted.cmd);
  },
}));

import { useCliCommandName } from "./cliCommand";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

function Probe() {
  return createElement("span", null, useCliCommandName());
}

async function render() {
  await act(async () => {
    root.render(createElement(Probe));
  });
}

beforeEach(() => {
  hoisted.cmd = "amenbo";
  hoisted.calls = 0;
  hoisted.fails = false;
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("useCliCommandName", () => {
  it("answers what this build installs", async () => {
    hoisted.cmd = "amenbo-dev";
    await render();
    expect(container.textContent).toBe("amenbo-dev");
  });

  it("stands at the production name when there is no build to ask", async () => {
    hoisted.cmd = null;
    await render();
    expect(container.textContent).toBe("amenbo");
  });

  it("stands at the production name when the question is refused", async () => {
    hoisted.fails = true;
    await render();
    expect(container.textContent).toBe("amenbo");
  });

  it("is asked once, not on every render", async () => {
    hoisted.cmd = "amenbo-dev";
    await render();
    await render(); // a re-render of the same tree
    expect(hoisted.calls).toBe(1);
  });
});
