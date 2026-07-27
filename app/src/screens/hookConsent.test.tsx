// @vitest-environment jsdom
// The modal that asks whether amenbo may wire the lint hooks. Only the boundary is stubbed (core's
// `hook_offer` / `hook_answer`); the modal's own branching runs for real.
//
// What these guard is the three-valued answer, which is why this modal exists at all rather than a native
// confirm: "not now" must record nothing (the device stays unanswered and is asked again), while "never"
// must record a no. A native dialog cannot tell those apart, so a regression toward one would silently turn
// dismissing the modal into "never ask me again". And they guard the one-question shape: there is a
// single question for the device, not one per repository, and answering it once is the whole of it.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { HookOfferDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  /** The one question core says is due (`hooks::reconcile` already judged it), or null for none. */
  offer: null as HookOfferDto | null,
  /** Every answer that reached the store — the point being that a dismissal leaves this empty. There is
   *  never a repository in it: the answer is the device's. */
  answers: [] as { yes: boolean }[],
  /** How many times the offer was fetched: the environment is probed once, at startup. */
  fetches: 0,
  /** When set, the answer fails and nothing lands. */
  failWith: null as string | null,
  /** When set, the fetch itself fails — a surface that could not ask anything. */
  fetchFails: false,
  /** How many times the modal reported it has nothing left to ask (what lets the setup banner take its turn). */
  done: 0,
}));

vi.mock("../core/mutations", () => ({
  fetchHookOffer: () => {
    hoisted.fetches += 1;
    if (hoisted.fetchFails) return Promise.reject(new Error("cannot probe"));
    return Promise.resolve(hoisted.offer);
  },
  answerHookOffer: (yes: boolean) => {
    if (hoisted.failWith) return Promise.reject(new Error(hoisted.failWith));
    hoisted.answers.push({ yes });
    return Promise.resolve();
  },
}));

import { t } from "../core/i18n";
import { HookConsentModal } from "./HookConsentModal";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

function offer(over: Partial<HookOfferDto> = {}): HookOfferDto {
  return { cmd: "amenbo", ...over };
}

async function render() {
  await act(async () => {
    root.render(createElement(HookConsentModal, { onDone: () => (hoisted.done += 1) }));
  });
}

/** The modal's buttons, by their visible label. */
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

/** Esc, which is "not now" — the third answer, and the only way left to reach it once the button went. */
async function escape() {
  await act(async () => {
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
  });
}

beforeEach(() => {
  hoisted.offer = null;
  hoisted.answers = [];
  hoisted.fetches = 0;
  hoisted.failWith = null;
  hoisted.fetchFails = false;
  hoisted.done = 0;
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
  vi.restoreAllMocks();
});

describe("the lint hook consent modal", () => {
  it("asks nothing when core has nothing to ask", async () => {
    await render();
    expect(container.textContent).toBe("");
    expect(hoisted.fetches).toBe(1);
  });

  it("records a yes and closes — one answer, no repository", async () => {
    hoisted.offer = offer();
    await render();
    await click(t("hooks.yes"));
    expect(hoisted.answers).toEqual([{ yes: true }]);
    expect(container.textContent).toBe("");
  });

  it("records a no", async () => {
    hoisted.offer = offer();
    await render();
    await click(t("hooks.no"));
    expect(hoisted.answers).toEqual([{ yes: false }]);
  });

  it("records nothing when dismissed with Esc: putting it off is not a no", async () => {
    hoisted.offer = offer();
    await render();
    await escape();
    expect(hoisted.answers).toEqual([]); // Unanswered, so the next startup asks again.
    expect(container.textContent).toBe(""); // …and it is gone for this run.
  });

  it("says the answer is asked once and covers every repository", async () => {
    hoisted.offer = offer();
    await render();
    // The scope line is the promise the click makes good on, so it must be on screen before the click.
    expect(container.textContent).toContain(t("hooks.scope"));
  });

  it("words its hint with the command name this build answers to, never a spelled-in one", async () => {
    hoisted.offer = offer({ cmd: "amenbo-dev" });
    await render();
    expect(container.textContent).toContain("amenbo-dev hooks install");
  });

  // The point of the whole screen: it asks for permission and nothing else. Slots, strangers, shared hooks
  // directories, the lines to paste — and the list of repositories the answer covers — are core's business.
  // Putting any of them here would hand the user amenbo's problem, which is how this modal grew a button that
  // could not be pressed.
  it("puts no plumbing in front of the user — only the question", async () => {
    hoisted.offer = offer();
    await render();
    const text = container.textContent ?? "";
    for (const leak of ["pre-commit", "commit-msg", "|| exit 1", ".githooks", "core.hooksPath", "/w/"]) {
      expect(text).not.toContain(leak);
    }
    expect([...container.querySelectorAll("button")].every((b) => !b.disabled)).toBe(true);
  });

  it("keeps the question up when the answer failed, so the failure is never recorded as consent", async () => {
    hoisted.offer = offer();
    hoisted.failWith = "permission denied";
    await render();
    await click(t("hooks.yes"));
    expect(hoisted.answers).toEqual([]);
    expect(container.querySelector(".hookconsent__modal")).not.toBeNull(); // Still asking.
  });

  // Handing over to the setup banner, which reports what is still unwired. It must not speak while the question is
  // on screen, and it must read the disk only after an answer has had its chance to change it.
  describe("saying it is done asking", () => {
    it("does not say so while the question is still up", async () => {
      hoisted.offer = offer();
      await render();
      expect(hoisted.done).toBe(0);

      await escape();
      expect(hoisted.done).toBeGreaterThan(0);
    });

    it("says so when there was never anything to ask", async () => {
      await render();
      expect(hoisted.done).toBeGreaterThan(0);
    });

    // A surface that could not ask must not also mute the surface that only tells.
    it("says so when it could not even find out what to ask", async () => {
      hoisted.fetchFails = true;
      await render();
      expect(hoisted.done).toBeGreaterThan(0);
    });

    it("says so once the question is answered", async () => {
      hoisted.offer = offer();
      await render();
      await click(t("hooks.yes"));
      expect(hoisted.done).toBeGreaterThan(0);
    });

    // The failed answer left the question up, so the banner must keep waiting rather than warn behind the modal.
    it("does not say so when an answer failed and the question is still up", async () => {
      hoisted.offer = offer();
      hoisted.failWith = "permission denied";
      await render();
      await click(t("hooks.yes"));
      expect(hoisted.done).toBe(0);
    });
  });
});
