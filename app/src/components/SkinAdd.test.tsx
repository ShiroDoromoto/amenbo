// @vitest-environment jsdom
// Reading a skin file over before anything is written.
//
// What is asserted is the part the decision turns on: a pairing under its floor is shown with its
// number and the press to take the file in is still there, because what refusing would stop is an
// author trying their own work in progress. A file the check turns away never reaches that — the
// read fails, and what is shown is the sentence, with nothing to decide.
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { SkinJudgementDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  picked: [] as string[],
  read: null as SkinJudgementDto | null,
  readFails: null as string | null,
  /** Every `skin_add` asked for: the path, and whether it was told to replace. */
  added: [] as { path: string; replace: boolean }[],
}));

vi.mock("../core/dialog", () => ({ pickFiles: () => Promise.resolve(hoisted.picked) }));
vi.mock("../core/hostDrop", () => ({ watchHostDrop: () => Promise.resolve(() => {}) }));
vi.mock("../core/ipc", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => {
    if (cmd === "skin_read") {
      return hoisted.readFails
        ? Promise.reject(new Error(hoisted.readFails))
        : Promise.resolve(hoisted.read);
    }
    if (cmd === "skin_add") {
      hoisted.added.push({ path: args?.path as string, replace: args?.replace as boolean });
      return Promise.resolve("washi");
    }
    return Promise.reject(new Error(`unmocked ${cmd}`));
  },
}));

import { SkinAdd } from "./SkinAdd";

const judgement = (over: Partial<SkinJudgementDto> = {}): SkinJudgementDto => ({
  name: "washi",
  title: "和紙",
  author: null,
  version: "1.2.0",
  themes: ["light", "dark"],
  held: false,
  heldVersion: null,
  warnings: [],
  short: [],
  unread: [],
  measured: 35,
  ...over,
});

let host: HTMLDivElement;
let root: Root;
let added = 0;

async function drawAndPick() {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  await act(async () => {
    root.render(<SkinAdd onAdded={() => { added += 1; }} />);
  });
  await act(async () => {
    host.querySelector<HTMLButtonElement>(".skinwell .btn")!.click();
  });
  await act(async () => {});
}

beforeEach(() => {
  hoisted.picked = ["/tmp/washi.yaml"];
  hoisted.read = judgement();
  hoisted.readFails = null;
  hoisted.added = [];
  added = 0;
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
});

describe("reading a skin file over", () => {
  it("shows what fell short with its number, and still offers to take it in", async () => {
    hoisted.read = judgement({
      short: [{ theme: "light", ink: "c-on-accent", ground: "c-accent", ratio: 1.2, floor: 4.5 }],
    });
    await drawAndPick();

    const said = host.textContent ?? "";
    expect(said).toContain("c-on-accent");
    expect(said).toContain("1.20");
    expect(said).toContain("4.5");

    const take = host.querySelector<HTMLButtonElement>(".skinfit__answer .btn");
    await act(async () => take!.click());
    await act(async () => {});
    expect(hoisted.added).toEqual([{ path: "/tmp/washi.yaml", replace: false }]);
    expect(added, "and the list it lands in is read again").toBe(1);
  });

  it("names each thing the check set aside", async () => {
    hoisted.read = judgement({
      warnings: [
        { kind: "unknown", theme: null, key: "radius_scale" },
        { kind: "closed", theme: "light", key: "k-slack" },
        { kind: "notText", theme: "light", key: "s-3" },
      ],
    });
    await drawAndPick();
    const said = host.textContent ?? "";
    expect(said).toContain("radius_scale");
    expect(said).toContain("light.k-slack");
    expect(said).toContain("light.s-3");
  });

  it("says a skin made for one side is made for one side", async () => {
    hoisted.read = judgement({ themes: ["dark"] });
    await drawAndPick();
    expect(host.querySelector(".chip--heed")?.textContent).toBeTruthy();
  });

  it("puts both versions side by side when the name is already held, and says to replace", async () => {
    hoisted.read = judgement({ held: true, heldVersion: "1.0.0", version: "2.0.0" });
    await drawAndPick();
    const said = host.textContent ?? "";
    expect(said).toContain("1.0.0");
    expect(said).toContain("2.0.0");

    await act(async () => host.querySelector<HTMLButtonElement>(".skinfit__answer .btn")!.click());
    await act(async () => {});
    expect(hoisted.added).toEqual([{ path: "/tmp/washi.yaml", replace: true }]);
  });

  it("a file the check turns away is a sentence and nothing to decide", async () => {
    hoisted.readFails = "this skin is written for skin_v 9; this Amenbo reads 1";
    await drawAndPick();
    expect(host.textContent).toContain("skin_v 9");
    expect(host.querySelector(".skinread"), "nothing to look over").toBe(null);
    expect(host.querySelector(".skinfit__answer"), "and nothing to press").toBe(null);
  });
});
