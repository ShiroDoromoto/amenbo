// @vitest-environment jsdom
// The modal that asks whether amenbo may write the lint hooks. Only the boundary is stubbed (core's
// `hook_offers` / `hook_answer`); the modal's own branching runs for real.
//
// What these guard is the three-valued answer, which is why this modal exists at all rather than a native
// confirm: "not now" must record nothing (the project stays unanswered and is asked again), while "never"
// must record a no. A native dialog cannot tell those apart, so a regression toward one would silently turn
// dismissing the modal into "never ask me again".
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { HookOfferDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  /** What core says is worth asking about (`hooks::reconcile` already judged this). */
  offers: [] as HookOfferDto[],
  /** Every answer that reached the store — the point being that a dismissal leaves this empty. */
  answers: [] as { projectId: number; dir: string; yes: boolean }[],
  /** How many times the offers were fetched: the environment is probed once, at startup. */
  fetches: 0,
  /** When set, the install fails and the answer never lands. */
  failWith: null as string | null,
  /** When set, the fetch itself fails — a surface that could not ask anything. */
  fetchFails: false,
  /** How many times the modal reported it has nothing left to ask (what lets the setup banner take its turn). */
  done: 0,
}));

vi.mock("../core/mutations", () => ({
  fetchHookOffers: () => {
    hoisted.fetches += 1;
    if (hoisted.fetchFails) return Promise.reject(new Error("cannot probe"));
    return Promise.resolve(hoisted.offers);
  },
  answerHookOffer: (projectId: number, dir: string, yes: boolean) => {
    if (hoisted.failWith) return Promise.reject(new Error(hoisted.failWith));
    hoisted.answers.push({ projectId, dir, yes });
    return Promise.resolve();
  },
}));

import { HookConsentModal } from "./HookConsentModal";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

function offer(over: Partial<HookOfferDto> = {}): HookOfferDto {
  return {
    projectId: 3,
    projectName: "amenbo",
    dir: "/w/amenbo",
    cmd: "amenbo",
    unwired: ["pre-commit", "commit-msg"],
    foreign: [],
    guidance: [],
    ...over,
  };
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

beforeEach(() => {
  hoisted.offers = [];
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

  it("records a yes and moves on", async () => {
    hoisted.offers = [offer()];
    await render();
    await click("設置する");
    expect(hoisted.answers).toEqual([{ projectId: 3, dir: "/w/amenbo", yes: true }]);
    expect(container.textContent).toBe("");
  });

  it("records a no when the user says never ask again", async () => {
    hoisted.offers = [offer()];
    await render();
    await click("二度と聞かない");
    expect(hoisted.answers).toEqual([{ projectId: 3, dir: "/w/amenbo", yes: false }]);
  });

  it("records nothing when dismissed: not now is not a no", async () => {
    hoisted.offers = [offer()];
    await render();
    await click("今はしない");
    expect(hoisted.answers).toEqual([]); // Unanswered, so the next startup asks again.
    expect(container.textContent).toBe(""); // …and it is gone for this run.
  });

  it("asks one repository at a time", async () => {
    hoisted.offers = [offer(), offer({ projectId: 4, projectName: "別PJ", dir: "/w/other" })];
    await render();
    expect(container.textContent).toContain("/w/amenbo");
    expect(container.textContent).not.toContain("/w/other");
    await click("今はしない");
    expect(container.textContent).toContain("/w/other"); // The second question waits its turn.
  });

  it("words itself with the command name this build answers to, never a spelled-in one", async () => {
    hoisted.offers = [offer({ cmd: "amenbo-dev" })];
    await render();
    expect(container.textContent).toContain("amenbo-dev lint");
    expect(container.textContent).toContain("amenbo-dev hooks install");
  });

  it("shows the line to add by hand for a slot amenbo will not write", async () => {
    hoisted.offers = [offer({ unwired: ["commit-msg"], foreign: ["pre-commit"], guidance: ["amenbo lint || exit 1"] })];
    await render();
    expect(container.textContent).toContain("amenbo lint || exit 1");
  });

  it("offers nothing to install when every slot is a stranger's, and still takes an answer", async () => {
    hoisted.offers = [offer({ unwired: [], foreign: ["pre-commit"], guidance: ["amenbo lint || exit 1"] })];
    await render();
    expect(button("設置する").disabled).toBe(true); // There is nothing amenbo may write.
    await click("二度と聞かない"); // Answering still settles it — that is why it is asked at all.
    expect(hoisted.answers).toEqual([{ projectId: 3, dir: "/w/amenbo", yes: false }]);
  });

  it("keeps the question up when the install failed, so the failure is never recorded as consent", async () => {
    hoisted.offers = [offer()];
    hoisted.failWith = "permission denied";
    await render();
    await click("設置する");
    expect(hoisted.answers).toEqual([]);
    expect(container.textContent).toContain("/w/amenbo"); // Still asking.
  });

  // Handing over to the setup banner, which reports what is still unwired. It must not speak while a question about
  // the same repository is on screen, and it must read the disk only after an answer has had its chance to change it.
  describe("saying it is done asking", () => {
    it("does not say so while a question is still up", async () => {
      hoisted.offers = [offer(), offer({ dir: "/w/案件Y" })];
      await render();
      expect(hoisted.done).toBe(0);

      await click("今はしない"); // One left, so still asking.
      expect(hoisted.done).toBe(0);

      await click("今はしない");
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

    it("says so once the last question is answered", async () => {
      hoisted.offers = [offer()];
      await render();
      await click("設置する");
      expect(hoisted.done).toBeGreaterThan(0);
    });

    // The failed install left the question up, so the banner must keep waiting rather than warn behind the modal.
    it("does not say so when an answer failed and the question is still up", async () => {
      hoisted.offers = [offer()];
      hoisted.failWith = "permission denied";
      await render();
      await click("設置する");
      expect(hoisted.done).toBe(0);
    });
  });
});
