// @vitest-environment jsdom
// The build screens' dialogs: the one that puts a step in inside an action (`AMB-T-5257`), the one
// that makes an action on the spot from an automation's picture (`AMB-D-956`), and the one that
// declares what a way out hands on.
//
// What these guard: **inside an action there is no library to pick from** (`AMB-D-949`) and the press
// goes through that picture's own door — the line it was opened from, or the action itself where
// there is no line yet (`AMB-T-5315`); **the two roads are named apart**, and only the one on a line
// draws where it goes (`AMB-T-5526`); **what the dialog took — a name and a prompt — is what is
// sent**; **nothing is sent until the dialog has what a step cannot be made without**; **an action
// made on the spot is asked a name and a library and nothing else**, under a small picture of where
// it goes and starting from the name the library was searched with, lands where it was asked for,
// and hands its id on so the screen can go and build it; and, for what a way out hands on, **the name
// starts on the way out's own and stops following once somebody writes their own** — but only where
// that way out hands on nothing yet — asked in a row inside the way out's card rather than a dialog.
// **No dialog closes from the backdrop or Escape** (`AMB-T-5363`) — only its buttons do, so what was
// typed is not thrown away by a stray press.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const hoisted = vi.hoisted(() => ({
  make: vi.fn((..._args: unknown[]) => Promise.resolve(21 as number | null)),
  insertInside: vi.fn(),
  add: vi.fn(),
  output: vi.fn(),
}));

vi.mock("../core/automations", () => ({
  makeAutomationAction: hoisted.make,
  insertAutomationActionStep: hoisted.insertInside,
  addAutomationStep: hoisted.add,
  addAutomationOutput: hoisted.output,
}));

import { t } from "../core/i18n";
import { AutomationActionMake } from "./AutomationActionMake";
import { AutomationOutputAdd } from "./AutomationOutputAdd";
import { AutomationStepAdd } from "./AutomationStepAdd";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let root: Root;
let host: HTMLDivElement;

/** The dialogs draw into `document.body`, so what is looked at is the page and not the host. */
const boxes = () => [...document.body.querySelectorAll<HTMLInputElement>("input")];
const selects = () => [...document.body.querySelectorAll<HTMLSelectElement>("select")];
const buttons = () => [...document.body.querySelectorAll<HTMLButtonElement>("button")];
const button = (label: string) => buttons().find((b) => b.textContent === label)!;

