// @vitest-environment jsdom
// The two answers that decide what the application looks like, and the one rule that joins them.
//
// What is asserted is the part a reader cannot get themselves out of. A set of colours is put on the
// frame and on nothing else, so the screen that would be used to choose another keeps the colours it
// had. And a skin written for one side leaves no theme to choose, so the row says so and carries the
// way out rather than simply refusing.
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { SkinListDto, SkinTablesDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  list: { on: null, skins: [] } as SkinListDto,
  tables: {} as Record<string, SkinTablesDto>,
  /** Every `skin_use` the screen asked for, in order. */
  worn: [] as (string | null)[],
  /** What the host answers for a font's licence in full. */
  licence: null as string | null,
}));

vi.mock("../core/ipc", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => {
    if (cmd === "skin_list") return Promise.resolve(hoisted.list);
    if (cmd === "skin_font_licence") return Promise.resolve(hoisted.licence);
    if (cmd === "skin_tables") return Promise.resolve(hoisted.tables[args?.name as string] ?? null);
    if (cmd === "skin_use") {
      hoisted.worn.push((args?.name as string | null) ?? null);
      return Promise.resolve(undefined);
    }
    return Promise.reject(new Error(`unmocked ${cmd}`));
  },
}));

import { AppearanceSettings } from "./AppearanceSettings";

const row = (name: string, themes: string[]) => ({
  name,
  title: name,
  author: null,
  version: null,
  themes,
  license: null,
  homepage: null,
  error: null,
  fontFamily: null,
  fontLicense: null,
});

let host: HTMLDivElement;
let root: Root;

/** Draw the settings and let the reads that follow the first paint land. */
async function draw() {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  await act(async () => {
    root.render(<AppearanceSettings />);
  });
  await act(async () => {});
}

const selects = () => [...host.querySelectorAll("select")];
const pick = async (el: HTMLSelectElement, value: string) => {
  await act(async () => {
    el.value = value;
    el.dispatchEvent(new Event("change", { bubbles: true }));
  });
  await act(async () => {});
};

beforeEach(() => {
  hoisted.list = { on: null, skins: [] };
  hoisted.tables = {};
  hoisted.worn = [];
  hoisted.licence = null;
  document.documentElement.dataset.theme = "light";
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
});

describe("trying a skin on", () => {
  it("puts the values on the frame and on nothing else", async () => {
    hoisted.list = { on: null, skins: [{ ...row("washi", ["light", "dark"]), title: "和紙" }] };
    hoisted.tables.washi = {
      name: "washi",
      title: "和紙",
      light: { "c-bg": "#faf7f0" },
      dark: { "c-bg": "#1a1713" },
      font: null,
    };
    await draw();

    await pick(selects()[0]!, "washi");

    const frame = host.querySelector<HTMLElement>(".skinfit");
    expect(frame, "the fitting frame is up").not.toBe(null);
    expect(frame!.querySelector(".skinfit__title")?.textContent, "the author's own word for it")
      .toBe("和紙");
    expect(frame!.style.getPropertyValue("--c-bg")).toBe("#faf7f0");
    expect(document.documentElement.style.getPropertyValue("--c-bg")).toBe("");
    expect(hoisted.worn, "nothing is worn until it is chosen").toEqual([]);
  });

  it("wears it when it is chosen, and puts the frame away", async () => {
    hoisted.list = { on: null, skins: [row("washi", ["light", "dark"])] };
    hoisted.tables.washi = {
      name: "washi",
      title: "washi",
      light: { "c-bg": "#faf7f0" },
      dark: {},
      font: null,
    };
    await draw();
    await pick(selects()[0]!, "washi");

    // The first of the two presses that end a fitting, which sit under the frame rather than in it.
    const press = host.querySelector<HTMLButtonElement>(".skinfit__answer .btn");
    await act(async () => press!.click());
    await act(async () => {});

    expect(hoisted.worn).toEqual(["washi"]);
    expect(host.querySelector(".skinfit")).toBe(null);
  });
});

describe("the font a skin carries", () => {
  it("says where it came from, and hands over the licence when it is asked for", async () => {
    hoisted.list = {
      on: "retro",
      skins: [{ ...row("retro", ["dark"]), fontFamily: "Silkscreen", fontLicense: "OFL-1.1" }],
    };
    hoisted.licence = "Copyright 2001 The Silkscreen Project Authors";
    await draw();

    expect(host.textContent).toContain("Silkscreen");
    expect(host.textContent).toContain("OFL-1.1");
    expect(host.querySelector(".skinlicence"), "not until it is asked for").toBe(null);

    const ask = [...host.querySelectorAll<HTMLButtonElement>("button")]
      .find((b) => b.textContent === "ライセンス全文" || b.textContent === "Licence in full");
    await act(async () => ask!.click());
    await act(async () => {});
    expect(host.querySelector(".skinlicence")?.textContent).toContain("Silkscreen Project Authors");
  });

  it("says nothing where the skin carries none", async () => {
    hoisted.list = { on: "washi", skins: [row("washi", ["light", "dark"])] };
    await draw();
    expect(host.querySelector(".skinlicence")).toBe(null);
  });
});

describe("a skin made for one side only", () => {
  it("pins the theme row to that side and carries the way out", async () => {
    hoisted.list = { on: "retro", skins: [row("retro", ["dark"])] };
    await draw();

    const theme = selects()[1]!;
    expect(theme.disabled, "there is no theme to choose while it is on").toBe(true);
    expect(theme.value).toBe("dark");
    // What the window is actually drawn in is `applySkin`'s, not this screen's (core/skin.test.ts):
    // a skin is worn at startup and in every window, and only one of those has this screen open.

    const out = host.querySelector<HTMLButtonElement>(".skinesc");
    expect(out, "and the way out is beside the reason").not.toBe(null);
    await act(async () => out!.click());
    await act(async () => {});
    expect(hoisted.worn).toEqual([null]);
  });

  it("leaves the row answerable when the skin has both sides", async () => {
    hoisted.list = { on: "washi", skins: [row("washi", ["light", "dark"])] };
    await draw();
    expect(selects()[1]!.disabled).toBe(false);
    expect(host.querySelector(".skinesc")).toBe(null);
  });
});
