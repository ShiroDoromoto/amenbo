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

import type { SkinJudgementDto, SkinListDto, SkinTablesDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  list: { on: null, skins: [] } as SkinListDto,
  tables: {} as Record<string, SkinTablesDto>,
  /** Every `skin_use` the screen asked for, in order. */
  worn: [] as (string | null)[],
  /** What the host answers for a font's licence in full. */
  licence: null as string | null,
  /** Every `skin_write_out` the screen asked for, in order. */
  handedOn: [] as { name: string; path: string }[],
  /** The path the save panel answers with, or `null` for a reader who cancelled. */
  saveAs: null as string | null,
  /** The name the save panel was opened under. */
  suggested: null as string | null,
  /** What the file picker answers with, for the panel that takes a skin in. */
  picked: [] as string[],
  /** What reading that file over gives back, and the name taking it in answers with. */
  read: null as SkinJudgementDto | null,
}));

vi.mock("../core/dialog", () => ({
  pickFiles: () => Promise.resolve(hoisted.picked),
  pickSaveAs: (suggested: string) => {
    hoisted.suggested = suggested;
    return Promise.resolve(hoisted.saveAs);
  },
}));
vi.mock("../core/hostDrop", () => ({ watchHostDrop: () => Promise.resolve(() => {}) }));

vi.mock("../core/ipc", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => {
    if (cmd === "skin_list") return Promise.resolve(hoisted.list);
    if (cmd === "skin_font_licence") return Promise.resolve(hoisted.licence);
    if (cmd === "skin_tables") return Promise.resolve(hoisted.tables[args?.name as string] ?? null);
    if (cmd === "skin_use") {
      hoisted.worn.push((args?.name as string | null) ?? null);
      return Promise.resolve(undefined);
    }
    if (cmd === "skin_read") return Promise.resolve(hoisted.read);
    if (cmd === "skin_add") return Promise.resolve(hoisted.read?.name ?? "");
    if (cmd === "skin_write_out") {
      hoisted.handedOn.push({ name: args?.name as string, path: args?.path as string });
      return Promise.resolve(undefined);
    }
    return Promise.reject(new Error(`unmocked ${cmd}`));
  },
}));

import { applySnapshot, getSnapshot } from "../core/snapshot";
import { AppearanceSettings } from "./AppearanceSettings";