/** Type into a controlled box the way React hears it — its own value tracker has to be moved. */
async function typeInto(box: HTMLInputElement | HTMLTextAreaElement, value: string) {
  const proto = box instanceof HTMLTextAreaElement ? HTMLTextAreaElement : HTMLInputElement;
  await act(async () => {
    Object.getOwnPropertyDescriptor(proto.prototype, "value")!.set!.call(box, value);
    box.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

beforeEach(() => {
  host = document.createElement("div");
  document.body.appendChild(host);
  root = createRoot(host);
  hoisted.make.mockClear();
  hoisted.insertInside.mockReset();
  hoisted.add.mockReset();
  hoisted.output.mockReset();
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
});

describe("putting a step in inside an action", () => {
  async function open(into: { picture: "action"; edgeId: number } | { picture: "action"; actionId: number }) {
    await act(async () => {
      root.render(
        createElement(AutomationStepAdd, {
          into,
          where: "edgeId" in into ? { box: "書く", exit: "書けた", next: "見直す" } : null,
          onClose: () => undefined,
        }),
      );
    });
  }

  it("asks a name and a prompt, and nothing else", async () => {
    await open({ picture: "action", actionId: 4 });
    expect(boxes()).toHaveLength(1);
    expect(selects()).toHaveLength(0);
    expect(document.body.querySelectorAll("textarea")).toHaveLength(1);
  });

  it("says it puts one in, and draws where, only on a line", async () => {
    await open({ picture: "action", edgeId: 9 });
    expect(document.body.querySelector(".autodlg__title")?.textContent).toBe(t("auto.act.insertTitle"));
    expect([...document.body.querySelectorAll(".wheremark__box")].map((one) => one.textContent)).toEqual([
      "書く",
      "見直す",
    ]);
    await open({ picture: "action", actionId: 4 });
    expect(document.body.querySelector(".autodlg__title")?.textContent).toBe(t("auto.act.addTitle"));
    expect(document.body.querySelector(".wheremark")).toBeNull();
  });

  it("offers no library to pick from — an action places no actions", async () => {
    await open({ picture: "action", edgeId: 9 });
    expect(selects().some((one) => one.value === "" && one.options.length > 1)).toBe(false);
    expect(document.body.querySelector("textarea")).not.toBeNull();
  });

  it("sends a step to the line it was opened from", async () => {
    await open({ picture: "action", edgeId: 9 });
    await typeInto(boxes()[0]!, "直す");
    await typeInto(document.body.querySelector("textarea")!, "やる");
    await act(async () => button(t("auto.act.insertPut")).click());
    expect(hoisted.insertInside).toHaveBeenCalledWith(9, { name: "直す", prompt: "やる" });
  });

  it("adds the first step where the picture has no line to press", async () => {
    await open({ picture: "action", actionId: 4 });
    await typeInto(boxes()[0]!, "取る");
    await typeInto(document.body.querySelector("textarea")!, "やる");
    await act(async () => button(t("auto.act.addPut")).click());
    expect(hoisted.add).toHaveBeenCalledWith(4, { name: "取る", prompt: "やる" });
  });
});

describe("making an action on the spot", () => {
  const made = vi.fn();
  async function open(
    into: { edgeId: number } | { automationId: number } = { edgeId: 9 },
    name?: string,
  ) {
    made.mockClear();
    await act(async () => {
      root.render(
        createElement(AutomationActionMake, {
          into,
          name,
          where: { box: "Take a task", exit: "taken", next: "Build it" },
          projectId: 1,
          onMade: made,
          onClose: () => undefined,
        }),
      );
    });
  }

  it("asks a name and a library under where it goes, and nothing an action's steps hold", async () => {
    await open();
    expect(document.body.querySelector("textarea")).toBeNull();
    expect(boxes()).toHaveLength(1);
    expect(selects()).toHaveLength(0);
    const where = document.body.querySelector(".wheremark")!;
    expect([...where.querySelectorAll(".wheremark__box")].map((one) => one.textContent)).toEqual([
      "Take a task",
      "Build it",
    ]);
    expect(button(t("auto.actions.reachProject")).getAttribute("aria-pressed")).toBe("true");
    expect(button(t("auto.actions.reachGlobal")).getAttribute("aria-pressed")).toBe("false");
    expect(button(t("auto.make.go")).disabled).toBe(true);
  });

  it("starts from the name the library was searched with", async () => {
    await open({ edgeId: 9 }, "レビュ");
    expect(boxes()[0]!.value).toBe("レビュ");
    expect(button(t("auto.make.go")).disabled).toBe(false);
  });

  it("places it on the line it was opened from, and goes on to build it", async () => {
    await open({ edgeId: 9 });
    await typeInto(boxes()[0]!, "書く");
    await act(async () => button(t("auto.actions.reachGlobal")).click());
    await act(async () => button(t("auto.make.go")).click());
    expect(hoisted.make).toHaveBeenCalledWith({ edgeId: 9 }, "書く", "device");
    expect(made).toHaveBeenCalledWith(21);
  });

  it("places it on a picture with no line yet", async () => {
    await open({ automationId: 7 });
    await typeInto(boxes()[0]!, "取る");
    await act(async () => button(t("auto.make.go")).click());
    expect(hoisted.make).toHaveBeenCalledWith({ automationId: 7 }, "取る", "project");
  });

  it("draws a refusal and stays where it is", async () => {
    hoisted.make.mockRejectedValueOnce({ code: "invalid", message_en: "that line is gone" });
    await open();
    await typeInto(boxes()[0]!, "書く");
    await act(async () => button(t("auto.make.go")).click());
    expect(document.body.textContent).toContain("that line is gone");
    expect(made).not.toHaveBeenCalled();
  });
});

describe("declaring what a way out hands on", () => {
  async function open(outputs: { name: string; kind: "value"; required: boolean }[]) {
    await act(async () => {
      root.render(
        createElement(AutomationOutputAdd, {
          exit: { id: 3, name: "drafted", outputs },
          onClose: () => undefined,
        }),
      );
    });
  }

  it("starts the name on the way out's own where it hands on nothing yet", async () => {
    await open([]);
    expect(boxes()[0]!.value).toBe("drafted");
  });

  it("does not, where the way out already hands something on", async () => {
    await open([{ name: "draft", kind: "value", required: true }]);
    expect(boxes()[0]!.value).toBe("");
  });

  it("stops following the way out once somebody writes their own", async () => {
    await open([]);
    await typeInto(boxes()[0]!, "下書き");
    await act(async () => button(t("auto.kind.file")).click());
    expect(button(t("auto.decl.required")).getAttribute("aria-pressed")).toBe("true");
    await act(async () => button(t("auto.decl.required")).click());
    await act(async () => button(t("auto.out.add")).click());
    expect(hoisted.output).toHaveBeenCalledWith(3, {
      name: "下書き",
      kind: "file",
      required: false,
    });
  });

  // A row in the card rather than a dialog: nothing half written is lost by putting it away.
  it("is put away by Escape or its ×", async () => {
    const onClose = vi.fn();
    await act(async () => {
      root.render(createElement(AutomationOutputAdd, { exit: { id: 3, name: "drafted", outputs: [] }, onClose }));
    });
    expect(document.body.querySelector(".modal__overlay")).toBeNull();
    await act(async () => {
      boxes()[0]!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    });
    expect(onClose).toHaveBeenCalledOnce();
    await act(async () => document.body.querySelector<HTMLButtonElement>(".autoout__close")!.click());
    expect(onClose).toHaveBeenCalledTimes(2);
  });
});

describe("leaving either dialog", () => {
  const dialogs = {
    "the one that puts a step in": (onClose: () => void) =>
      createElement(AutomationStepAdd, {
        into: { picture: "action", edgeId: 9 },
        onClose,
      }),
    "the one that makes an action on the spot": (onClose: () => void) =>
      createElement(AutomationActionMake, {
        into: { edgeId: 9 },
        where: null,
        projectId: 1,
        onMade: () => undefined,
        onClose,
      }),
  };

  for (const [which, make] of Object.entries(dialogs)) {
    it(`closes ${which} only from its buttons, not from the backdrop or Escape`, async () => {
      const onClose = vi.fn();
      await act(async () => root.render(make(onClose)));
      const backdrop = document.querySelector<HTMLDivElement>(".modal__overlay")!;
      await act(async () => backdrop.click());
      await act(async () => {
        document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
      });
      expect(onClose).not.toHaveBeenCalled();
      await act(async () => button(t("auto.add.cancel")).click());
      expect(onClose).toHaveBeenCalledOnce();
    });
  }
});
