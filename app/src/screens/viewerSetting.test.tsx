// @vitest-environment jsdom
// Settings > Viewer — the phone that reads this store, and the server it reads from (`AMB-D-884`).
//
// What these hold the screen to is what the old plugin form left unsaid, and what it drew that it should
// not have: taking the read code away stops **every** phone and the screen says so before the press; how
// many are reading is not a number anybody has; new keys make everything already on the server
// unreadable; and the API token, the server's address and the encryption key are nowhere on the screen.
//
// Only the seam that talks to core is replaced — what this device holds, and what each press was called
// with. The screen's own rendering and branching are the real thing.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ViewerPairing, ViewerState } from "../core/viewer";

const hoisted = vi.hoisted(() => ({
  state: {} as import("../core/viewer").ViewerState,
  pairing: null as import("../core/viewer").ViewerPairing | null,
  pairingLoading: false,
  /** Every throw of the switch, in order. */
  carrying: [] as boolean[],
  /** Every setup, with what it was handed. */
  setUp: [] as { apiToken: string; account?: string }[],
  /** What setup answers with — whether the keys were kept or drawn afresh. */
  keys: "kept" as "kept" | "generated",
  /** How many read codes were issued. */
  issued: 0,
  /** How many times the code was taken away, and what the server was holding. */
  cutOff: 0,
  hadCode: true,
  /** Every send asked for by hand. */
  sent: 0,
  heldBack: undefined as undefined | "another_turn" | "switched_off",
  /** Every repair, with the consent it carried. */
  repaired: [] as boolean[],
  /** What the repair answers with. */
  outcome: "counted" as "not_set_up" | "level" | "counted" | "placed" | "sending_elsewhere",
  /** What the confirmation was asked, and how it was answered. */
  asked: [] as string[],
  confirm: true,
}));

vi.mock("../core/viewer", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/viewer")>();
  return {
    ...orig,
    useViewerState: () => ({ state: hoisted.state, loading: false, error: undefined }),
    useViewerPairing: () => ({
      pairing: hoisted.pairing,
      loading: hoisted.pairingLoading,
      error: undefined,
    }),
    useViewerApps: () => [{ phone: "iPhone", link: "https://example.invalid/ios" }],
    setViewerCarrying: (on: boolean) => {
      hoisted.carrying.push(on);
      return Promise.resolve();
    },
    setUpViewer: (apiToken: string, account?: string) => {
      hoisted.setUp.push({ apiToken, account });
      return Promise.resolve({
        url: "https://amenbo-viewer.example.invalid",
        account: "acct",
        database: "amenbo-viewer",
        keys: hoisted.keys,
      });
    },
    issueViewerCode: () => {
      hoisted.issued += 1;
      return Promise.resolve({ carried: "{\"v\":1}", issuedAt: "2026-08-30T00:00:00Z" });
    },
    cutOffViewer: () => {
      hoisted.cutOff += 1;
      return Promise.resolve(hoisted.hadCode);
    },
    sendToViewer: () => {
      hoisted.sent += 1;
      return Promise.resolve({ placed: 4, waiting: 2, heldBack: hoisted.heldBack });
    },
    repairViewer: (place: boolean) => {
      hoisted.repaired.push(place);
      return Promise.resolve({
        outcome: hoisted.outcome,
        drift: { toPlace: 4, toDrop: 1 },
        sent: { placed: 5, waiting: 0 },
      });
    },
  };
});

vi.mock("../core/dialog", () => ({
  confirmDialog: (message: string) => {
    hoisted.asked.push(message);
    return Promise.resolve(hoisted.confirm);
  },
}));

import { ViewerSetting } from "./ViewerSetting";
import { t, tf } from "../core/i18n";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

/** A device with a server standing and nothing waiting, until a test says otherwise. */
const device = (over: Partial<ViewerState> = {}): ViewerState => ({
  setUp: true,
  carrying: true,
  waiting: 0,
  serverBuild: 3,
  workerBuild: 3,
  tokenLink: "https://dash.cloudflare.com/?to=%2F%3Aaccount%2Fapi-tokens",
  ...over,
});

const reading = (over: Partial<ViewerPairing> = {}): ViewerPairing => ({ paired: true, ...over });