const row = (name: string, themes: string[]) => ({
  name,
  title: name,
  titles: {},
  author: null,
  version: null,
  themes,
  license: null,
  homepage: null,
  error: null,
  fontFamily: null,
  fontLicense: null,
  // One of the four that ship inside the build, which are kept in no file. A device's own is a
  // row with a filename on it, and that is what the write-out is offered for.
  fileName: null as string | null,
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
  hoisted.handedOn = [];
  hoisted.saveAs = null;
  hoisted.suggested = null;
  hoisted.picked = [];
  hoisted.read = null;
  document.head.innerHTML = "";
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
      titles: {},
      light: { "c-bg": "#faf7f0" },
      dark: { "c-bg": "#1a1713" },
      icons: {},
      font: null,
      backgrounds: {},
      stamp: "s1",
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
      titles: {},
      light: { "c-bg": "#faf7f0" },
      dark: {},
      icons: {},
      font: null,
      backgrounds: {},
      stamp: "s1",
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

describe("the name a skin goes by", () => {
  // The reader's language is the whole question here, so it is set rather than left to whatever the
  // snapshot came up as.
  const wasLanguage = getSnapshot().language;
  beforeEach(() => applySnapshot({ ...getSnapshot(), language: "ja" }));
  afterEach(() => applySnapshot({ ...getSnapshot(), language: wasLanguage }));

  const retro = { ...row("retro", ["light", "dark"]), title: "Retro", titles: { ja: "レトロゲーム" } };

  it("is the one written for the reader's language, in the list and on the frame", async () => {
    hoisted.list = { on: null, skins: [retro] };
    hoisted.tables.retro = { name: "retro", title: "Retro", titles: { ja: "レトロゲーム" }, light: {}, dark: {}, icons: {}, font: null, backgrounds: {}, stamp: "s1" };
    await draw();

    const option = host.querySelector<HTMLOptionElement>('option[value="retro"]');
    expect(option?.textContent).toBe("レトロゲーム");

    await pick(selects()[0]!, "retro");
    expect(host.querySelector(".skinfit__title")?.textContent).toBe("レトロゲーム");
  });

  it("is the one the author wrote where they wrote none for this language", async () => {
    hoisted.list = { on: null, skins: [{ ...row("washi", ["light", "dark"]), title: "Washi" }] };
    await draw();
    expect(host.querySelector<HTMLOptionElement>('option[value="washi"]')?.textContent).toBe("Washi");
  });

  it("names the skin the same way where a one-sided one pins the theme", async () => {
    hoisted.list = { on: "retro", skins: [{ ...retro, themes: ["dark"] }] };
    await draw();
    expect(host.textContent).toContain("レトロゲーム");
    expect(host.textContent, "and not the author's one name beside it").not.toContain("Retro");
  });
});

describe("handing a skin on", () => {
  /** The button that writes the shown skin's own file out, where the screen is offering one. */
  const handOn = () =>
    [...host.querySelectorAll("button")].find(
      (b) => b.textContent === "このスキンを渡す" || b.textContent === "Hand this one on",
    );

  it("offers the file under the name it is kept as, and hands those bytes on", async () => {
    hoisted.list = {
      on: "kozo",
      skins: [{ ...row("kozo", ["light"]), fileName: "kozo.zip" }],
    };
    hoisted.saveAs = "/tmp/somewhere/kozo.zip";
    await draw();

    await act(async () => handOn()!.click());
    await act(async () => {});
    expect(hoisted.suggested).toBe("kozo.zip");
    expect(hoisted.handedOn).toEqual([{ name: "kozo", path: "/tmp/somewhere/kozo.zip" }]);
  });

  it("writes nothing where the reader closed the panel", async () => {
    hoisted.list = {
      on: "kozo",
      skins: [{ ...row("kozo", ["light"]), fileName: "kozo.zip" }],
    };
    hoisted.saveAs = null;
    await draw();

    await act(async () => handOn()!.click());
    await act(async () => {});
    expect(hoisted.handedOn).toEqual([]);
  });

  it("is not offered for one that ships inside the build", async () => {
    // There is no file of a person's to hand on, and what would be written out is this build's
    // own values — which is what the template already writes.
    hoisted.list = { on: "washi", skins: [row("washi", ["light", "dark"])] };
    await draw();
    expect(handOn()).toBe(undefined);
  });
});

// Taking a skin in is how a skin is edited: an author moves a value, packs the file and hands it
// to their own machine under the name it already has, with the screen it is meant to change in
// front of them. The window wears what it read when it came up, so unless the file is read again
// here, the value they moved is on disk and nowhere else until the app is opened next — which
// reads exactly like a change that did not take.
describe("a file landing under the name of the skin that is on", () => {
  /** What the panel that takes a file in gives back about it. */
  const judged = (name: string): SkinJudgementDto => ({
    name,
    title: name,
    titles: {},
    author: null,
    version: "2",
    themes: ["light"],
    held: true,
    heldVersion: "1",
    warnings: [],
    short: [],
    unread: [],
    covered: [],
    measured: 35,
    font: null,
    carries: [],
  });

  /** Choose a file in the panel and press what takes it in. */
  const takeIn = async () => {
    await act(async () => host.querySelector<HTMLButtonElement>(".skinwell .btn")!.click());
    await act(async () => {});
    await act(async () => host.querySelector<HTMLButtonElement>(".skinfit__answer .btn")!.click());
    await act(async () => {});
  };

  /** What the one sheet a skin is worn through holds. */
  const sheet = () => {
    const el = document.getElementById("amenbo-skin") as HTMLStyleElement | null;
    return [...(el?.sheet?.cssRules ?? [])].map((r) => r.cssText).join("\n");
  };

  it("is worn again, so what the file changed is on the screen", async () => {
    hoisted.list = { on: "kozo", skins: [{ ...row("kozo", ["light"]), fileName: "kozo.zip" }] };
    hoisted.tables.kozo = {
      name: "kozo",
      title: "kozo",
      titles: {},
      light: { "c-bg": "#123456" },
      dark: {},
      icons: {},
      font: null,
      backgrounds: {},
      stamp: "s2",
    };
    hoisted.picked = ["/tmp/kozo.zip"];
    hoisted.read = judged("kozo");
    await draw();
    expect(sheet(), "nothing is worn through this screen before the file lands").toBe("");

    await takeIn();
    expect(sheet()).toContain("--c-bg: #123456");
  });

  it("is not worn where it landed under another name", async () => {
    hoisted.list = { on: "kozo", skins: [{ ...row("kozo", ["light"]), fileName: "kozo.zip" }] };
    hoisted.tables.washi = {
      name: "washi",
      title: "washi",
      titles: {},
      light: { "c-bg": "#654321" },
      dark: {},
      icons: {},
      font: null,
      backgrounds: {},
      stamp: "s1",
    };
    hoisted.picked = ["/tmp/washi.zip"];
    hoisted.read = judged("washi");
    await draw();

    await takeIn();
    // It is held now and it is not on. A skin taken in is not a skin put on, and the screen this
    // reader is looking at must not change under them because a file arrived.
    expect(sheet()).toBe("");
  });
});
