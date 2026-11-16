// @vitest-environment jsdom
// The update screen the setup-again band opens. Only the seam that talks to core is replaced — what this
// device holds, and what the press was called with. The screen's own rendering and branching are real.
//
// What these hold it to: the token is the only thing asked for and the only thing handed on (no account,
// no name — core keeps the keys and reads the name off the stored address), the token box is emptied once
// it is spent, and the screen says the data and the phones stay as they are.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const hoisted = vi.hoisted(() => ({
  /** Every setup, with every argument it was handed. */
  setUp: [] as unknown[][],
  /** What setup answers with — whether the keys were kept or drawn afresh. */
  keys: "kept" as "kept" | "generated",
  /** What setup throws, if anything. */
  refuse: null as string | null,
  /** How many times the band was told to ask core again. */
  rechecked: 0,
  /** Every link opened outside. */
  opened: [] as string[],
  closed: 0,
}));

const TOKEN_LINK = "https://dash.cloudflare.com/?to=%2F%3Aaccount%2Fapi-tokens";

vi.mock("../core/viewer", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/viewer")>();
  return {
    ...orig,
    useViewerState: () => ({
      state: { setUp: true, carrying: true, waiting: 0, serverBuild: 3, workerBuild: 4, tokenLink: TOKEN_LINK },
      loading: false,
      error: undefined,
    }),
    setUpViewer: (...args: unknown[]) => {
      hoisted.setUp.push(args);
      if (hoisted.refuse !== null) return Promise.reject(new Error(hoisted.refuse));
      return Promise.resolve({
        url: "https://amenbo-viewer.example.invalid",
        account: "acct",
        database: "amenbo-viewer",
        keys: hoisted.keys,
      });
    },
    recheckSetupAgain: () => {
      hoisted.rechecked += 1;
    },
  };
});

vi.mock("../core/mutations", () => ({
  openExternalUrl: (url: string) => {
    hoisted.opened.push(url);
    return Promise.resolve();
  },
}));

import { ViewerUpdate } from "./ViewerUpdate";
import { t, tf } from "../core/i18n";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

async function render() {
  await act(async () => {
    root.render(createElement(ViewerUpdate, { onClose: () => { hoisted.closed += 1; } }));
  });
}

const button = (label: string) =>
  Array.from(container.querySelectorAll("button")).find((b) => b.textContent === label);
const tokenBox = () => container.querySelector<HTMLInputElement>(`input[aria-label="${t("viewer.token")}"]`)!;
const type = (el: HTMLInputElement, value: string) => {
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
  setter.call(el, value);
  el.dispatchEvent(new Event("input", { bubbles: true }));
};

async function press(label: string) {
  await act(async () => {
    button(label)!.click();
  });
}

beforeEach(() => {
  hoisted.setUp = [];
  hoisted.keys = "kept";
  hoisted.refuse = null;
  hoisted.rechecked = 0;
  hoisted.opened = [];
  hoisted.closed = 0;
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("ViewerUpdate", () => {
  it("is a dialog asking for the API token alone, and says the data and phones stay", async () => {
    await render();
    const dialog = container.querySelector('[role="dialog"]');
    expect(dialog).not.toBeNull();
    expect(dialog!.getAttribute("aria-modal")).toBe("true");
    expect(container.querySelector("#viewerupdate-title")!.textContent).toBe(t("viewer.update.title"));
    expect(container.textContent).toContain(t("viewer.update.keeps"));
    expect(container.querySelectorAll("input")).toHaveLength(1);
    expect(tokenBox().type).toBe("password");
    expect(button(t("viewer.update.run"))!.disabled).toBe(true);
  });

  it("opens the page where the token is made", async () => {
    await render();
    await press(t("viewer.tokenLink"));
    expect(hoisted.opened).toEqual([TOKEN_LINK]);
  });

  it("hands core the token and nothing else, then empties the box", async () => {
    await render();
    await act(async () => {
      type(tokenBox(), "  secret-token  ");
    });
    await press(t("viewer.update.run"));
    expect(hoisted.setUp).toEqual([["secret-token"]]);
    expect(tokenBox().value).toBe("");
    expect(container.textContent).toContain(t("viewer.update.done"));
    expect(hoisted.rechecked).toBe(1);
  });

  it("says every phone has to pair again when core drew new keys", async () => {
    hoisted.keys = "generated";
    await render();
    await act(async () => {
      type(tokenBox(), "secret-token");
    });
    await press(t("viewer.update.run"));
    expect(container.textContent).toContain(t("viewer.stoodGenerated"));
    expect(container.textContent).not.toContain(t("viewer.update.done"));
  });

  it("shows core's refusal and points at Settings for the account", async () => {
    hoisted.refuse = "the token reaches 2 accounts";
    await render();
    await act(async () => {
      type(tokenBox(), "secret-token");
    });
    await press(t("viewer.update.run"));
    expect(container.querySelector('[role="alert"]')!.textContent).toContain("the token reaches 2 accounts");
    expect(container.textContent).toContain(tf("viewer.update.elsewhere", { recreate: t("viewer.recreate") }));
    expect(tokenBox().value).toBe("secret-token");
  });

  it("closes from its button and on Escape", async () => {
    await render();
    await press(t("viewer.update.close"));
    expect(hoisted.closed).toBe(1);
    await act(async () => {
      window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    });
    expect(hoisted.closed).toBe(2);
  });
});
