// @vitest-environment jsdom
// The machine's own confirmation dialog, as `confirmDialog` opens it (`AMB-T-5665`).
//
// What these guard: **one question on screen at a time** — a second call while the first is still
// open answers no without opening anything, because on macOS a second sheet stacked on the first
// leaves behind a dialog neither button closes; **the next question is asked** once the first is
// answered, whichever way; and **the buttons are named in the app's language**, since the dialog
// otherwise says OK and Cancel in English.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const hoisted = vi.hoisted(() => ({
  /** Each dialog opened: what it asked, what its buttons said, and the answer to give it. */
  opened: [] as { message: string; options: unknown; answer: (yes: boolean) => void }[],
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  confirm: (message: string, options: unknown) =>
    new Promise<boolean>((answer) => { hoisted.opened.push({ message, options, answer }); }),
}));

vi.mock("./i18n", () => ({ t: (key: string) => `<${key}>` }));

import { confirmDialog } from "./dialog";

/** Let the lazy import and the call behind it settle. */
const settle = () => new Promise((done) => setTimeout(done, 0));

beforeEach(() => {
  hoisted.opened = [];
  (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
});

afterEach(() => {
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
});

describe("confirmDialog", () => {
  it("names its buttons in the app's language", async () => {
    const asked = confirmDialog("Remove it?");
    await settle();
    expect(hoisted.opened.map((one) => [one.message, one.options])).toEqual([
      ["Remove it?", { okLabel: "<dialog.ok>", cancelLabel: "<dialog.cancel>" }],
    ]);
    hoisted.opened[0].answer(true);
    expect(await asked).toBe(true);
  });

  it("answers no to a second question while the first is open, and opens nothing for it", async () => {
    const first = confirmDialog("Close this pane?");
    await settle();
    const second = await confirmDialog("Close this pane?");

    expect(second, "the second question was let through").toBe(false);
    expect(hoisted.opened, "a second dialog was stacked on the first").toHaveLength(1);
    hoisted.opened[0].answer(true);
    expect(await first, "the first question lost its own answer").toBe(true);
  });

  it("asks the next question once the first is answered, whichever way", async () => {
    const first = confirmDialog("One?");
    await settle();
    hoisted.opened[0].answer(false);
    expect(await first).toBe(false);

    const next = confirmDialog("Two?");
    await settle();
    expect(hoisted.opened.map((one) => one.message), "the dialog stayed shut after a no").toEqual(["One?", "Two?"]);
    hoisted.opened[1].answer(true);
    expect(await next).toBe(true);
  });
});
