// @vitest-environment jsdom
// Settings > Notification targets — the device's shelf (`AMB-D-885`).
//
// What these hold the screen to is the three things the decision turned on: a kind is readable as a word
// and not only as a colour, a credential is never drawn or sent back unasked, and a delete says what it
// costs before it is made.
//
// Only the seam that talks to core is replaced — which rows exist, and what each write was called with.
// The screen's own rendering and branching are the real thing.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { NotifyTarget, NotifyTargetEdit } from "../core/notifyTargets";

const hoisted = vi.hoisted(() => ({
  targets: [] as import("../core/notifyTargets").NotifyTarget[],
  loading: false,
  error: undefined as unknown,
  /** Every row raised, in order. */
  added: [] as { kind: string; name: string }[],
  /** Every save, in order — the id it was aimed at included. */
  saved: [] as { id: number; edit: NotifyTargetEdit }[],
  /** Every target the mark was moved onto. */
  marked: [] as number[],
  /** Every target deleted. */
  deleted: [] as number[],
  /** The id a fresh row comes back with. */
  nextId: 7,
  /** What the confirmation was asked, and how it was answered. */
  asked: [] as string[],
  confirm: true,
  /** Every target a check was asked of, and what the check came back saying. */
  checked: [] as number[],
  reached: false,
  /** Every target a test message was sent through. */
  tested: [] as number[],
  /** What the next press is answered with, where a test wants a refusal. */
  refuse: null as string | null,
}));

vi.mock("../core/notifyTargets", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/notifyTargets")>();
  return {
    ...orig,
    useNotifyTargets: () => ({
      targets: hoisted.targets,
      loading: hoisted.loading,
      error: hoisted.error,
    }),
    addNotifyTarget: (kind: string, name: string) => {
      hoisted.added.push({ kind, name });
      return Promise.resolve({
        id: hoisted.nextId, kind, name, isDefault: false, secretSet: false, projectsUsing: 0,
      });
    },
    saveNotifyTarget: (id: number, edit: NotifyTargetEdit) => {
      hoisted.saved.push({ id, edit });
      return Promise.resolve(null);
    },
    setDefaultNotifyTarget: (id: number) => {
      hoisted.marked.push(id);
      return Promise.resolve();
    },
    deleteNotifyTarget: (id: number) => {
      hoisted.deleted.push(id);
      return Promise.resolve([]);
    },
    checkNotifyTarget: (id: number) => {
      hoisted.checked.push(id);
      if (hoisted.refuse !== null) return Promise.reject(new Error(hoisted.refuse));
      return Promise.resolve({ reached: hoisted.reached });
    },
    testNotifyTarget: (id: number) => {
      hoisted.tested.push(id);
      if (hoisted.refuse !== null) return Promise.reject(new Error(hoisted.refuse));
      return Promise.resolve();
    },
  };
});

vi.mock("../core/dialog", () => ({
  confirmDialog: (message: string) => {
    hoisted.asked.push(message);
    return Promise.resolve(hoisted.confirm);
  },
}));

import { NotifyTargetsSetting } from "./NotifyTargetsSetting";
import { t, tf } from "../core/i18n";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

/** One target on the shelf: a Slack row holding nothing, until a test says otherwise. */
const target = (over: Partial<NotifyTarget> & { id: number; name: string }): NotifyTarget => ({
  kind: "slack",
  isDefault: false,
  secretSet: false,
  projectsUsing: 0,
  ...over,
});

const button = (label: string) =>
  Array.from(container.querySelectorAll("button")).find((b) => b.textContent === label);
const shelfRows = () => Array.from(container.querySelectorAll(".shelf__row"));
const box = (label: string) =>
  container.querySelector<HTMLInputElement>(`input[aria-label="${label}"]`)!;
const type = (el: HTMLInputElement, value: string) => {
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
  setter.call(el, value);
  el.dispatchEvent(new Event("input", { bubbles: true }));
};
const select = (el: HTMLSelectElement, value: string) => {
  el.value = value;
  el.dispatchEvent(new Event("change", { bubbles: true }));
};

