// @vitest-environment jsdom
// What an empty slot says about work nothing is doing any more, and the two things it must not do:
// ask before there is a project to ask about, and act on the answer.
//
// The host's read is stubbed and everything else runs — the point of the component is entirely in
// which of the two shapes it draws and what pressing them does.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { TaskCardDto } from "../bindings/bindings";
import { RefNavProvider } from "../core/refNav";
import { AdriftSlot } from "./AdriftSlot";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const hoisted = vi.hoisted(() => ({
  /** What the host answers with, and which folders it was asked about. */
  adrift: [] as unknown[],
  asked: [] as string[],
}));

vi.mock("../core/mutations", () => ({
  fetchAdriftTasks: vi.fn(async (folder: string) => {
    hoisted.asked.push(folder);
    return hoisted.adrift;
  }),
}));

/** A card, with only the fields this slot draws filled in. */
function card(id: number, title: string): TaskCardDto {
  return { id, ref: `#${id}`, title } as unknown as TaskCardDto;
}

let container: HTMLDivElement;
let root: Root;
const opened: number[] = [];
const ledger: number[] = [];

beforeEach(() => {
  hoisted.adrift = [];
  hoisted.asked = [];
  opened.length = 0;
  ledger.length = 0;
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

/** Draw the slot, with the way to the ledger and the way to a task both counted. */
async function draw(folder: string | null): Promise<void> {
  await act(async () => {
    root.render(
      createElement(
        RefNavProvider,
        {
          value: { selectTask: (id: number) => { opened.push(id); } },
          children: createElement(AdriftSlot, {
            folder,
            onOpen: () => {},
            onOpenLedger: () => ledger.push(1),
          }),
        },
      ),
    );
  });
}

function buttons(): HTMLButtonElement[] {
  return [...container.querySelectorAll("button")];
}

describe("an empty slot with nothing to ask about", () => {
  it("is the plain way to open a terminal", async () => {
    await draw("/work/here");

    expect(buttons()).toHaveLength(1);
    expect(container.querySelector(".slot--empty"), "the plain slot was not drawn").toBeTruthy();
  });

  it("asks the host nothing about a page that has no folder yet", async () => {
    await draw(null);

    expect(hoisted.asked, "a page with no project was asked about anyway").toEqual([]);
    expect(container.querySelector(".slot--empty")).toBeTruthy();
  });
});

describe("an empty slot with work nothing is doing any more", () => {
  it("puts the question and the tasks, and keeps the way to open a terminal", async () => {
    hoisted.adrift = [card(11, "the migration"), card(12, "the sender")];
    await draw("/work/here");

    expect(hoisted.asked).toEqual(["/work/here"]);
    expect(container.textContent).toContain("Carry on with it?");
    expect(container.textContent).toContain("the migration");
    // The way in is still there: an empty slot is somewhere to start a terminal whether or not there
    // is anything to be asked about.
    expect(buttons().some((b) => b.textContent === "Open a terminal here")).toBe(true);
  });

  it("opens the task on the other face rather than moving it", async () => {
    hoisted.adrift = [card(11, "the migration")];
    await draw("/work/here");

    const task = buttons().find((b) => b.textContent?.includes("the migration"));
    expect(task, "the task was not pressable").toBeTruthy();
    await act(async () => { task?.click(); });

    // The ledger first, then the task on it — a click that selected without switching would land on a
    // face the reader cannot see.
    expect(ledger, "the ledger was not brought up").toEqual([1]);
    expect(opened).toEqual([11]);
  });
});
