// @vitest-environment jsdom
// The gate the creation screen holds: a project is raised with a folder or not at all (`AMB-D-532`).
// A project bound to none is one no AI can reach, and the screen is one of the two doors it could be
// made through — the other being the CLI, which closes it its own way.
//
// Only the boundaries are stubbed (the folder picker, the write, the name of the CLI this build
// installs); the screen's own branching runs for real, since what is under test is exactly that.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const hoisted = vi.hoisted(() => ({
  /** What the OS folder picker answers with — null is the reader backing out of it. */
  picked: null as string | null,
  /** Every create that reached the write layer, as `[name, dir]`. */
  created: [] as Array<[string, string | null]>,
}));

vi.mock("../core/snapshot", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/snapshot")>();
  return { ...orig, inTauri: () => true, subscribe: () => () => {} };
});
vi.mock("../core/cliCommand", () => ({ useCliCommandName: () => "amenbo" }));
vi.mock("../core/mutations", () => ({
  pickFolder: () => Promise.resolve(hoisted.picked),
  createProject: (name: string, dir: string | null) => {
    hoisted.created.push([name, dir]);
    return Promise.resolve(1);
  },
  revealFolder: () => Promise.resolve(),
  openTerminal: () => Promise.resolve(),
  fetchCliCommandName: () => Promise.resolve("amenbo"),
}));

import { t } from "../core/i18n";
import { NewProjectScreen } from "./NewProjectScreen";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

const render = () =>
  act(async () => {
    root.render(createElement(NewProjectScreen, { onCreated: () => {}, onCancel: () => {} }));
  });

function button(label: string): HTMLButtonElement {
  const found = [...container.querySelectorAll("button")].find((b) => b.textContent?.includes(label));
  if (!found) throw new Error(`no button labelled ${label}`);
  return found as HTMLButtonElement;
}
const createButton = () => button(t("newproj.create"));

/** Type a name into the only text field on the form. */
async function name(value: string) {
  const input = container.querySelector("input")!;
  await act(async () => {
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
    setter.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

beforeEach(() => {
  hoisted.picked = null;
  hoisted.created.length = 0;
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("raising a project on the desktop", () => {
  // The whole of the decision, read off the button: a name is not enough.
  it("will not create on a name alone", async () => {
    await render();
    await name("Seedbed");

    expect(createButton().disabled).toBe(true);
    await act(async () => { createButton().click(); });
    expect(hoisted.created, "and the press does nothing either").toEqual([]);
  });

  // Why it is shut is on the screen next to what would open it — what the folder is for, and so what
  // choosing one buys.
  it("says what the folder is for, where the folder is asked for", async () => {
    await render();

    expect(container.textContent).toContain(t("newproj.folderHint"));
  });

  it("creates with the folder once one is chosen", async () => {
    hoisted.picked = "/w/seedbed";
    await render();
    await name("Seedbed");
    await act(async () => { button(t("newproj.chooseFolder")).click(); });

    expect(container.textContent).toContain("/w/seedbed");
    expect(createButton().disabled).toBe(false);

    await act(async () => { createButton().click(); });
    expect(hoisted.created).toEqual([["Seedbed", "/w/seedbed"]]);
  });

  // Clearing would put the form back in the one state it cannot be created from.
  it("offers a change of folder and no way to unset it", async () => {
    hoisted.picked = "/w/seedbed";
    await render();
    await act(async () => { button(t("newproj.chooseFolder")).click(); });

    expect(() => button(t("newproj.changeFolder"))).not.toThrow();
    // Read off the row rather than off a label an unset would carry: naming a label the screen does
    // not have asserts nothing, since the row could grow any other button and still pass.
    const folder = container.querySelector(".newproj__folder")!;
    expect([...folder.querySelectorAll("button")].map((b) => b.textContent)).toEqual([
      t("newproj.changeFolder"),
    ]);
  });

  // Backing out of the picker is an answer, and it leaves the form where it was.
  it("stays shut when the picker is backed out of", async () => {
    await render();
    await name("Seedbed");
    await act(async () => { button(t("newproj.chooseFolder")).click(); });

    expect(createButton().disabled).toBe(true);
  });
});
