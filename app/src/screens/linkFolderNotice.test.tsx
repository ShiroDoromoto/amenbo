// @vitest-environment jsdom
// The warning a project with no folder gets on its own board (`AMB-D-533`). It carries one move and no
// other, so what is worth pinning is that the move is offered and that it is the only one.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { t } from "../core/i18n";
import { LinkFolderNotice } from "./LinkFolderNotice";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
let asked: number;

const render = () =>
  act(async () => {
    root.render(createElement(LinkFolderNotice, { onLinkFolder: () => { asked++; } }));
  });

beforeEach(() => {
  asked = 0;
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the warning about a project with no folder", () => {
  it("says what is missing and why it matters", async () => {
    await render();

    expect(container.textContent).toContain(t("noFolder.title"));
    expect(container.textContent).toContain(t("noFolder.hint"));
  });

  // What it is short of is a folder, so linking one is the whole of what it offers.
  it("offers the one move that ends it, and nothing else", async () => {
    await render();

    const buttons = [...container.querySelectorAll("button")];
    expect(buttons.map((b) => b.textContent?.trim())).toEqual([t("noFolder.btn")]);
    // The move is named by a mark as well as by its words, and the mark is the folder it asks for.
    expect(buttons[0].querySelector("svg")?.getAttribute("data-icon")).toBe("folder");

    await act(async () => { buttons[0].click(); });
    expect(asked).toBe(1);
  });
});
