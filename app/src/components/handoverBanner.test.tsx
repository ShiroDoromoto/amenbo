// @vitest-environment jsdom
//
// The band that says where the plugins went, and what putting it away records.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { t } from "../core/i18n";
import type { HandoverDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  held: null as HandoverDto | null,
  asked: 0,
  told: 0,
  tellFails: false,
}));

vi.mock("../core/snapshot", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/snapshot")>();
  return { ...orig, inTauri: () => true, subscribe: () => () => {} };
});
vi.mock("../core/mutations", () => ({
  fetchHandoverNotice: () => {
    hoisted.asked += 1;
    return Promise.resolve(hoisted.held);
  },
  markHandoverTold: () => {
    hoisted.told += 1;
    return hoisted.tellFails ? Promise.reject(new Error("the store would not take it")) : Promise.resolve();
  },
}));

import { HandoverBanner } from "./HandoverBanner";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  hoisted.held = null;
  hoisted.asked = 0;
  hoisted.told = 0;
  hoisted.tellFails = false;
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(async () => {
  await act(async () => root.unmount());
  container.remove();
});

async function render() {
  await act(async () => {
    root.render(createElement(HandoverBanner));
  });
}

const banner = () => container.querySelector(".healthbanner");
const dismiss = () => container.querySelector<HTMLButtonElement>(".healthbanner__close");

describe("HandoverBanner", () => {
  /// A device that had every one of them reads all four lines, and the account is asked for once.
  it("says what was installed and what came across with it", async () => {
    hoisted.held = { plugins: ["slack", "viewer", "worktree"], targets: 2, projects: 3, viewer: true };
    await render();

    expect(hoisted.asked).toBe(1);
    const text = banner()?.textContent ?? "";
    expect(text).toContain(t("handover.title"));
    expect(text).toContain("slack, viewer, worktree");
    expect(text).toContain("2");
    expect(text).toContain("3");
    expect(text).toContain(t("handover.viewer"));
    expect(text).toContain(t("handover.worktree"));
  });

  /// Each line stands on what the account holds. A device that carried no connection is not told about a
  /// shelf with nothing on it, and one that never had the Viewer is not warned about a send it will not
  /// make.
  it("leaves out the lines the account does not carry", async () => {
    hoisted.held = { plugins: ["mail"], targets: 0, projects: 0, viewer: false };
    await render();

    const text = banner()?.textContent ?? "";
    expect(text).toContain(t("handover.title"));
    expect(text).not.toContain(t("handover.viewer"));
    expect(text).not.toContain(t("handover.worktree"));
    expect(text).not.toContain("connection");
  });

  /// Nothing owed is no band — and the window that has already said it owes nothing.
  it("puts nothing up when there is nothing to say", async () => {
    hoisted.held = null;
    await render();
    expect(banner()).toBeNull();
  });

  /// Putting it away is what records the turn, and the write is what the band waits on.
  it("records the turn when it is put away", async () => {
    hoisted.held = { plugins: ["worktree"], targets: 0, projects: 0, viewer: false };
    await render();

    await act(async () => dismiss()?.click());
    expect(hoisted.told).toBe(1);
    expect(banner()).toBeNull();
  });

  /// A write that never landed leaves the band up with the reason on it: one that vanished anyway would
  /// leave the reader believing they had been told something the store does not know they were.
  it("stays up with the reason when the write fails", async () => {
    hoisted.held = { plugins: ["worktree"], targets: 0, projects: 0, viewer: false };
    hoisted.tellFails = true;
    await render();

    await act(async () => dismiss()?.click());
    expect(hoisted.told).toBe(1);
    expect(banner()).not.toBeNull();
    expect(banner()?.textContent ?? "").toContain("the store would not take it");
  });
});
