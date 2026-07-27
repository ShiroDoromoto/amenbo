// @vitest-environment jsdom
// The onboarding steps hand over commands to type. Only the boundary is stubbed (core's build-time
// channel, which a test cannot vary); which command the screen puts on screen runs for real.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const hoisted = vi.hoisted(() => ({
  /** What core answers for this build's CLI name; null is the browser, with no build to ask. */
  cmd: "amenbo" as string | null,
}));

vi.mock("../core/mutations", () => ({
  fetchCliCommandName: () => Promise.resolve(hoisted.cmd),
}));

import { OnboardingScreen } from "./OnboardingScreen";
import { t } from "../core/i18n";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

const commands = () => Array.from(container.querySelectorAll("code")).map((c) => c.textContent ?? "");

async function render() {
  await act(async () => {
    root.render(createElement(OnboardingScreen, { onNav: () => {} }));
  });
}

beforeEach(() => {
  hoisted.cmd = "amenbo";
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("OnboardingScreen commands", () => {
  it("a production build tells the reader to type amenbo", async () => {
    await render();
    expect(commands()).toContain("amenbo init");
    expect(commands().some((c) => c.startsWith("amenbo bind --project"))).toBe(true);
    expect(commands()).toContain("amenbo agent --json");
  });

  it("a dev build names the CLI installed beside it, not the one that is not there", async () => {
    hoisted.cmd = "amenbo-dev";
    await render();
    expect(commands()).toContain("amenbo-dev init");
    expect(commands().some((c) => c.startsWith("amenbo-dev bind --project"))).toBe(true);
    expect(commands()).toContain("amenbo-dev agent --json");
    expect(commands().some((c) => c.startsWith("amenbo "))).toBe(false);
  });

  it("falls back to the production name when there is no build to ask", async () => {
    hoisted.cmd = null;
    await render();
    expect(commands()).toContain("amenbo init");
  });
});

describe("what the asking step hands over", () => {
  // Two wordings for the same move leave the reader checking which one is right, so the step shows
  // the request the first loop copies rather than an example of its own.
  it("is the request the first loop copies, word for word", async () => {
    await render();
    expect(commands()).toContain(t("firstloop.prompt"));
  });
});