const button = (label: string) =>
  Array.from(container.querySelectorAll("button")).find((b) => b.textContent === label);
const box = (label: string) =>
  container.querySelector<HTMLInputElement>(`input[aria-label="${label}"]`)!;
const type = (el: HTMLInputElement, value: string) => {
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
  setter.call(el, value);
  el.dispatchEvent(new Event("input", { bubbles: true }));
};

beforeEach(() => {
  hoisted.state = device();
  hoisted.pairing = reading();
  hoisted.pairingLoading = false;
  hoisted.carrying = [];
  hoisted.setUp = [];
  hoisted.keys = "kept";
  hoisted.issued = 0;
  hoisted.cutOff = 0;
  hoisted.hadCode = true;
  hoisted.sent = 0;
  hoisted.heldBack = undefined;
  hoisted.repaired = [];
  hoisted.outcome = "counted";
  hoisted.asked = [];
  hoisted.confirm = true;
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

const render = () => act(() => root.render(createElement(ViewerSetting)));
const press = async (label: string) => {
  await act(async () => { button(label)!.click(); });
};

describe("what the screen says about this device", () => {
  /**
   * The three secrets setup leaves behind were read-only rows on the plugin's form. They are not drawn
   * here and nothing on this screen can reach them — what a reader is told is whether a server stands.
   */
  it("draws no address, no token and no key", () => {
    hoisted.state = device();
    render();

    const drawn = container.textContent ?? "";
    expect(drawn).not.toContain("workers.dev");
    expect(drawn).not.toContain("amenbo-viewer");
    expect(container.querySelector('input[type="password"]')).toBeNull();
  });

  /**
   * An empty queue cannot answer "when did this last get through": it is both "the phone is up to date"
   * and "nothing has gone out since Tuesday". So the date is said in its own words, and a device that
   * has never placed anything says that rather than showing a blank.
   */
  it("says that nothing has ever been sent, rather than leaving the date blank", () => {
    hoisted.state = device({ lastPlacedAt: undefined, waiting: 0 });
    render();

    expect(container.textContent).toContain(t("viewer.neverPlaced"));
  });

  /**
   * Whether a phone may read comes over the network, so the rest of the screen is drawn without it. Until
   * it lands the line says it is being asked — not "no phone may read", which is an answer nobody has yet.
   */
  it("does not claim nobody is reading while the server is still being asked", () => {
    hoisted.pairing = null;
    hoisted.pairingLoading = true;
    render();

    expect(container.textContent).toContain(t("viewer.pairingAsking"));
    expect(container.textContent).not.toContain(t("viewer.notPaired"));
  });

  /** There is one read code and the server never learns which phone offered it. */
  it("says a count is not something it can give", () => {
    render();
    expect(container.textContent).toContain(t("viewer.howMany"));
  });
});

describe("a device with no server", () => {
  /**
   * Nothing is failing — setup has simply never run — so the screen draws the state it is in and offers
   * the press that changes it. The things that want a server are not drawn at all.
   */
  it("offers the press that stands one up, and nothing that needs one", () => {
    hoisted.state = device({ setUp: false, serverBuild: 0 });
    render();

    expect(button(t("viewer.createServer"))).toBeTruthy();
    expect(button(t("viewer.showQr"))).toBeUndefined();
    expect(button(t("viewer.cutOff"))).toBeUndefined();
    expect(container.textContent).toContain(t("viewer.noServer"));
  });
});

describe("standing the server up", () => {
  /** The token is typed and spent. An empty account box is not an account — it is left unsaid. */
  it("hands core the token, and no account where none was typed", async () => {
    render();
    await press(t("viewer.recreate"));
    type(box(t("viewer.token")), "  cf-token  ");
    await press(t("viewer.create"));

    expect(hoisted.setUp).toEqual([{ apiToken: "cf-token", account: undefined }]);
  });

  /** A token that reaches more than one account is one core will not choose for them. */
  it("carries the account that was typed", async () => {
    render();
    await press(t("viewer.recreate"));
    type(box(t("viewer.token")), "cf-token");
    type(box(t("viewer.account")), "acct-1");
    await press(t("viewer.create"));

    expect(hoisted.setUp).toEqual([{ apiToken: "cf-token", account: "acct-1" }]);
  });

  /**
   * A key drawn now opens nothing already on the server. That is the whole of what the reader has to act
   * on afterwards, so which of the two happened is said in its own sentence rather than as "done".
   */
  it("says whether every phone has to be paired again", async () => {
    hoisted.keys = "generated";
    render();
    await press(t("viewer.recreate"));
    type(box(t("viewer.token")), "cf-token");
    await press(t("viewer.create"));

    expect(container.textContent).toContain(t("viewer.stoodGenerated"));
    // The box is cleared on the way out: the token is spent, and the form stays on screen.
    expect(box(t("viewer.token")).value).toBe("");
  });
});

describe("the read code", () => {
  /** Issuing replaces whatever the server held, so the phone that had the one before stops reading. */
  it("says what showing a new code costs, beside the code", async () => {
    render();
    await press(t("viewer.showQr"));

    expect(hoisted.issued).toBe(1);
    expect(container.querySelector(".qrcode")).toBeTruthy();
    expect(container.textContent).toContain(t("viewer.codeReplaced"));
    expect(container.textContent).toContain(t("viewer.codeCarriesKey"));
  });

  /**
   * There is one code, so this takes every phone off at once — and the confirmation says so in those
   * words rather than asking about "this phone".
   */
  it("asks before taking it away, in the words that say every phone", async () => {
    render();
    await press(t("viewer.cutOff"));

    expect(hoisted.asked).toEqual([t("viewer.cutOffConfirm")]);
    expect(hoisted.cutOff).toBe(1);
    expect(container.textContent).toContain(t("viewer.cutOffDone"));
  });

  it("does nothing when the confirmation is answered no", async () => {
    hoisted.confirm = false;
    render();
    await press(t("viewer.cutOff"));

    expect(hoisted.cutOff).toBe(0);
  });

  /** Asking for the state the server is already in is answered, not refused. */
  it("says so where the server was holding no code", async () => {
    hoisted.hadCode = false;
    render();
    await press(t("viewer.cutOff"));

    expect(container.textContent).toContain(t("viewer.cutOffNothing"));
  });
});

describe("when it is not keeping up", () => {
  /**
   * The first press counts the difference and the second spends it. A screen that placed on the first
   * press would be sending on a number the reader never saw.
   */
  it("counts before it sends, and sends on the press after", async () => {
    render();
    await press(t("viewer.repair"));

    expect(hoisted.repaired).toEqual([false]);
    expect(container.textContent).toContain(tf("viewer.repairCounted", { place: 4, drop: 1 }));

    hoisted.outcome = "placed";
    await press(t("viewer.repairAgain"));
    expect(hoisted.repaired).toEqual([false, true]);
    expect(container.textContent).toContain(tf("viewer.repairPlaced", { placed: 5, waiting: 0 }));
  });

  /** Two ends that are level are not a difference waiting to be spent, so the next press counts again. */
  it("goes back to counting where the two ends are level", async () => {
    hoisted.outcome = "level";
    render();
    await press(t("viewer.repair"));

    expect(container.textContent).toContain(t("viewer.repairLevel"));
    expect(button(t("viewer.repair"))).toBeTruthy();
  });

  /** Neither reason a turn does nothing is a failure, and each is said in its own words. */
  it("says why a turn that did nothing did nothing", async () => {
    hoisted.heldBack = "switched_off";
    render();
    await press(t("viewer.send"));

    expect(hoisted.sent).toBe(1);
    expect(container.textContent).toContain(t("viewer.sentSwitchedOff"));
  });
});

describe("the switch", () => {
  /** Off, neither the reading nor the placing happens — and what is queued keeps, which the note says. */
  it("throws it, and says what off costs", async () => {
    render();
    const select = container.querySelector<HTMLSelectElement>(`select[aria-label="${t("viewer.sync")}"]`)!;
    select.value = "off";
    await act(async () => { select.dispatchEvent(new Event("change", { bubbles: true })); });

    expect(hoisted.carrying).toEqual([false]);
    expect(container.textContent).toContain(t("viewer.syncNote"));
  });
});