beforeEach(() => {
  hoisted.targets = [];
  hoisted.loading = false;
  hoisted.error = undefined;
  hoisted.added = [];
  hoisted.saved = [];
  hoisted.marked = [];
  hoisted.deleted = [];
  hoisted.nextId = 7;
  hoisted.asked = [];
  hoisted.confirm = true;
  hoisted.checked = [];
  hoisted.reached = false;
  hoisted.tested = [];
  hoisted.refuse = null;
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

const render = () => act(() => root.render(createElement(NotifyTargetsSetting)));
/** Open one row's form, by the position it sits at on the shelf. */
const edit = (i = 0) =>
  act(() => { shelfRows()[i].querySelector<HTMLButtonElement>("button")!.click(); });

describe("the shelf", () => {
  it("lists every target, and says each kind in a word as well as a colour", () => {
    hoisted.targets = [
      target({ id: 1, name: "dev team", isDefault: true }),
      target({ id: 2, name: "my mail", kind: "mail", smtpHost: "smtp.example.com", smtpPort: 587, smtpUser: "me@example.com" }),
    ];
    render();

    expect(shelfRows()).toHaveLength(2);
    expect(container.textContent).toContain("dev team");
    expect(container.textContent).toContain("my mail");
    // The word beside the tile: a colour alone is unreadable once there are four kinds.
    expect(container.textContent).toContain(t("notify.kind.slack"));
    expect(container.textContent).toContain(t("notify.kind.mail"));
    // The mark says where a new project starts, so exactly the row carrying it wears the chip.
    expect(Array.from(container.querySelectorAll(".chip")).map((c) => c.textContent))
      .toEqual([t("notify.default")]);
  });

  it("draws a mail target's connection and never a Slack one's URL", () => {
    hoisted.targets = [
      target({ id: 1, name: "dev team", secretSet: true }),
      target({ id: 2, name: "my mail", kind: "mail", smtpHost: "smtp.example.com", smtpPort: 587, smtpUser: "me@example.com" }),
    ];
    render();

    // The server and the account are not credentials, so they are the line under a mail target's name.
    expect(container.textContent).toContain("smtp.example.com:587 · me@example.com");
    // A webhook URL is, and it never leaves core — so the line says held, and says nothing else.
    expect(container.textContent).toContain(t("notify.webhookSet"));
  });

  it("says so when the shelf is empty rather than drawing nothing", () => {
    render();
    expect(shelfRows()).toHaveLength(0);
    expect(container.textContent).toContain(t("notify.empty"));
  });
});

describe("one target's connection", () => {
  it("saves a mail connection whole, and sends no credential where none was typed", async () => {
    hoisted.targets = [target({
      id: 2, name: "my mail", kind: "mail", secretSet: true,
      smtpHost: "smtp.example.com", smtpPort: 587, smtpUser: "me@example.com",
    })];
    render();
    edit();

    // The credential's box starts empty even though one is held: it cannot be read back.
    expect(box(t("notify.smtpPassword")).value).toBe("");
    act(() => { type(box(t("notify.smtpHost")), "smtp.other.com"); });
    await act(async () => { button(t("notify.save"))!.click(); });

    expect(hoisted.saved).toHaveLength(1);
    expect(hoisted.saved[0].id).toBe(2);
    expect(hoisted.saved[0].edit.smtpHost).toBe("smtp.other.com");
    expect(hoisted.saved[0].edit.smtpPort).toBe(587);
    // Nothing was typed into the masked box, so nothing is sent — which is what keeps what is held.
    expect(hoisted.saved[0].edit.secret).toBeUndefined();
  });

  it("sends the credential once it is typed", async () => {
    hoisted.targets = [target({ id: 1, name: "dev team" })];
    render();
    edit();

    act(() => { type(box(t("notify.webhook")), "https://hooks.example.com/T/B"); });
    await act(async () => { button(t("notify.save"))!.click(); });

    expect(hoisted.saved[0].edit.secret).toBe("https://hooks.example.com/T/B");
    // A Slack target has no SMTP connection at all, so the save carries none.
    expect(hoisted.saved[0].edit.smtpHost).toBeUndefined();
  });

  it("raises a new row first, then saves the connection onto the id that came back", async () => {
    render();
    select(container.querySelector<HTMLSelectElement>("select")!, "mail");
    act(() => {});

    act(() => { type(box(t("notify.name")), "work mail"); });
    await act(async () => { button(t("notify.save"))!.click(); });

    expect(hoisted.added).toEqual([{ kind: "mail", name: "work mail" }]);
    expect(hoisted.saved[0].id).toBe(7);
    expect(hoisted.saved[0].edit.name).toBe("work mail");
  });

  it("moves the default mark without touching what the standing projects chose", async () => {
    hoisted.targets = [target({ id: 1, name: "dev team" })];
    render();
    edit();

    await act(async () => { button(t("notify.makeDefault"))!.click(); });

    expect(hoisted.marked).toEqual([1]);
    expect(hoisted.saved).toHaveLength(0);
  });
});

describe("deleting a target", () => {
  it("says how many projects send through it before the press", () => {
    hoisted.targets = [target({ id: 1, name: "dev team", projectsUsing: 2 })];
    render();
    edit();

    expect(container.textContent).toContain(tf("notify.usedBy", { n: 2 }));
    expect(container.textContent).toContain(t("notify.deleteLoses"));
  });

  it("asks first, and a refusal deletes nothing", async () => {
    hoisted.targets = [target({ id: 1, name: "dev team" })];
    hoisted.confirm = false;
    render();
    edit();

    await act(async () => { button(t("notify.delete"))!.click(); });

    expect(hoisted.asked).toEqual([tf("notify.deleteConfirm", { name: "dev team" })]);
    expect(hoisted.deleted).toEqual([]);
  });

  it("deletes on a yes", async () => {
    hoisted.targets = [target({ id: 1, name: "dev team" })];
    render();
    edit();

    await act(async () => { button(t("notify.delete"))!.click(); });

    expect(hoisted.deleted).toEqual([1]);
  });
});

describe("asking whether the connection works", () => {
  /** The one line the last press left, whichever way it went. */
  const answer = () => container.querySelector(".donetext, .errortext")?.textContent ?? "";

  it("claims only what was asked: a relay was spoken to, a webhook's URL was read", async () => {
    hoisted.targets = [target({ id: 1, name: "dev team" })];
    render();
    edit();

    // A Slack webhook has no door but posting, so the check reads the shape and says so.
    await act(async () => { button(t("notify.check"))!.click(); });
    expect(hoisted.checked).toEqual([1]);
    expect(answer()).toBe(t("notify.checkShape"));

    // A mail relay is connected to and the account offered to it, so it may claim more.
    hoisted.reached = true;
    await act(async () => { button(t("notify.check"))!.click(); });
    expect(answer()).toBe(t("notify.checkReached"));
  });

  it("sends the test through the row it was opened on, and says where to look", async () => {
    hoisted.targets = [
      target({ id: 1, name: "dev team" }),
      target({ id: 2, name: "my mail", kind: "mail", smtpHost: "smtp.example.com" }),
    ];
    render();

    edit(0);
    await act(async () => { button(t("notify.test"))!.click(); });
    expect(hoisted.tested).toEqual([1]);
    expect(answer()).toBe(t("notify.testSentSlack"));

    // Where to look is not the same place, so the two kinds do not share a sentence.
    await act(async () => { button(t("notify.cancel"))!.click(); });
    edit(1);
    await act(async () => { button(t("notify.test"))!.click(); });
    expect(hoisted.tested).toEqual([1, 2]);
    expect(answer()).toBe(t("notify.testSentMail"));
  });

  it("says what was refused, and leaves nothing standing beside it", async () => {
    hoisted.targets = [target({ id: 1, name: "dev team" })];
    hoisted.refuse = "the webhook refused the message: 404 no_service";
    render();
    edit();

    await act(async () => { button(t("notify.test"))!.click(); });

    expect(answer()).toContain("no_service");
    // The refusal is the whole answer: a "sent" line standing beside it would be the screen saying
    // both at once.
    expect(container.querySelector(".donetext")).toBe(null);
  });

  // Both read the connection off a row, and a form that has not been saved yet has none.
  it("offers neither press before the row exists", () => {
    render();
    act(() => {
      select(container.querySelector<HTMLSelectElement>("select")!, "slack");
    });

    expect(button(t("notify.check"))).toBe(undefined);
    expect(button(t("notify.test"))).toBe(undefined);
  });
});

describe("the kind's own words", () => {
  // The screen names these keys at runtime (`notify.kind.${kind}`), which the source sweep in
  // `core/i18n/sourceKeys.test.ts` cannot read — so the family is held to English here instead.
  it("has a word for every kind the shelf can hold", () => {
    for (const kind of ["slack", "mail"]) {
      expect(t(`notify.kind.${kind}`)).not.toBe(`notify.kind.${kind}`);
    }
  });
});
