// @vitest-environment jsdom
// The one nudge Amenbo puts today: the offer to start at login. Only the boundary is stubbed (core's
// `config_set_autostart`, and which build this is); the modal's own branching runs for real.
//
// What these guard is who is asked and what an answer costs. A build with no login registration to
// write must never put the question (`AMB-D-547`), and neither must one where the setting is already
// on. A yes is the setting being turned on through the one door that writes the OS registration too, so
// a rejected write has to leave the question up rather than close on a login nothing registered. A no
// writes nothing at all — off is what the setting already says.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const hoisted = vi.hoisted(() => ({
  /** What the snapshot says the setting is now. */
  autostart: false,
  /** The dev badge: a string on a development build, null on a shipped one. */
  badge: null as string | null,
  /** Every value written to the setting — empty is the point of the "no" case. */
  written: [] as boolean[],
  /** When set, the write fails and nothing lands. */
  failWith: null as string | null,
  /** How many times the modal asked to be taken off the screen. */
  closed: 0,
}));

vi.mock("../core/mutations", () => ({
  setAutostart: (enabled: boolean) => {
    if (hoisted.failWith) return Promise.reject(new Error(hoisted.failWith));
    hoisted.written.push(enabled);
    return Promise.resolve();
  },
  fetchDevBadge: () => Promise.resolve(hoisted.badge),
}));

vi.mock("../core/snapshot", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../core/snapshot")>()),
  getSnapshot: () => ({ autostart: hoisted.autostart }),
}));

import { t } from "../core/i18n";
import { AutostartNudge, autostartOfferable } from "./AutostartNudge";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

async function render() {
  await act(async () => {
    root.render(createElement(AutostartNudge, { onClose: () => { hoisted.closed += 1; } }));
  });
}

function button(label: string): HTMLButtonElement {
  const found = [...container.querySelectorAll("button")].find((b) => b.textContent?.includes(label));
  if (!found) throw new Error(`no button labelled ${label}: ${container.textContent}`);
  return found as HTMLButtonElement;
}

async function click(label: string) {
  await act(async () => {
    button(label).click();
  });
}

beforeEach(() => {
  hoisted.autostart = false;
  hoisted.badge = null;
  hoisted.written = [];
  hoisted.failWith = null;
  hoisted.closed = 0;
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
  vi.restoreAllMocks();
});

describe("the stage the autostart nudge is held behind", () => {
  it("is open on a shipped build where the setting is still off", async () => {
    expect(await autostartOfferable()).toBe(true);
  });

  it("is closed on a development build, which registers nothing at login", async () => {
    hoisted.badge = "DEV";
    expect(await autostartOfferable()).toBe(false);
  });

  it("is closed once the setting is on — there is nothing left to offer", async () => {
    hoisted.autostart = true;
    expect(await autostartOfferable()).toBe(false);
  });
});

describe("the autostart nudge", () => {
  it("turns the setting on and closes when the offer is taken", async () => {
    await render();
    await click(t("nudge.autostart.yes"));
    expect(hoisted.written).toEqual([true]);
    expect(hoisted.closed).toBe(1);
  });

  it("writes nothing when it is declined, and goes away all the same", async () => {
    await render();
    await click(t("nudge.autostart.no"));
    expect(hoisted.written).toEqual([]);
    expect(hoisted.closed).toBe(1);
  });

  it("stays up with the error when the login registration could not be written", async () => {
    hoisted.failWith = "could not write the login registration";
    await render();
    await click(t("nudge.autostart.yes"));
    expect(hoisted.closed).toBe(0);
    expect(container.textContent).toContain("could not write the login registration");
    // Still answerable: the button is live again, so a second try is not a reload away.
    expect(button(t("nudge.autostart.yes")).disabled).toBe(false);
  });

  it("says where the answer can be changed, since that is what makes a no cheap", async () => {
    await render();
    expect(container.textContent).toContain(t("nudge.autostart.hint"));
  });
});
