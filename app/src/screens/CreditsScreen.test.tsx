// @vitest-environment jsdom
// The credits screen. The license files are read through `fetchLicenses`, which is the one boundary
// replaced here; the cutting of the notices and the drawing are the real thing.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { LicensesDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  /** What `fetchLicenses` answers: the files, null outside Tauri, or an Error to reject with. */
  answer: null as LicensesDto | null | Error,
}));

vi.mock("../core/mutations", () => ({
  fetchLicenses: () =>
    hoisted.answer instanceof Error ? Promise.reject(hoisted.answer) : Promise.resolve(hoisted.answer),
}));

import { CreditsScreen, splitNotices } from "./CreditsScreen";
import { t } from "../core/i18n";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const RULE = "=".repeat(78);
const DASH = "-".repeat(78);

/** A notices file in the shape the generator writes, small enough to read in a test. */
const NOTICES = [
  "THIRD-PARTY NOTICES",
  "",
  "Amenbo is licensed under Apache-2.0 (see LICENSE).",
  "",
  RULE,
  "Rust crates",
  RULE,
  "",
  "adler2 — 0BSD OR MIT OR Apache-2.0",
  "anyhow — MIT OR Apache-2.0",
  "",
  RULE,
  "License texts — Rust crates",
  RULE,
  "",
  "Used by: adler2, anyhow",
  DASH,
  "                              Apache License",
  "TERMS AND CONDITIONS",
  "",
  "Used by: aho-corasick",
  DASH,
  "This is free and unencumbered software released into the public domain.",
  "",
].join("\n");

let container: HTMLDivElement;
let root: Root;
let backs = 0;

async function render() {
  await act(async () => {
    root.render(createElement(CreditsScreen, { onBack: () => { backs += 1; } }));
  });
}

function buttonByLabel(label: string): HTMLButtonElement | null {
  return ([...container.querySelectorAll("button")].find((b) => b.textContent?.trim() === label) ?? null) as HTMLButtonElement | null;
}

async function press(label: string) {
  const btn = buttonByLabel(label);
  if (!btn) throw new Error(`no button labelled ${label}`);
  await act(async () => btn.click());
}

beforeEach(() => {
  hoisted.answer = { license: "Apache License\nVersion 2.0", thirdPartyNotices: NOTICES };
  backs = 0;
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("splitNotices", () => {
  it("cuts the file into its sections, and each license-text section into its entries", () => {
    const notices = splitNotices(NOTICES);
    expect(notices.preamble).toBe("THIRD-PARTY NOTICES\n\nAmenbo is licensed under Apache-2.0 (see LICENSE).");
    expect(notices.sections.map((s) => s.title)).toEqual(["Rust crates", "License texts — Rust crates"]);

    const [list, texts] = notices.sections;
    expect(list.intro).toBe("adler2 — 0BSD OR MIT OR Apache-2.0\nanyhow — MIT OR Apache-2.0");
    expect(list.entries).toEqual([]);

    expect(texts.intro).toBe("");
    expect(texts.entries).toEqual([
      { title: "Used by: adler2, anyhow", text: "                              Apache License\nTERMS AND CONDITIONS" },
      { title: "Used by: aho-corasick", text: "This is free and unencumbered software released into the public domain." },
    ]);
  });

  it("leaves the headings and rules a license text draws inside itself in that text", () => {
    // LLVM's license carries `=` headings of the generator's shape, and libm's a `-` rule.
    const inner = [RULE, "Software from third parties included in the LLVM Project:", RULE, "", "SOFTWARE.", DASH, "more"];
    const notices = splitNotices([
      RULE, "npm packages", RULE, "", "llvm — Apache-2.0", "",
      RULE, "License texts — npm packages", RULE, "",
      "Used by: llvm", DASH, "Apache License", "", ...inner, "",
      "Used by: zod", DASH, "MIT", "",
    ].join("\n"));
    expect(notices.sections.map((s) => s.title)).toEqual(["npm packages", "License texts — npm packages"]);
    expect(notices.sections[1].entries).toEqual([
      { title: "Used by: llvm", text: ["Apache License", "", ...inner].join("\n") },
      { title: "Used by: zod", text: "MIT" },
    ]);
  });

  it("keeps a file with no headings whole, as the preamble", () => {
    expect(splitNotices("just text\n")).toEqual({ preamble: "just text", sections: [] });
  });
});

describe("CreditsScreen", () => {
  it("lists the sections without drawing their text until one is opened", async () => {
    await render();
    expect(buttonByLabel("Rust crates")).not.toBeNull();
    expect(buttonByLabel("License texts — Rust crates")).not.toBeNull();
    expect(container.textContent).not.toContain("anyhow — MIT OR Apache-2.0");

    await press("Rust crates");
    expect(container.textContent).toContain("anyhow — MIT OR Apache-2.0");
  });

  it("opens a license text one entry at a time", async () => {
    await render();
    await press("License texts — Rust crates");
    expect(container.textContent).toContain("Used by: aho-corasick");
    expect(container.textContent).not.toContain("released into the public domain");

    await press("Used by: aho-corasick");
    expect(container.textContent).toContain("released into the public domain");
    expect(container.textContent).not.toContain("TERMS AND CONDITIONS");
  });

  it("shows Amenbo's own license", async () => {
    await render();
    await press("LICENSE (Apache-2.0)");
    expect(container.textContent).toContain("Version 2.0");
  });

  it("says there is nothing to show where no bundle can be read", async () => {
    hoisted.answer = null;
    await render();
    expect(container.textContent).toContain(t("credits.unavailable"));
  });

  it("shows why the files could not be read", async () => {
    hoisted.answer = new Error("cannot read 'THIRD-PARTY-NOTICES'");
    await render();
    expect(container.textContent).toContain("cannot read 'THIRD-PARTY-NOTICES'");
  });

  it("goes back to settings", async () => {
    await render();
    await press(t("credits.back"));
    expect(backs).toBe(1);
  });
});
